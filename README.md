# Rusty Claw

**The full OpenClaw experience in a single Rust binary.**

Rusty Claw is a personal AI assistant gateway written in Rust, inspired by [OpenClaw](https://github.com/openclaw/openclaw). It aims for OpenClaw feature parity at 1/20th the resource cost: <50MB RAM, <2s cold start, <20MB binary, cross-compiled to x86_64, aarch64, armv7, and RISC-V.

## Status

**Phase 6 complete** — 208 tests passing, zero warnings, zero clippy. Full gateway with 30 WS methods, agent runtime with thinking tokens and image input, 10 channels, 4 providers (6+ services), 24 tools, WASM plugin sandbox, Prometheus metrics, graceful shutdown, and voice pipeline.

See [ROADMAP.md](ROADMAP.md) for upcoming phases and [COMPARISON.md](COMPARISON.md) for a detailed feature comparison with OpenClaw.

### What's Working

| Component | Status |
|-----------|--------|
| **Gateway** | Protocol v3, 30 WS methods, auth, TLS, rate limiting, Prometheus metrics, graceful shutdown |
| **Agent** | Tool-calling loop, streaming, thinking tokens, image input, personas, multi-agent spawning |
| **Channels** | Telegram, Discord, Slack, WebChat, WhatsApp, Signal, Google Chat, MS Teams, Matrix, iMessage |
| **Providers** | Anthropic (extended thinking), OpenAI (+OpenRouter, +Ollama), Google Gemini, Failover |
| **Tools** | 24 built-in (exec, files, web, memory, sessions, browser, multimedia, canvas, agents.spawn) |
| **Plugins** | 17 lifecycle hooks, PluginApi, PluginManager, WASM sandbox (wasmtime, feature-gated) |
| **Skills** | YAML definitions, hot-reload, prompt injection |
| **Security** | WS auth, SSRF protection, exec hardening, Docker sandbox, per-IP rate limiting |
| **Voice** | VAD, STT (Whisper), TTS streaming (ElevenLabs), binary WS frames, push/vad modes |
| **Context** | Token estimation, auto-compaction (LLM summarize), file_list (glob), TOOLS.md loading |
| **Production** | Structured logging (JSON/plain), config validation, enhanced health endpoint |
| **Control UI** | Embedded SPA — Dashboard, Sessions, Chat, Channels, Config |
| **CLI** | Interactive REPL, onboarding wizard, doctor, migrate, status |
| **Tests** | 208 (unit + integration), CI with GitHub Actions |

## Architecture

```
rusty_claw/
  crates/
    rusty-claw-core/        # Shared types, config, errors, protocol
    rusty-claw-gateway/     # WebSocket server (OpenClaw protocol v3)
    rusty-claw-agent/       # Agent runtime, tool loop, streaming
    rusty-claw-channels/    # Channel trait + Telegram, Discord, Slack, WebChat
    rusty-claw-providers/   # LLM providers (Anthropic, OpenAI, Google, Failover)
    rusty-claw-tools/       # 24 built-in tools (exec, fs, web, memory, browser, ...)
    rusty-claw-plugins/     # Plugin SDK + runtime (17 hooks, WASM sandbox)
    rusty-claw-cli/         # CLI entry point (rusty-claw binary)
    rusty-claw-web/         # Control UI + WebChat (embedded SPA)
    rusty-claw-browser/     # CDP browser automation (chromiumoxide)
    rusty-claw-media/       # Media pipeline
    rusty-claw-tts/         # Text-to-speech
    rusty-claw-canvas/      # Canvas/A2UI host
```

## Build

```bash
# Debug build
cargo build

# Release build (optimized for size)
cargo build --release

# Build with browser automation
cargo build --release --features browser

# Build with specific channels only
cargo build --release --no-default-features --features telegram

# Run tests
cargo test --workspace

# Run clippy
cargo clippy --workspace
```

## Run

```bash
# Show help
cargo run -- --help

# Show system status
cargo run -- status

# Start gateway
cargo run -- gateway --port 18789

# Start gateway with Control UI
cargo run -- gateway --port 18789 --ui

# Chat with agent (one-shot)
cargo run -- agent -m "Hello, Rusty Claw!"

# Chat with agent (interactive REPL)
cargo run -- agent

# Run onboarding wizard
cargo run -- onboard

# Check system health
cargo run -- doctor
```

## Performance Targets

| Metric | OpenClaw (Node.js) | Rusty Claw Target |
|--------|--------------------|-------------------|
| RAM (idle) | >1 GB | <30 MB |
| Cold start | ~5s | <1s |
| Binary size | ~200MB (npm) | <20 MB |
| Concurrent WS | ~1,000 | >10,000 |
| Dependencies | ~2,000+ npm | ~150 crates |

## Wire Protocol Compatibility

Rusty Claw implements OpenClaw's WebSocket protocol v3, so existing OpenClaw native apps (macOS, iOS, Android) can connect without modification.

## Documentation

- [PRD.md](PRD.md) — Full product requirements document
- [ROADMAP.md](ROADMAP.md) — Phase 5+ development roadmap
- [COMPARISON.md](COMPARISON.md) — Feature comparison with OpenClaw
- [WHITEPAPER.md](WHITEPAPER.md) — Technical whitepaper on Rust advantages

## License

MIT
