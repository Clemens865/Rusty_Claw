# Rusty Claw

**The full OpenClaw experience in a single Rust binary.**

Rusty Claw is a personal AI assistant gateway written in Rust, inspired by [OpenClaw](https://github.com/openclaw/openclaw). It aims for OpenClaw feature parity at 1/20th the resource cost: <50MB RAM, <2s cold start, <20MB binary, cross-compiled to x86_64, aarch64, armv7, and RISC-V.

## Status

**Pre-alpha** — project scaffolding and architecture only. See [PRD.md](PRD.md) for the full product requirements document.

## Architecture

```
rusty_claw/
  crates/
    rusty-claw-core/        # Shared types, config, errors, protocol
    rusty-claw-gateway/     # WebSocket server (OpenClaw protocol v3)
    rusty-claw-agent/       # Agent runtime, tool loop, streaming
    rusty-claw-channels/    # Channel trait + Telegram, Discord, Slack, ...
    rusty-claw-providers/   # LLM providers (Anthropic, OpenAI, Google, ...)
    rusty-claw-tools/       # Built-in tools (exec, fs, web, memory, ...)
    rusty-claw-plugins/     # Plugin SDK + runtime
    rusty-claw-cli/         # CLI entry point (rusty-claw binary)
    rusty-claw-web/         # Control UI + WebChat
    rusty-claw-browser/     # CDP browser automation
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

# Build with specific channels only
cargo build --release --no-default-features --features telegram
```

## Run

```bash
# Show help
cargo run -- --help

# Show status
cargo run -- status

# Start gateway (not yet implemented)
cargo run -- gateway --port 18789

# Chat with agent (not yet implemented)
cargo run -- agent -m "Hello, Rusty Claw!"
```

## Performance Targets

| Metric | OpenClaw (Node.js) | Rusty Claw Target |
|--------|--------------------|-------------------|
| RAM (idle) | >1 GB | <30 MB |
| Cold start | ~5s | <1s |
| Binary size | ~200MB | <20 MB |
| Concurrent WS | ~1000 | >10,000 |

## Wire Protocol Compatibility

Rusty Claw implements OpenClaw's WebSocket protocol v3, so existing OpenClaw native apps (macOS, iOS, Android) can connect without modification.

## License

MIT
