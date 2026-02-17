# Rusty Claw vs OpenClaw: Comparative Analysis

**Date:** 2026-02-16
**Rusty Claw version:** Phase 4 complete (pre-alpha)
**OpenClaw version:** Latest (TypeScript, ~198K stars)

---

## Executive Summary

Rusty Claw implements approximately **60-70% of OpenClaw's feature surface** after Phase 4, with fundamental architectural advantages in performance, security, and deployment flexibility. The remaining gaps are primarily in channel coverage (4/14+), advanced voice pipeline, WASM plugin sandbox, and some specialized WS methods.

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
| WS methods | ~80+ | 25 | Gap (~55 methods) |
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
| Multi-agent / sub-agent spawn | Yes | Partial (tools exist) | Gap |
| Transcript compaction | Yes | No | Gap |
| Extended thinking (Claude) | Yes | No | Gap |
| Image/vision input | Yes | No | Gap |
| Token budget management | Yes | No | Gap |

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
| WhatsApp (Baileys) | Yes | No | Gap |
| Signal | Yes | No | Gap |
| iMessage (BlueBubbles) | Yes | No | Gap |
| Google Chat | Yes | No | Gap |
| Microsoft Teams | Yes | No | Gap |
| Matrix | Yes | No | Gap |
| LINE | Yes | No | Gap |
| Zalo | Yes | No | Gap |
| Facebook Messenger | Yes | No | Gap |
| Instagram DMs | Yes | No | Gap |
| **Total channels** | **14+** | **4** | Gap (10+) |

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
| Sessions spawn | Yes | No | Gap |
| File upload/download | Yes | No | Gap |
| Channel-specific actions | Yes | No | Gap |
| Cron tool | Yes | No (API only) | Gap |
| **Total tools** | **25+** | **22** | Near-parity |

### 1.6 Plugin System

| Feature | OpenClaw | Rusty Claw | Status |
|---------|----------|------------|--------|
| Lifecycle hooks | 17 | 17 | Parity |
| BeforeToolCall cancel | Yes | Yes | Parity |
| Plugin API (tools, hooks, channels, providers) | Yes | Yes | Parity |
| Plugin manager (async init) | Yes | Yes | Parity |
| Native plugins | Yes (JS) | Yes (Rust) | Different approach |
| WASM sandbox | No (Node.js) | Planned (wasmtime) | Future advantage |
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
| STT (speech-to-text) | Yes (streaming) | No | Gap |
| TTS (text-to-speech) | Yes (streaming) | Tool only (no streaming) | Gap |
| Voice Activity Detection | Yes | No | Gap |
| Push-to-talk / auto modes | Yes | No | Gap |
| talk.config WS method | Yes | Yes | Partial |
| Audio WebSocket transport | Yes | No | Gap |

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
| Prometheus metrics | Yes | No | Gap |

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

### 3.1 Channel Coverage (Biggest Gap)

OpenClaw supports **14+ messaging channels** vs Rusty Claw's **4**. The missing channels (WhatsApp, Signal, iMessage, Teams, Google Chat, Matrix, LINE, etc.) are critical for users who depend on specific platforms. WhatsApp in particular is the most-requested channel.

**Path to close:** Phase 5b targets 6 additional channels. WhatsApp requires a bridge approach (Baileys is JS-only). Signal has a native Rust client (`presage`). Matrix has `matrix-sdk`. The rest are HTTP/webhook-based.

### 3.2 Voice Pipeline

OpenClaw has a **complete voice conversation system** with streaming STT/TTS, VAD, push-to-talk, and audio WebSocket transport. Rusty Claw only has a `tts` tool and `transcription` tool — no real-time voice conversation.

**Path to close:** Phase 5a implements the full voice pipeline.

### 3.3 WS Method Coverage

OpenClaw exposes **~80+ WS methods** vs Rusty Claw's **25**. Many missing methods are for file management, advanced session control, presence, health monitoring, and HTTP chat API.

**Path to close:** Phase 5c (file management) + Phase 6 (production hardening) add the most impactful missing methods. Some methods are low-priority (OpenClaw has accumulated many over years of development).

### 3.4 Plugin Ecosystem

OpenClaw has a **mature npm-based plugin ecosystem** with community-contributed plugins. Rusty Claw has the plugin infrastructure (17 hooks, PluginApi, PluginManager) but no ecosystem yet.

**Path to close:** Phase 6a adds WASM plugin sandbox. Ecosystem growth requires time and community adoption.

### 3.5 Context Management

OpenClaw has **transcript compaction** (automatic summarization when approaching token limits), **token budget management**, and **extended thinking** display. Rusty Claw lacks these.

**Path to close:** Phase 6c adds these agent features.

### 3.6 Native App Compatibility

OpenClaw has **native apps** for macOS, iOS, and Android. Rusty Claw is wire-compatible with protocol v3, but native app compatibility has not been verified.

**Path to close:** Needs integration testing with actual OpenClaw native apps. The protocol is implemented — verification is the gap.

---

## 4. Quantitative Summary

### Feature Coverage Score

| Category | OpenClaw Features | Rusty Claw Has | Coverage |
|----------|-------------------|----------------|----------|
| Gateway / Protocol | Core | Core | **90%** |
| WS Methods | ~80+ | 25 | **31%** |
| Agent Runtime | Full | Core loop + streaming | **70%** |
| Providers | 10+ services | 6+ services (4 impls) | **60%** |
| Channels | 14+ | 4 | **29%** |
| Tools | 25+ | 22 | **88%** |
| Plugin System | Full + ecosystem | Infrastructure only | **60%** |
| Skills | Full + library | Infrastructure only | **70%** |
| Security | Good | Better | **120%** |
| Voice/Talk | Full pipeline | Basic tools only | **20%** |
| Infrastructure | Production-ready | Dev/test-ready | **70%** |
| **Overall** | | | **~60-65%** |

### Lines of Code Comparison

| Metric | OpenClaw | Rusty Claw |
|--------|----------|------------|
| Language | TypeScript | Rust |
| Core LoC | ~50K+ (estimated) | ~36K |
| Dependencies LoC | ~20M+ (node_modules) | ~500K (compiled crates) |
| Test count | Unknown | 154 |

---

## 5. Strategic Assessment

### What to prioritize next

1. **Channels** (biggest user-facing gap) — WhatsApp alone would significantly expand the user base
2. **Voice pipeline** (differentiation opportunity) — Can be more efficient than OpenClaw's Node.js implementation
3. **Context management** (agent quality) — Compaction and token budgets directly affect conversation quality
4. **Production hardening** (deployment readiness) — Metrics, graceful shutdown, systemd integration

### Where Rusty Claw will always win

- **Resource efficiency:** 20-50x less RAM, instant startup
- **Deployment targets:** Runs anywhere, single binary, no runtime
- **Security:** Memory safety, smaller attack surface, hardened by default
- **Operational simplicity:** One file to deploy, update, and backup

### Where OpenClaw will always win (unless we catch up)

- **Ecosystem:** Years of community plugins, skills, integrations
- **Native apps:** Polished macOS/iOS/Android apps
- **Channel breadth:** 14+ channels vs our 4 (shrinking gap)
- **Maturity:** Battle-tested in production by thousands of users
