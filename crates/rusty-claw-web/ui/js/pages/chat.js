// Chat page — send messages to the agent, display streaming responses.

import { call, on } from '../gateway.js';
import { escapeHtml } from '../utils.js';

let isRunning = false;
let streamingText = '';
let unsubs = [];

export function mount(app) {
  app.innerHTML = `
    <div class="page-header"><h2>Chat</h2></div>
    <div class="chat-container">
      <div class="chat-messages" id="chat-messages">
        <div class="empty-state"><h3>Start a conversation</h3><p>Send a message to chat with the agent.</p></div>
      </div>
      <div class="chat-input-area">
        <textarea id="chat-input" placeholder="Type a message... (Ctrl+Enter to send)" rows="1"></textarea>
        <button class="btn btn-primary" id="chat-send">Send</button>
      </div>
    </div>
  `;

  const input = document.getElementById('chat-input');
  const sendBtn = document.getElementById('chat-send');

  input.addEventListener('keydown', e => {
    if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      sendMessage();
    }
  });

  // Auto-resize textarea
  input.addEventListener('input', () => {
    input.style.height = 'auto';
    input.style.height = Math.min(input.scrollHeight, 120) + 'px';
  });

  sendBtn.addEventListener('click', sendMessage);

  // Subscribe to agent events
  unsubs.push(on('agent.event', handleAgentEvent));

  return () => {
    unsubs.forEach(fn => fn());
    unsubs = [];
    isRunning = false;
  };
}

function sendMessage() {
  const input = document.getElementById('chat-input');
  const text = input.value.trim();
  if (!text || isRunning) return;

  input.value = '';
  input.style.height = 'auto';

  // Clear empty state if first message
  const messages = document.getElementById('chat-messages');
  if (messages.querySelector('.empty-state')) {
    messages.innerHTML = '';
  }

  // Add user bubble
  appendBubble('user', escapeHtml(text));

  // Start agent run
  isRunning = true;
  streamingText = '';
  updateSendButton();

  call('agent', { message: text })
    .then(() => {
      isRunning = false;
      updateSendButton();
    })
    .catch(err => {
      isRunning = false;
      updateSendButton();
      appendBubble('error', `Error: ${escapeHtml(err.message || 'Unknown error')}`);
    });
}

function handleAgentEvent(payload) {
  if (!payload) return;
  const type = payload.type;

  switch (type) {
    case 'partial_reply': {
      if (!streamingText) {
        // Create streaming bubble
        appendBubble('assistant streaming', '', 'streaming-bubble');
      }
      streamingText += payload.delta || '';
      const bubble = document.getElementById('streaming-bubble');
      if (bubble) bubble.textContent = streamingText;
      scrollToBottom();
      break;
    }

    case 'block_reply': {
      // Finalize streaming or create new bubble
      const existing = document.getElementById('streaming-bubble');
      if (existing) {
        existing.removeAttribute('id');
        existing.parentElement.classList.remove('streaming');
        if (payload.text) existing.textContent = payload.text;
      } else if (payload.text) {
        appendBubble('assistant', escapeHtml(payload.text));
      }
      if (payload.is_final) {
        streamingText = '';
      }
      break;
    }

    case 'reasoning': {
      const messages = document.getElementById('chat-messages');
      // Find or create thinking section
      let thinkEl = messages.querySelector('.thinking-current');
      if (!thinkEl) {
        const bubble = document.createElement('div');
        bubble.className = 'chat-bubble assistant';
        bubble.innerHTML = '<details open><summary style="cursor:pointer;color:var(--text-muted);font-size:12px">Thinking...</summary><div class="thinking thinking-current"></div></details>';
        messages.appendChild(bubble);
        thinkEl = bubble.querySelector('.thinking-current');
      }
      thinkEl.textContent += payload.text || '';
      scrollToBottom();
      break;
    }

    case 'tool_call': {
      // Finalize any thinking section
      finalizeThinking();
      const params = payload.params ? JSON.stringify(payload.params, null, 2) : '';
      appendBubble('tool', `<details><summary><strong>Tool:</strong> ${escapeHtml(payload.tool)}</summary><pre>${escapeHtml(params)}</pre></details>`);
      break;
    }

    case 'tool_result': {
      const cls = payload.is_error ? 'style="color:var(--red)"' : '';
      const preview = (payload.content || '').length > 500
        ? payload.content.slice(0, 500) + '...'
        : payload.content || '';
      appendBubble('tool', `<details><summary ${cls}><strong>Result:</strong> ${escapeHtml(payload.tool)}${payload.is_error ? ' (error)' : ''}</summary><pre>${escapeHtml(preview)}</pre></details>`);
      break;
    }

    case 'usage': {
      const el = document.createElement('div');
      el.style.cssText = 'font-size:11px;color:var(--text-muted);text-align:right;margin-bottom:12px';
      el.textContent = `Tokens: ${payload.input_tokens || 0} in / ${payload.output_tokens || 0} out`;
      document.getElementById('chat-messages')?.appendChild(el);
      scrollToBottom();
      break;
    }

    case 'error': {
      appendBubble('error', `${escapeHtml(payload.kind || 'Error')}: ${escapeHtml(payload.message)}`);
      isRunning = false;
      updateSendButton();
      break;
    }
  }
}

function appendBubble(cls, html, id) {
  const messages = document.getElementById('chat-messages');
  if (!messages) return;
  const bubble = document.createElement('div');
  bubble.className = `chat-bubble ${cls}`;
  const content = document.createElement('div');
  if (id) content.id = id;
  content.innerHTML = html;
  bubble.appendChild(content);
  messages.appendChild(bubble);
  scrollToBottom();
}

function scrollToBottom() {
  const messages = document.getElementById('chat-messages');
  if (messages) messages.scrollTop = messages.scrollHeight;
}

function updateSendButton() {
  const btn = document.getElementById('chat-send');
  if (btn) {
    btn.disabled = isRunning;
    btn.textContent = isRunning ? 'Running...' : 'Send';
  }
}

function finalizeThinking() {
  const el = document.querySelector('.thinking-current');
  if (el) el.classList.remove('thinking-current');
}
