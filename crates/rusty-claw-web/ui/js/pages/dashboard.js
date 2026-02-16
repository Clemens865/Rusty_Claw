// Dashboard page — overview stats, channel status, recent sessions.

import { call, on } from '../gateway.js';
import { formatTime, escapeHtml } from '../utils.js';

export function mount(app) {
  app.innerHTML = `
    <div class="page-header"><h2>Dashboard</h2></div>
    <div class="stat-grid" id="stats">
      <div class="stat-card"><div class="stat-value" id="stat-sessions">-</div><div class="stat-label">Sessions</div></div>
      <div class="stat-card"><div class="stat-value" id="stat-channels">-</div><div class="stat-label">Channels</div></div>
      <div class="stat-card"><div class="stat-value" id="stat-models">-</div><div class="stat-label">Models</div></div>
      <div class="stat-card"><div class="stat-value" id="stat-connections">-</div><div class="stat-label">WS Connections</div></div>
    </div>
    <div class="card" style="margin-bottom:16px">
      <div class="card-title">Channels</div>
      <div class="channel-grid" id="channel-grid"></div>
    </div>
    <div class="card">
      <div class="card-title">Recent Sessions</div>
      <div id="recent-sessions"></div>
    </div>
  `;

  loadData();

  const unsub = on('session.updated', () => loadSessions());
  return () => unsub();
}

async function loadData() {
  const [sessions, channels, models, health] = await Promise.allSettled([
    call('sessions.list'),
    call('channels.status'),
    call('models.list'),
    fetch('/health').then(r => r.json()),
  ]);

  if (sessions.status === 'fulfilled' && sessions.value) {
    const list = sessions.value.sessions || sessions.value || [];
    document.getElementById('stat-sessions').textContent = list.length;
    renderRecentSessions(Array.isArray(list) ? list : []);
  }

  if (channels.status === 'fulfilled' && channels.value) {
    const list = channels.value.channels || channels.value || [];
    document.getElementById('stat-channels').textContent = Array.isArray(list) ? list.length : 0;
    renderChannels(Array.isArray(list) ? list : []);
  }

  if (models.status === 'fulfilled' && models.value) {
    const list = models.value.models || models.value || [];
    document.getElementById('stat-models').textContent = Array.isArray(list) ? list.length : 0;
  }

  if (health.status === 'fulfilled' && health.value) {
    document.getElementById('stat-connections').textContent = health.value.connections ?? '-';
    const versionEl = document.getElementById('version-label');
    if (versionEl && health.value.version) {
      versionEl.textContent = `v${health.value.version}`;
    }
  }
}

async function loadSessions() {
  try {
    const data = await call('sessions.list');
    const list = data?.sessions || data || [];
    document.getElementById('stat-sessions').textContent = Array.isArray(list) ? list.length : 0;
    renderRecentSessions(Array.isArray(list) ? list : []);
  } catch { /* ignore */ }
}

function renderChannels(channels) {
  const grid = document.getElementById('channel-grid');
  if (!grid) return;
  if (channels.length === 0) {
    grid.innerHTML = '<div class="empty-state">No channels configured</div>';
    return;
  }
  grid.innerHTML = channels.map(ch => {
    const connected = ch.connected || ch.status === 'connected';
    const badgeClass = connected ? 'badge-green' : 'badge-red';
    const statusText = connected ? 'Connected' : 'Disconnected';
    return `<div class="card">
      <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:8px">
        <strong>${escapeHtml(ch.name || ch.id)}</strong>
        <span class="badge ${badgeClass}">${statusText}</span>
      </div>
      ${ch.error ? `<div style="color:var(--red);font-size:12px">${escapeHtml(ch.error)}</div>` : ''}
    </div>`;
  }).join('');
}

function renderRecentSessions(sessions) {
  const container = document.getElementById('recent-sessions');
  if (!container) return;

  const sorted = sessions.slice().sort((a, b) =>
    new Date(b.last_updated_at || 0) - new Date(a.last_updated_at || 0)
  );
  const recent = sorted.slice(0, 8);

  if (recent.length === 0) {
    container.innerHTML = '<div class="empty-state">No sessions yet</div>';
    return;
  }

  container.innerHTML = recent.map(s => `
    <div class="session-item" style="cursor:default">
      <div class="session-label">${escapeHtml(s.label || s.key?.peer_id || s.hash_key || 'Session')}</div>
      <div class="session-meta">
        ${escapeHtml(s.key?.channel || s.channel || '')}
        ${s.last_updated_at ? ` &middot; ${formatTime(s.last_updated_at)}` : ''}
      </div>
    </div>
  `).join('');
}
