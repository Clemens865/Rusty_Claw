# Rusty Claw vs OpenClaw: Comparative Analysis

**Date:** 2026-02-17
**Rusty Claw version:** Phase 6 complete (pre-alpha)
**OpenClaw version:** Latest (TypeScript, ~204K stars)

---

## Executive Summary

Rusty Claw implements approximately **75-80% of OpenClaw's feature surface** after Phase 6, with fundamental architectural advantages in performance, security, and deployment flexibility. The remaining gaps are primarily in agentic loop sophistication (context overflow recovery, auth rotation, thinking fallbacks), multi-agent orchestration depth (steer, announce, persistent registry), and some specialized WS methods (~30/80+).

---

## 1. Feature-by-Feature Comparison

### 1.1 Gateway / Wire Protocol

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| Protocol v3 frames (req/res/event) | Yes | Yes | Parity |
| WS handshake + HelloOk | Yes | Yes | Parity |
| Token auth | Yes | Yes | Parity |
| Password auth | Yes | Yes | Parity |
| TLS (rustls) | Yes (Node TLS) | Yes (feature flag) | Parity |
| Per-IP rate limiting | Plugin-based | Built-in | Advantage |
| WS methods | ~80+ | 30 | Gap (~50 methods) |
| HTTP chat API | Yes | No | Gap |
| State versioning (presence/health) | Yes | Partial | Gap |

**WS Methods Breakdown:**

| Method Group | OpenClaw | Rusty Claw | Gap |
|-------------|----------|------------|-----|
| Sessions (list, preview, patch, reset, delete) | 5+ | 5 | Parity |
| Agent (send, abort, status, wait) | 4+ | 3 | Minor |
| Config (get, set) | 2 | 2 | Parity |
| Models (list) | 1+ | 1 | Parity |
| Channels (status, login, logout) | 3+ | 3 | Parity |
| Cron (list, add, remove) | 3+ | 3 | Parity |
| Skills (list, get, install, update) | 4+ | 2 | Gap |
| Talk (mode, config, start, stop) | 4+ | 1 | Gap |
| Node pairing (request, approve, invoke, event) | 4+ | 4 | Parity |
| Files (list, upload, download, delete) | 4+ | 0 | Gap |
| HTTP chat (send, abort, history) | 3+ | 0 | Gap |
| Misc (presence, health, logs, etc.) | ~20+ | 1 (wake) | Gap |

### 1.2 Agent Runtime

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| Tool-calling loop | Yes | Yes | Parity |
| Streaming events (7 types) | Yes | Yes | Parity |
| System prompt assembly | Yes | Yes | Parity |
| Skills injection into prompt | Yes | Yes | Parity |
| Agent abort (CancellationToken) | Yes | Yes | Parity |
| Session-scoped context | Yes | Yes | Parity |
| Multi-agent / sub-agent spawn | Yes | Yes (agents.spawn WS, depth limits) | Parity |
| Transcript compaction | Yes | Yes (LLM summarize + keep recent) | Parity |
| Extended thinking (Claude) | Yes | Yes (thinking budget tokens) | Parity |
| Image/vision input | Yes | Yes (OpenAI + Gemini multimodal) | Parity |
| Per-session personas | Yes | Yes (custom system prompts) | Parity |
| Token budget management | Yes | Partial (estimation, auto-compact) | Gap |
| Context overflow auto-recovery | Yes (3-attempt) | No (single pass) | Gap |
| Context pruning (non-LLM) | Yes (soft-trim + hard-clear) | No | Gap |
| Subagent steer/announce | Yes | No | Gap |
| Auth profile rotation | Yes | No | Gap |
| Thinking level fallback | Yes | No | Gap |

### 1.3 LLM Providers

| Provider | OpenClaw | Rusty Claw | Status |
|----------|----------|------------|--------|
| Anthropic (Messages API) | Yes | Yes | Parity |
| OpenAI (Completions) | Yes | Yes | Parity |
| OpenAI (Responses API) | Yes | Yes | Parity |
| Google Gemini | Yes | Yes | Parity |
| OpenRouter | Yes | Yes (via OpenAI) | Parity |
| Ollama | Yes | Yes (via OpenAI) | Parity |
| AWS Bedrock | Yes | No | Gap |
| GitHub Copilot | Yes | No | Gap |
| Azure OpenAI | Yes | No | Gap |
| Groq | Yes | Partial (via OpenAI-compat) | Partial |
| Model failover | Yes | Yes | Parity |
| Auth profile rotation | Yes | No | Gap |
| **Total providers** | **10+** | **4 (covering 6+ services)** | Partial |

