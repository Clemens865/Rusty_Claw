// Channels page — status grid with live updates.

import { call, on } from '../gateway.js';
import { escapeHtml } from '../utils.js';

export function mount(app) {
  app.innerHTML = `
    <div class="page-header"><h2>Channels</h2></div>
    <div class="channel-grid" id="channels-grid">
      <div class="loading-spinner">Loading...</div>
    </div>
  `;

  loadChannels();

  const unsub = on('session.updated', () => loadChannels());
  return () => unsub();
}

async function loadChannels() {
  try {
    const data = await call('channels.status');
    const channels = data?.channels || data || [];
    render(Array.isArray(channels) ? channels : []);
  } catch (e) {
    document.getElementById('channels-grid').innerHTML =
      `<div class="empty-state">Error: ${escapeHtml(e.message)}</div>`;
  }
}

function render(channels) {
  const grid = document.getElementById('channels-grid');
  if (!grid) return;

  if (channels.length === 0) {
    grid.innerHTML = '<div class="empty-state"><h3>No channels</h3><p>Configure channels in your config file.</p></div>';
    return;
  }

  grid.innerHTML = channels.map(ch => {
    const connected = ch.connected || ch.status === 'connected';
    const dotClass = connected ? 'connected' : 'disconnected';
    const badgeClass = connected ? 'badge-green' : 'badge-red';
    const statusText = connected ? 'Connected' : 'Disconnected';
    const label = ch.label || ch.name || ch.id || 'Unknown';
    const type = ch.type || ch.id || '';

    return `<div class="card">
      <div style="display:flex;align-items:center;gap:8px;margin-bottom:12px">
        <span class="status-dot ${dotClass}"></span>
        <strong>${escapeHtml(label)}</strong>
      </div>
      <div style="display:flex;gap:8px;align-items:center;margin-bottom:8px">
        <span class="badge">${escapeHtml(type)}</span>
        <span class="badge ${badgeClass}">${statusText}</span>
      </div>
      ${ch.error ? `<div style="color:var(--red);font-size:12px;margin-top:8px">${escapeHtml(ch.error)}</div>` : ''}
      ${ch.users != null ? `<div style="font-size:12px;color:var(--text-muted);margin-top:8px">Users: ${ch.users}</div>` : ''}
    </div>`;
  }).join('');
}
