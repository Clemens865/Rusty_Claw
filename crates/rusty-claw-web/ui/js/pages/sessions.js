// Sessions page — list + detail/transcript viewer.

import { call, on } from '../gateway.js';
import { escapeHtml, formatTime, extractText } from '../utils.js';

let allSessions = [];
let selectedHash = null;

export function mount(app) {
  app.innerHTML = `
    <div class="page-header">
      <h2>Sessions</h2>
    </div>
    <div class="toolbar">
      <input type="text" class="search-input" id="session-search"
             placeholder="Filter sessions..." style="max-width:300px;margin-bottom:0">
    </div>
    <div class="session-layout">
      <div class="session-list" id="session-list">
        <div class="loading-spinner">Loading...</div>
      </div>
      <div class="session-detail" id="session-detail">
        <div class="empty-state"><h3>Select a session</h3><p>Choose a session from the list to preview its transcript.</p></div>
      </div>
    </div>
  `;

  document.getElementById('session-search').addEventListener('input', renderList);
  loadSessions();

  const unsub = on('session.updated', () => loadSessions());
  return () => { unsub(); selectedHash = null; };
}

async function loadSessions() {
  try {
    const data = await call('sessions.list');
    allSessions = data?.sessions || data || [];
    if (!Array.isArray(allSessions)) allSessions = [];
    allSessions.sort((a, b) =>
      new Date(b.last_updated_at || 0) - new Date(a.last_updated_at || 0)
    );
    renderList();
  } catch (e) {
    document.getElementById('session-list').innerHTML =
      `<div class="empty-state">Error: ${escapeHtml(e.message)}</div>`;
  }
}

function renderList() {
  const container = document.getElementById('session-list');
  const query = (document.getElementById('session-search')?.value || '').toLowerCase();

  const filtered = allSessions.filter(s => {
    const label = (s.label || s.key?.peer_id || '').toLowerCase();
    const channel = (s.key?.channel || '').toLowerCase();
    return label.includes(query) || channel.includes(query);
  });

  if (filtered.length === 0) {
    container.innerHTML = '<div class="empty-state">No sessions found</div>';
    return;
  }

  container.innerHTML = filtered.map(s => {
    const hash = s.hash_key || s.key?.hash_key || '';
    const active = hash === selectedHash ? 'active' : '';
    return `<div class="session-item ${active}" data-hash="${escapeHtml(hash)}">
      <div class="session-label">${escapeHtml(s.label || s.key?.peer_id || 'Session')}</div>
      <div class="session-meta">
        <span class="badge badge-blue">${escapeHtml(s.key?.channel || '')}</span>
        ${s.model ? `<span class="badge">${escapeHtml(s.model)}</span>` : ''}
        ${s.last_updated_at ? ` &middot; ${formatTime(s.last_updated_at)}` : ''}
      </div>
    </div>`;
  }).join('');

  container.querySelectorAll('.session-item').forEach(el => {
    el.addEventListener('click', () => selectSession(el.dataset.hash));
  });
}

async function selectSession(hash) {
  selectedHash = hash;
  renderList();

  const detail = document.getElementById('session-detail');
  detail.innerHTML = '<div class="loading-spinner">Loading transcript...</div>';

  try {
    const data = await call('sessions.preview', { session_hash: hash });
    const entries = data?.transcript || data?.entries || data || [];
    const meta = data?.meta || allSessions.find(s => (s.hash_key || s.key?.hash_key) === hash);
    renderDetail(detail, hash, meta, Array.isArray(entries) ? entries : []);
  } catch (e) {
    detail.innerHTML = `<div class="empty-state">Error loading transcript: ${escapeHtml(e.message)}</div>`;
  }
}