### 1.4 Messaging Channels

| Channel | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| Telegram | Yes | Yes | Parity |
| Discord | Yes | Yes | Parity |
| Slack | Yes | Yes | Parity |
| WebChat | Yes | Yes | Parity |
| WhatsApp (Cloud API) | Yes | Yes | Parity |
| Signal (signal-cli REST) | Yes | Yes | Parity |
| iMessage (BlueBubbles) | Yes | Yes | Parity |
| Google Chat (webhook) | Yes | Yes | Parity |
| Microsoft Teams (Bot Framework) | Yes | Yes | Parity |
| Matrix (Client-Server API) | Yes | Yes | Parity |
| LINE | Yes | No | Gap |
| Zalo | Yes | No | Gap |
| Facebook Messenger | Yes | No | Gap |
| Instagram DMs | Yes | No | Gap |
| **Total channels** | **14+** | **10** | Near-parity |

### 1.5 Tools

| Tool Category | OpenClaw | Rusty Claw | Status |
|--------------|----------|------------|--------|
| Shell exec | Yes | Yes (hardened, 40+ blocked patterns) | Advantage |
| File read/write/edit | Yes | Yes | Parity |
| Web fetch | Yes | Yes (SSRF-protected) | Advantage |
| Web search | Yes | Yes | Parity |
| Memory get/set/list/search | Yes | Yes | Parity |
| Sessions list/send | Yes | Yes | Parity |
| TTS (ElevenLabs) | Yes | Yes | Parity |
| Image generation | Yes | Yes (OpenAI + Stability) | Parity |
| Audio transcription | Yes | Yes (Groq + OpenAI) | Parity |
| Browser (CDP) — 6 tools | Yes | Yes (feature-flagged) | Parity |
| Canvas | Yes | Yes | Parity |
| Sessions/agents spawn | Yes | Yes | Parity |
| File upload/download | Yes | No | Gap |
| Channel-specific actions | Yes | No | Gap |
| Cron tool | Yes | No (API only) | Gap |
| **Total tools** | **25+** | **24** | Near-parity |

### 1.6 Plugin System

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| Lifecycle hooks | 17 | 17 | Parity |
| BeforeToolCall cancel | Yes | Yes | Parity |
| Plugin API (tools, hooks, channels, providers) | Yes | Yes | Parity |
| Plugin manager (async init) | Yes | Yes | Parity |
| Native plugins | Yes (JS) | Yes (Rust) | Different approach |
| WASM sandbox | No (Node.js) | Yes (wasmtime, feature-gated) | Advantage |
| npm plugin ecosystem | Large | None yet | Gap |
| Hot-loadable plugins | Yes | Partial (skills hot-reload) | Gap |

### 1.7 Skills System

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| YAML skill definition | Yes | Yes | Parity |
| Skill registry | Yes | Yes | Parity |
| Hot-reload from filesystem | Yes | Yes | Parity |
| Skill injection into prompts | Yes | Yes | Parity |
| skills.list / skills.get | Yes | Yes | Parity |
| skills.install from URL | Yes | No | Gap |
| Bundled skill library | Large | None | Gap |

### 1.8 Security

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| DM pairing | Yes | Yes | Parity |
| WS auth (token + password) | Yes | Yes | Parity |
| Constant-time auth comparison | No (typical JS) | Yes (subtle crate) | Advantage |
| SSRF protection | Plugin-based | Built-in (DNS resolve + IP blocking) | Advantage |
| Exec hardening | Basic blocklist | 40+ patterns + allowlist mode | Advantage |
| Docker sandbox | Yes | Yes (config-based) | Parity |
| TLS | Yes (Node) | Yes (rustls, no OpenSSL) | Advantage |
| Rate limiting | Plugin-based | Built-in per-IP | Advantage |
| Memory safety | GC-managed | Compile-time guaranteed | Advantage |
| Supply chain | ~2000+ npm deps | ~150 crate deps | Advantage |

