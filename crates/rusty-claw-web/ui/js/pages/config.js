// Config page — collapsible JSON tree with secret redaction.

import { call } from '../gateway.js';
import { escapeHtml, redactSecrets } from '../utils.js';

export function mount(app) {
  app.innerHTML = `
    <div class="page-header"><h2>Configuration</h2></div>
    <div class="card">
      <div class="config-tree" id="config-tree">
        <div class="loading-spinner">Loading...</div>
      </div>
    </div>
  `;

  loadConfig();
}

async function loadConfig() {
  try {
    const data = await call('config.get');
    const config = data?.config || data || {};
    const redacted = redactSecrets(config);
    const container = document.getElementById('config-tree');
    if (container) {
      container.innerHTML = '';
      container.appendChild(renderValue(redacted, 0, true));
    }
  } catch (e) {
    document.getElementById('config-tree').innerHTML =
      `<div class="empty-state">Error: ${escapeHtml(e.message)}</div>`;
  }
}

function renderValue(value, depth, initialOpen) {
  if (value === null || value === undefined) {
    const span = document.createElement('span');
    span.className = 'config-null';
    span.textContent = 'null';
    return span;
  }

  if (typeof value === 'string') {
    const span = document.createElement('span');
    span.className = 'config-string';
    span.textContent = `"${value}"`;
    return span;
  }

  if (typeof value === 'number') {
    const span = document.createElement('span');
    span.className = 'config-number';
    span.textContent = String(value);
    return span;
  }

  if (typeof value === 'boolean') {
    const span = document.createElement('span');
    span.className = 'config-boolean';
    span.textContent = String(value);
    return span;
  }

  if (Array.isArray(value)) {
    if (value.length === 0) {
      const span = document.createElement('span');
      span.className = 'config-null';
      span.textContent = '[]';
      return span;
    }
    const frag = document.createDocumentFragment();
    value.forEach((item, i) => {
      const details = document.createElement('details');
      if (initialOpen) details.open = true;
      const summary = document.createElement('summary');
      summary.innerHTML = `<span class="config-key">[${i}]</span>`;
      details.appendChild(summary);
      const div = document.createElement('div');
      div.className = 'config-value';
      div.appendChild(renderValue(item, depth + 1, false));
      details.appendChild(div);
      frag.appendChild(details);
    });
    return frag;
  }

  if (typeof value === 'object') {
    const keys = Object.keys(value);
    if (keys.length === 0) {
      const span = document.createElement('span');
      span.className = 'config-null';
      span.textContent = '{}';
      return span;
    }
    const frag = document.createDocumentFragment();
    for (const key of keys) {
      const v = value[key];
      const isLeaf = v === null || typeof v !== 'object';

      if (isLeaf) {
        const div = document.createElement('div');
        div.className = 'config-value';
        div.innerHTML = `<span class="config-key">${escapeHtml(key)}</span>: `;
        div.appendChild(renderValue(v, depth + 1, false));
        frag.appendChild(div);
      } else {
        const details = document.createElement('details');
        if (initialOpen && depth < 1) details.open = true;
        const summary = document.createElement('summary');
        summary.innerHTML = `<span class="config-key">${escapeHtml(key)}</span>`;
        details.appendChild(summary);
        const div = document.createElement('div');
        div.className = 'config-value';
        div.appendChild(renderValue(v, depth + 1, false));
        details.appendChild(div);
        frag.appendChild(details);
      }
    }
    return frag;
  }

  const span = document.createElement('span');
  span.textContent = String(value);
  return span;
}
