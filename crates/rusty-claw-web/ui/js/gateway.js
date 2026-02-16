// Protocol v3 WebSocket client with request/response correlation and auto-reconnect.

let ws = null;
let reqId = 0;
const pending = new Map();   // id -> { resolve, reject, timer }
const listeners = new Map();  // event -> Set<handler>
let reconnectAttempt = 0;
let reconnectTimer = null;

export const state = {
  connected: false,
  connId: null,
  version: null,
};

export function connect() {
  if (ws && ws.readyState <= WebSocket.OPEN) return;

  const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
  const url = `${proto}//${location.host}/ws`;

  updateStatus('connecting');
  ws = new WebSocket(url);

  ws.onopen = () => {
    reconnectAttempt = 0;
  };

  ws.onmessage = (evt) => {
    let frame;
    try { frame = JSON.parse(evt.data); } catch { return; }

    if (frame.type === 'event') {
      if (frame.event === 'hello' && frame.payload) {
        state.connected = true;
        state.connId = frame.payload.server?.conn_id;
        state.version = frame.payload.server?.version;
        updateStatus('connected');
        emit('hello', frame.payload);
      } else {
        emit(frame.event, frame.payload);
      }
    } else if (frame.type === 'res') {
      const p = pending.get(frame.id);
      if (p) {
        clearTimeout(p.timer);
        pending.delete(frame.id);
        if (frame.ok) {
          p.resolve(frame.payload);
        } else {
          p.reject(frame.error || { code: 'unknown', message: 'Request failed' });
        }
      }
    }
  };

  ws.onclose = () => {
    state.connected = false;
    state.connId = null;
    updateStatus('disconnected');
    // Reject all pending requests
    for (const [id, p] of pending) {
      clearTimeout(p.timer);
      p.reject({ code: 'disconnected', message: 'Connection lost' });
    }
    pending.clear();
    scheduleReconnect();
  };

  ws.onerror = () => {
    // onclose will fire after this
  };
}

export function call(method, params = undefined, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    if (!ws || ws.readyState !== WebSocket.OPEN) {
      return reject({ code: 'not_connected', message: 'Not connected' });
    }
    const id = `ui-${++reqId}`;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject({ code: 'timeout', message: `Request ${method} timed out` });
    }, timeoutMs);

    pending.set(id, { resolve, reject, timer });

    ws.send(JSON.stringify({
      type: 'req',
      id,
      method,
      params: params ?? undefined,
    }));
  });
}

export function on(event, handler) {
  if (!listeners.has(event)) listeners.set(event, new Set());
  listeners.get(event).add(handler);
  return () => listeners.get(event)?.delete(handler);
}

function emit(event, payload) {
  const handlers = listeners.get(event);
  if (handlers) {
    for (const h of handlers) {
      try { h(payload); } catch (e) { console.error(`Event handler error (${event}):`, e); }
    }
  }
}

function scheduleReconnect() {
  if (reconnectTimer) return;
  const delay = Math.min(1000 * Math.pow(2, reconnectAttempt), 30000);
  reconnectAttempt++;
  reconnectTimer = setTimeout(() => {
    reconnectTimer = null;
    connect();
  }, delay);
}

function updateStatus(status) {
  const dot = document.getElementById('ws-status');
  const label = document.getElementById('ws-label');
  if (!dot || !label) return;

  dot.className = `status-dot ${status}`;
  if (status === 'connected') {
    label.textContent = `Connected${state.version ? ` (v${state.version})` : ''}`;
  } else if (status === 'connecting') {
    label.textContent = 'Connecting...';
  } else {
    label.textContent = 'Disconnected';
  }
}