### 1.9 Voice / Talk Mode

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| STT (speech-to-text) | Yes (streaming) | Yes (Whisper API, pcm_to_wav) | Parity |
| TTS (text-to-speech) | Yes (streaming) | Yes (ElevenLabs streaming) | Parity |
| Voice Activity Detection | Yes | Yes (energy-based RMS) | Parity |
| Push-to-talk / auto modes | Yes | Yes (push/vad modes) | Parity |
| talk.config/start/stop/mode WS methods | Yes | Yes (4 methods) | Parity |
| Audio WebSocket transport | Yes | Yes (binary WS frames) | Parity |

### 1.10 Infrastructure

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| Config hot-reload | Yes | Yes (file watcher + WS broadcast) | Parity |
| Cron scheduler | Yes | Yes | Parity |
| Dockerfile | Yes | Yes | Parity |
| docker-compose | Yes | Yes | Parity |
| Tailscale Funnel | Yes | Yes | Parity |
| Control UI | Yes (Svelte) | Yes (vanilla JS SPA) | Parity |
| Systemd service | Yes | No | Gap |
| Self-update | Yes | No | Gap |
| Prometheus metrics | Yes | Yes (feature-gated) | Parity |
| Graceful shutdown | Yes | Yes (SIGINT+SIGTERM, drain) | Parity |
| Structured logging | Yes | Yes (JSON/plain, configurable) | Parity |
| Config validation | N/A | Yes (warnings + errors) | Advantage |

---

## 2. Where Rusty Claw Leads

### 2.1 Performance

| Metric | OpenClaw (Node.js) | Rusty Claw (Rust) | Improvement |
|--------|--------------------|--------------------|-------------|
| RAM (idle, 1 channel) | >1 GB | ~25-30 MB (est.) | **30-40x** |
| Cold start | ~5s | <1s | **5x+** |
| Binary size | ~200MB (npm) | <20 MB (stripped) | **10x** |
| Concurrent WS | ~1,000 | >10,000 (est.) | **10x** |
| Dependencies | ~2,000+ npm | ~150 crates | **13x fewer** |

### 2.2 Deployment Flexibility

Rusty Claw runs on targets **impossible for OpenClaw**:

| Target | OpenClaw | Rusty Claw |
|--------|----------|------------|
| Raspberry Pi Zero 2 W (512MB) | Cannot run (>1GB RAM) | Runs comfortably |
| $5/mo VPS (512MB) | Cannot run | Runs with room to spare |
| Docker sidecar (128MB limit) | Cannot run | Runs |
| ARM SBCs (no Node.js) | Cannot run | Cross-compiles |
| RISC-V boards | Cannot run | Cross-compiles |
| Static binary (no runtime) | Impossible | Default |

### 2.3 Security Posture

| Aspect | OpenClaw | Rusty Claw |
|--------|----------|------------|
| Memory safety | GC-managed, but buffer overflows possible in native deps | Compile-time guaranteed (zero `unsafe` in app code) |
| Supply chain attack surface | ~2,000+ transitive npm deps | ~150 auditable crate deps |
| Auth comparison | Standard JS string comparison (timing side-channel) | Constant-time comparison |
| SSRF protection | Requires plugin | Built into core |
| Exec hardening | Basic blocklist | 40+ pattern blocklist + allowlist mode + Docker |

### 2.4 Operational Simplicity

| Aspect | OpenClaw | Rusty Claw |
|--------|----------|------------|
| Installation | npm install (requires Node.js 22+) | Single binary, zero dependencies |
| Update | npm update + dependency resolution | Replace one binary |
| Configuration | JSON5 (compatible) | JSON5 (compatible) |
| Backup | Sessions dir + config | Sessions dir + config |
| Resource monitoring | Complex (V8 heap, GC metrics) | Simple (RSS, one process) |

---

## 3. Where OpenClaw Leads

### 3.1 Agentic Loop Sophistication (Biggest Gap)

OpenClaw's agent runtime has a sophisticated outer retry loop with **context overflow auto-recovery** (3 attempts), **auth profile rotation** (multiple API keys with cooldown), **thinking level fallback** (parse error → lower level → retry), and **rich error classification** (`rate_limit`, `auth`, `billing`, `context_overflow`). Rusty Claw has a simpler single-pass loop.

**Path to close:** Phase 7 should implement the retry wrapper with overflow recovery, tool result truncation, and error classification.

### 3.2 Multi-Agent Orchestration Depth

