# Rusty Claw Roadmap

**Last updated:** 2026-02-16

---

## Completed Phases

### Phase 1: Foundation
Core gateway with WebSocket protocol v3, Telegram channel, Anthropic provider, 4 basic tools (exec, read/write/edit_file), session storage (JSONL), CLI scaffolding.

### Phase 2: Feature Parity Core
- **Providers:** Anthropic, OpenAI (+OpenRouter, +Ollama), Google Gemini, Failover
- **Channels:** Telegram, Discord, Slack, WebChat
- **12 tools:** exec, read/write/edit_file, web_fetch, web_search, memory_get/set/list/search, sessions_list/send
- **Gateway:** config.get/set, sessions.patch, hot-reload (ConfigWatcher), cron scheduler
- **CLI:** doctor, migrate, config get, channels status, sessions reset/delete, pairing
- **DM Pairing** system + Model Failover provider

### Phase 2.5: Control UI
- Embedded SPA (vanilla HTML/CSS/JS) via `rust-embed`
- 5 pages: Dashboard, Sessions, Chat (streaming), Channels, Config
- Protocol v3 JS client with request/response correlation and auto-reconnect

### Phase 3a: Plugin System + Multimedia
- HookRegistry with 17 lifecycle hooks (6 agent hook points, `BeforeToolCall` supports cancel)
- PluginManager with async init + deferred hook registration
- 3 multimedia tools: tts (ElevenLabs), generate_image (OpenAI/Stability), transcribe_audio (Groq/OpenAI)

### Phase 3b: Security + Skills + Canvas + Infrastructure
- **Security:** WS auth (token/password, constant-time compare), SSRF protection, exec hardening (40+ patterns, allowlist mode, Docker sandbox), per-IP rate limiting, TLS via rustls
- **Skills:** YAML skill model, SkillRegistry with hot-reload, skills.list/skills.get WS methods
- **Canvas:** Canvas protocol + session, /canvas/{session_id} WS endpoint, CanvasManager
- **Infrastructure:** Dockerfile + docker-compose, Tailscale Funnel, Node pairing protocol

### Phase 4: Browser, Hot-Reload, CLI, WS Methods, Testing
- **4a Browser:** Real chromiumoxide CDP behind `browser` feature flag, stub fallback, BrowserPool wired into ToolContext
- **4b Config Hot-Reload:** `Arc<RwLock<Config>>` throughout, config.set persists to disk, broadcasts config.changed
- **4c CLI:** Interactive REPL (`rusty-claw agent` without -m), onboarding wizard (dialoguer), enhanced status
- **4d WS Methods:** cron.list/add/remove, agent.abort/status (CancellationToken), channels.login/logout, talk.config (25 total)
- **4e Testing:** 8 gateway integration tests, 4 provider integration tests, measure.sh, CI integration job

**Current state:** 154 tests, zero warnings, zero clippy. 25 WS methods, 22 tools, 4 providers, 4 channels, 17 hooks. ~36K lines of Rust across 13 crates.

---

## Phase 5: Voice Pipeline + Remaining Channels (Next)

### 5a: Voice Pipeline
Full voice conversation support matching OpenClaw's talk mode.

- **STT streaming:** WebSocket-based audio stream from client to server, real-time transcription via Groq Whisper or local whisper.cpp
- **TTS streaming:** Chunked audio delivery (ElevenLabs streaming API, or local Piper TTS)
- **Voice Activity Detection (VAD):** Server-side silence detection for turn-taking
- **Talk mode WS methods:** `talk.start`, `talk.stop`, `talk.mode` (push-to-talk vs. auto)
- **Audio codec support:** Opus encoding/decoding for bandwidth-efficient voice transport
- **Integration:** Wire into existing `tts` and `transcription` tools, expose via gateway events

### 5b: Remaining Channels
Expand from 4 to 10+ channels to approach OpenClaw's 14.

| Channel | Approach | Priority |
|---------|----------|----------|
| WhatsApp | HTTP bridge to Baileys sidecar (Node.js) or WhatsApp Business API | P0 |
| Signal | `presage` crate (native Rust Signal client) or signal-cli bridge | P1 |
| iMessage | BlueBubbles HTTP API integration | P1 |
| Google Chat | Google Workspace API (webhook + polling) | P2 |
| Microsoft Teams | Microsoft Graph API (bot framework) | P2 |
| Matrix | `matrix-sdk` crate (native Rust) | P2 |

### 5c: File Management + Context
OpenClaw-compatible workspace file management.

- **File manager tools:** `file_list`, `file_upload`, `file_download`, `file_delete`
- **Context window management:** Automatic transcript compaction when approaching token limits
- **AGENTS.md / SOUL.md / TOOLS.md:** Auto-load workspace identity files into system prompt
- **$include directives:** Config modularity matching OpenClaw

