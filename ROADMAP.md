# Rusty Claw Roadmap

**Last updated:** 2026-02-17

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

### Phase 5: Voice Pipeline + Channels + File Management
- **5a Voice:** VAD (energy-based RMS), VoiceSession (push/vad modes), STT (pcm_to_wav + Whisper API), TTS streaming (ElevenLabs), binary WS frame handling, talk.start/stop/mode WS methods, AudioDelta agent event
- **5b Channels:** WhatsApp (Cloud API, HMAC verify), Signal (signal-cli REST), Google Chat (webhook), MS Teams (Bot Framework, OAuth2), Matrix (Client-Server API), iMessage (BlueBubbles HTTP). All feature-gated with typed configs
- **5c File+Context:** file_list tool (glob), TOOLS.md loading, token estimation, SessionConfig (max_context_tokens, auto_compact, compact_keep_recent), transcript compaction (LLM summarize old + keep recent), sessions.compact WS method

### Phase 6: Production Hardening + Agent Features + WASM
- **6b Production:** Graceful shutdown (SIGINT+SIGTERM, drain), structured logging (JSON/plain, configurable), Prometheus metrics (feature-gated), enhanced health endpoint (uptime, providers, channels), config validation (warnings+errors)
- **6c Agent:** Thinking token pass-through (Anthropic extended thinking), image input support (OpenAI+Gemini multimodal), per-session personas (custom system prompts), multi-agent spawning (agents.spawn WS method, depth limits)
- **6a WASM:** WasmPluginLoader (wasmtime engine), WasmPluginAdapter+WasmToolAdapter (Plugin+Tool trait impls), manager integration (add_wasm_plugin, CLI scanning)

**Current state:** 208 tests, zero warnings, zero clippy. 30 WS methods, 24 tools, 4 providers, 10 channels, 17 hooks, WASM sandbox. ~40K lines of Rust across 13 crates.

---

## Phase 7: Agentic Loop Depth + Multi-Agent Orchestration (Next)

### 7a: Context Overflow Auto-Recovery
Match OpenClaw's resilient agent loop.

- **Retry wrapper:** 3-attempt outer loop around agent run with error classification
- **Context overflow recovery:** Auto-compact + tool result truncation + retry on overflow
- **Tool result truncation:** Last-resort recovery for individual oversized results (head+tail format)
- **FailoverError classification:** `rate_limit`, `auth`, `billing`, `context_overflow`, `timeout`
- **Thinking level fallback:** Parse rejection error → retry with lower level
- **Auth profile rotation:** Multiple API keys per provider with cooldown tracking, round-robin

### 7b: Context Pruning Extension
Non-LLM context management (used BEFORE expensive LLM compaction).

- **Soft trim:** When context exceeds `softTrimRatio`, truncate large tool results to head+tail format
- **Hard clear:** When context exceeds `hardClearRatio`, replace tool result content with placeholder
- **Configurable:** `keepLastAssistants`, `maxChars`, `headChars`, `tailChars`, per-tool allowlist
- **Multi-stage compaction:** Chunk → summarize per chunk → merge partial summaries
- **Compaction safeguard:** Preserve post-compaction context sections and tool failures

### 7c: Advanced Sub-Agent Orchestration
Full parity with OpenClaw's multi-agent system.

- **SubagentRunRegistry:** In-memory + disk-persisted registry tracking all sub-agent runs
- **Subagent steer:** Redirect running sub-agent with new instructions (abort + re-prompt)
- **Cascade kill:** Recursively abort all descendants of a sub-agent
- **Announce flow:** Structured completion notification injected as system message to parent
- **Concurrent child limits:** `maxChildrenPerAgent` config (default 5)
- **Agent allowlist:** `subagents.allowAgents` config controls which agents can spawn
- **Run timeout:** Configurable per-sub-agent timeout with cleanup

### 7d: Multi-Agent Identity
Named agents with independent workspaces and per-channel identity.

- **Agent list config:** `agents.list[]` with id, name, workspace, model, skills, tools
- **Per-channel identity overrides:** Layered resolution (account → channel → global → agent)
- **Agent workspace isolation:** Each agent gets its own workspace directory
- **Session write locks:** `acquireSessionWriteLock()` prevents concurrent transcript corruption
- **Transcript policies:** Per-provider turn validation and repair rules (Anthropic, Gemini)

---

## Phase 8: Mobile, Embedded, and Edge Targets

### 8a: Cross-Compilation Matrix
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

### 8b: Mobile Library
Expose Rusty Claw as a library for mobile apps.

- **C FFI layer:** Expose core gateway/agent functions via `extern "C"`
- **Swift bindings:** For iOS integration
- **Kotlin/JNI bindings:** For Android integration
- **Embedded mode:** Run gateway in-process (no network, direct function calls)

### 8c: Embedded Optimization
Optimize for resource-constrained targets.

- **Single-threaded runtime:** `tokio::runtime::Builder::new_current_thread()` via feature flag
- **Minimal feature set:** `--no-default-features --features telegram,anthropic` for <5MB binary
- **Static linking:** musl libc for truly static binaries
- **Memory profiling:** Validate <30MB idle on Raspberry Pi Zero 2 W

---

## Phase 9: Ecosystem and Community

### 9a: Documentation
- API reference (auto-generated from Rust doc comments)
- Getting Started guide
- Channel setup guides (one per channel)
- Plugin development guide
- Migration guide (OpenClaw to Rusty Claw)

### 9b: Community
- Plugin template repository
- Skill marketplace (curated YAML skills)
- Discord/Matrix community server
- Contribution guide + good-first-issues

### 9c: v1.0 Release Criteria
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
Phase 1-6 (COMPLETE) ─────────┐
                               ├──→ Phase 7a (Overflow Recovery)
                               ├──→ Phase 7b (Context Pruning)
                               ├──→ Phase 7c (Sub-Agent Orchestration)
                               ├──→ Phase 7d (Multi-Agent Identity)
                               │
                               ├──→ Phase 8a (Cross-compile)
                               ├──→ Phase 8b (Mobile)
                               ├──→ Phase 8c (Embedded)
                               │
                               └──→ Phase 9 (v1.0)
```

Phase 7a-d are largely independent and can be worked in parallel. Phase 8 is independent of 7. Phase 9 gates on all prior phases.