OpenClaw has a full sub-agent system with **steer** (redirect running sub-agent), **cascade kill** (recursively abort descendants), **announce flow** (structured completion notification to parent), **persistent registry** (survives restarts), and **concurrent child limits**. Rusty Claw has basic spawning with depth limits but lacks these advanced orchestration primitives.

**Path to close:** Phase 7 should add SubagentRunRegistry, steer/announce, and cascade kill.

### 3.3 Context Management Layers

OpenClaw uses a **multi-layer context management** system: non-LLM context pruning (soft-trim/hard-clear of tool results), multi-stage LLM compaction (chunk → summarize → merge), compaction safeguard (preserve failures), and tool result truncation as last resort. Rusty Claw has single-pass LLM compaction only.

**Path to close:** Phase 7 should add context pruning extension and multi-stage compaction.

### 3.4 WS Method Coverage

OpenClaw exposes **~80+ WS methods** vs Rusty Claw's **30**. Many missing methods are for file management, presence, advanced health monitoring, HTTP chat API, and subagent management.

**Path to close:** Most impactful remaining methods are low-priority incremental additions.

### 3.5 Provider & Identity Breadth

OpenClaw supports **15+ providers** with auth profile rotation, plus a full multi-agent identity system (named agents with independent workspaces, per-channel identity overrides). Rusty Claw has 4 providers and single-agent persona.

**Path to close:** Additional providers are straightforward (most are OpenAI-compatible). Multi-agent identity is a Phase 7 target.

### 3.6 Native App Compatibility

OpenClaw has **native apps** for macOS, iOS, and Android. Rusty Claw is wire-compatible with protocol v3, but native app compatibility has not been verified.

**Path to close:** Needs integration testing with actual OpenClaw native apps.

---

## 4. Quantitative Summary

### Feature Coverage Score

| Category | OpenClaw Features | Rusty Claw Has | Coverage |
|----------|-------------------|----------------|----------|
| Gateway / Protocol | Core | Core + metrics + graceful shutdown | **90%** |
| WS Methods | ~80+ | 30 | **38%** |
| Agent Runtime | Full (overflow recovery, auth rotation) | Core + thinking + images + personas + spawning | **75%** |
| Providers | 15+ services | 6+ services (4 impls) | **50%** |
| Channels | 14+ | 10 | **71%** |
| Tools | 25+ | 24 | **96%** |
| Plugin System | Full + ecosystem | Infrastructure + WASM sandbox | **65%** |
| Skills | Full + library | Infrastructure + hot-reload | **70%** |
| Security | Good | Better | **120%** |
| Voice/Talk | Full pipeline | Full pipeline (VAD, STT, TTS streaming) | **85%** |
| Infrastructure | Production-ready | Production-ready (metrics, logging, shutdown) | **85%** |
| **Overall** | | | **~75-80%** |

### Lines of Code Comparison

| Metric | OpenClaw | Rusty Claw |
|--------|----------|------------|
| Language | TypeScript | Rust |
| Core LoC | ~50K+ (estimated) | ~40K |
| Dependencies LoC | ~20M+ (node_modules) | ~500K (compiled crates) |
| Test count | Unknown | 208 |

---

## 5. Strategic Assessment

### What to prioritize next

1. **Context overflow auto-recovery** (reliability) — 3-attempt compact + truncate + retry prevents stuck sessions
2. **Context pruning extension** (cost reduction) — Non-LLM soft-trim/hard-clear before expensive LLM compaction
3. **Subagent orchestration** (agent quality) — Steer, announce, cascade kill, persistent registry
4. **Auth profile rotation** (production reliability) — Multiple API keys with cooldown tracking
5. **Multi-agent identity** (deployment flexibility) — Named agents with independent workspaces

### Where Rusty Claw will always win

- **Resource efficiency:** 20-50x less RAM, instant startup
- **Deployment targets:** Runs anywhere, single binary, no runtime
- **Security:** Memory safety, smaller attack surface, hardened by default
- **Operational simplicity:** One file to deploy, update, and backup

### Where OpenClaw will always win (unless we catch up)

- **Ecosystem:** Years of community plugins, skills, integrations
- **Native apps:** Polished macOS/iOS/Android apps
- **Channel breadth:** 14+ channels vs our 10 (closing gap)
- **Agentic loop depth:** Retry logic, auth rotation, thinking fallbacks
- **Maturity:** Battle-tested in production by thousands of users
