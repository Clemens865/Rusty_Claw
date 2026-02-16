// Shared utility functions

const escapeMap = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };

export function escapeHtml(str) {
  if (!str) return '';
  return String(str).replace(/[&<>"']/g, c => escapeMap[c]);
}

export function formatTime(iso) {
  if (!iso) return '';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: 'short', day: 'numeric',
    hour: '2-digit', minute: '2-digit',
  });
}

export function formatDuration(ms) {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60000) return `${(ms / 1000).toFixed(1)}s`;
  return `${(ms / 60000).toFixed(1)}m`;
}

export function formatTokens(n) {
  if (n == null) return '0';
  if (n >= 1000000) return `${(n / 1000000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}K`;
  return String(n);
}

export function extractText(contentBlocks) {
  if (!contentBlocks) return '';
  if (typeof contentBlocks === 'string') return contentBlocks;
  if (!Array.isArray(contentBlocks)) return JSON.stringify(contentBlocks);
  return contentBlocks
    .filter(b => b.type === 'text' || typeof b === 'string')
    .map(b => (typeof b === 'string' ? b : b.text || ''))
    .join('');
}

const SECRET_KEYS = /key|token|secret|password|credential|auth/i;

export function redactSecrets(obj) {
  if (obj === null || obj === undefined) return obj;
  if (typeof obj !== 'object') return obj;
  if (Array.isArray(obj)) return obj.map(redactSecrets);
  const result = {};
  for (const [k, v] of Object.entries(obj)) {
    if (SECRET_KEYS.test(k) && typeof v === 'string' && v.length > 0) {
      result[k] = '***';
    } else {
      result[k] = redactSecrets(v);
    }
  }
  return result;
}