function renderDetail(container, hash, meta, entries) {
  const labelText = meta?.label || meta?.key?.peer_id || 'Session';

  let toolbar = `<div class="toolbar">
    <strong id="detail-label">${escapeHtml(labelText)}</strong>
    <button class="btn btn-sm" id="btn-edit-label">Edit</button>
    <span class="spacer"></span>
    <button class="btn btn-sm" id="btn-reset">Reset</button>
    <button class="btn btn-sm btn-danger" id="btn-delete">Delete</button>
  </div>`;

  let transcript = '';
  if (entries.length === 0) {
    transcript = '<div class="empty-state">Empty transcript</div>';
  } else {
    transcript = entries.map(renderEntry).join('');
  }

  container.innerHTML = toolbar + transcript;

  // Wire up buttons
  document.getElementById('btn-edit-label')?.addEventListener('click', () => editLabel(hash, labelText));
  document.getElementById('btn-reset')?.addEventListener('click', () => resetSession(hash));
  document.getElementById('btn-delete')?.addEventListener('click', () => deleteSession(hash));
}

function renderEntry(entry) {
  const type = entry.type || 'system';
  const time = formatTime(entry.timestamp);

  if (type === 'user') {
    const text = extractText(entry.content);
    return `<div class="transcript-entry user">
      <div class="entry-role">User <span style="float:right;font-weight:400">${time}</span></div>
      <div>${escapeHtml(text)}</div>
    </div>`;
  }

  if (type === 'assistant') {
    const text = extractText(entry.content);
    return `<div class="transcript-entry assistant">
      <div class="entry-role">Assistant <span style="float:right;font-weight:400">${time}</span></div>
      <div>${escapeHtml(text)}</div>
    </div>`;
  }

  if (type === 'tool_call') {
    return `<div class="transcript-entry tool_call">
      <details>
        <summary class="entry-role">Tool Call: ${escapeHtml(entry.tool)} <span style="float:right;font-weight:400">${time}</span></summary>
        <pre>${escapeHtml(JSON.stringify(entry.params, null, 2))}</pre>
      </details>
    </div>`;
  }

  if (type === 'tool_result') {
    const cls = entry.is_error ? 'style="color:var(--red)"' : '';
    return `<div class="transcript-entry tool_result">
      <details>
        <summary class="entry-role" ${cls}>Tool Result: ${escapeHtml(entry.tool)}${entry.is_error ? ' (error)' : ''} <span style="float:right;font-weight:400">${time}</span></summary>
        <pre>${escapeHtml(entry.content)}</pre>
      </details>
    </div>`;
  }

  // System or unknown
  return `<div class="transcript-entry" style="color:var(--text-muted);font-size:12px">
    <em>${escapeHtml(entry.event || type)}</em> ${time}
  </div>`;
}

async function editLabel(hash, currentLabel) {
  const labelEl = document.getElementById('detail-label');
  if (!labelEl) return;

  const input = document.createElement('input');
  input.type = 'text';
  input.className = 'inline-input';
  input.value = currentLabel;
  labelEl.replaceWith(input);
  input.focus();
  input.select();

  const commit = async () => {
    const newLabel = input.value.trim();
    if (newLabel && newLabel !== currentLabel) {
      try {
        await call('sessions.patch', { session_hash: hash, label: newLabel });
        await loadSessions();
        if (selectedHash === hash) selectSession(hash);
      } catch { /* ignore */ }
    } else {
      const span = document.createElement('strong');
      span.id = 'detail-label';
      span.textContent = currentLabel;
      input.replaceWith(span);
    }
  };

  input.addEventListener('blur', commit);
  input.addEventListener('keydown', e => {
    if (e.key === 'Enter') { e.preventDefault(); commit(); }
    if (e.key === 'Escape') {
      const span = document.createElement('strong');
      span.id = 'detail-label';
      span.textContent = currentLabel;
      input.replaceWith(span);
    }
  });
}

async function resetSession(hash) {
  if (!confirm('Reset this session? The transcript will be cleared.')) return;
  try {
    await call('sessions.reset', { session_hash: hash });
    await loadSessions();
    if (selectedHash === hash) selectSession(hash);
  } catch { /* ignore */ }
}

async function deleteSession(hash) {
  if (!confirm('Delete this session permanently?')) return;
  try {
    await call('sessions.delete', { session_hash: hash });
    selectedHash = null;
    await loadSessions();
    document.getElementById('session-detail').innerHTML =
      '<div class="empty-state"><h3>Select a session</h3></div>';
  } catch { /* ignore */ }
}