---

## Phase 6: Plugin Ecosystem + Production Hardening

### 6a: WASM Plugin Sandbox
Move beyond native-only plugins to sandboxed WASM execution.

- **wasmtime integration:** Load `.wasm` plugin modules at runtime
- **WASI Preview 2:** Scoped filesystem/network access for plugins
- **Component Model:** Typed interfaces for tools, hooks, channels, providers
- **Hot-loading:** Add/remove/update plugins without gateway restart
- **Plugin registry:** Discover and install community plugins from a registry

### 6b: Production Hardening
Make Rusty Claw production-ready for always-on deployments.

- **Graceful shutdown:** Drain active connections, finish pending agent runs
- **Structured logging:** JSON log output, configurable levels per crate
- **Metrics export:** Prometheus-compatible metrics (message throughput, latency, memory)
- **Health checks:** Deeper health probes (provider connectivity, channel status, disk space)
- **Crash recovery:** Persist in-flight agent state, resume on restart
- **Systemd unit file:** Ready-made service configuration
- **Automatic updates:** Self-update mechanism (GitHub releases + binary replacement)

### 6c: Advanced Agent Features
Close the remaining agent capability gaps.

- **Multi-agent orchestration:** `sessions_spawn` for sub-agent tasks with lineage tracking
- **Thinking/reasoning tokens:** Extended thinking support for Claude, o1-style reasoning display
- **Image input:** Pass images to vision-capable models (already in protocol, needs tool + provider wiring)
- **Streaming tool results:** Progressive tool output for long-running operations
- **Agent personas:** Per-session personality/instruction switching

---

## Phase 7: Mobile, Embedded, and Edge Targets

### 7a: Cross-Compilation Matrix
Verify and optimize for all target platforms.

| Target | Status | Notes |
|--------|--------|-------|
| x86_64-unknown-linux-gnu | CI | Primary |
| aarch64-unknown-linux-gnu | CI | Raspberry Pi 4/5, ARM servers |
| armv7-unknown-linux-gnueabihf | Planned | Raspberry Pi 3, older ARM |
| riscv64gc-unknown-linux-gnu | Planned | RISC-V SBCs |
| x86_64-apple-darwin | Dev | macOS Intel |
| aarch64-apple-darwin | Dev | macOS Apple Silicon |
| x86_64-pc-windows-msvc | Planned | Windows |
| wasm32-wasi | Stretch | Cloudflare Workers, edge compute |

### 7b: Mobile Library
Expose Rusty Claw as a library for mobile apps.

- **C FFI layer:** Expose core gateway/agent functions via `extern "C"`
- **Swift bindings:** For iOS integration
- **Kotlin/JNI bindings:** For Android integration
- **Embedded mode:** Run gateway in-process (no network, direct function calls)

### 7c: Embedded Optimization
Optimize for resource-constrained targets.

- **Single-threaded runtime:** `tokio::runtime::Builder::new_current_thread()` via feature flag
- **Minimal feature set:** `--no-default-features --features telegram,anthropic` for <5MB binary
- **Static linking:** musl libc for truly static binaries
- **Memory profiling:** Validate <30MB idle on Raspberry Pi Zero 2 W

---

## Phase 8: Ecosystem and Community

### 8a: Documentation
- API reference (auto-generated from Rust doc comments)
- Getting Started guide
- Channel setup guides (one per channel)
- Plugin development guide
- Migration guide (OpenClaw to Rusty Claw)

### 8b: Community
- Plugin template repository
- Skill marketplace (curated YAML skills)
- Discord/Matrix community server
- Contribution guide + good-first-issues

### 8c: v1.0 Release Criteria
- [ ] All 14+ OpenClaw channels working
- [ ] WASM plugin sandbox stable
- [ ] Native app compatibility verified (macOS, iOS, Android)
- [ ] Security audit completed
- [ ] Performance targets met on all platforms
- [ ] Documentation site live
- [ ] <20MB binary, <50MB RAM, <2s cold start

---

## Feature Dependency Graph

```
Phase 5a (Voice) ────────────┐
Phase 5b (Channels) ─────────┤
Phase 5c (File Mgmt) ────────┤
                              ├──→ Phase 6a (WASM Plugins)
                              ├──→ Phase 6b (Production)
                              ├──→ Phase 6c (Agent Features)
                              │
                              ├──→ Phase 7a (Cross-compile)
                              ├──→ Phase 7b (Mobile)
                              ├──→ Phase 7c (Embedded)
                              │
                              └──→ Phase 8 (v1.0)
```

Phases 5a/5b/5c are independent and can be worked in parallel. Phase 6 builds on the expanded surface area. Phase 7 is independent of 6 and can run in parallel. Phase 8 gates on all prior phases.
