# Rusty Claw - Product Requirements Document

**Version:** 0.1.0-draft
**Date:** 2026-02-16
**Author:** Clemens Hoenig
**Repository:** https://github.com/Clemens865/Rusty_Claw
**Reference Implementation:** [OpenClaw](https://github.com/openclaw/openclaw) (TypeScript, ~198K stars)

---

## 1. Vision

**Rusty Claw** is a personal AI assistant gateway written in Rust, inspired by OpenClaw's architecture but built from scratch to achieve:

- **Near-zero overhead:** Single static binary, <50MB RAM baseline, <2s cold start
- **OpenClaw feature parity:** Same gateway protocol, channel ecosystem, tool system, and plugin model
- **Memory safety without GC:** Rust's ownership model eliminates the entire class of runtime memory bugs that plague Node.js at scale
- **True portability:** Cross-compile to x86_64, aarch64, armv7, and RISC-V from a single codebase — no Node.js runtime required on target

The goal is not a minimal reimplementation (that's PicoClaw). The goal is **the full OpenClaw experience at 1/20th the resource cost**, deployed as a single binary that runs everywhere from a Raspberry Pi to a cloud VM.

---

## 2. Problem Statement

OpenClaw is the leading open-source personal AI assistant, but it has fundamental deployment constraints:

| Problem | Impact |
|---------|--------|
| Requires Node.js >= 22 runtime | Can't deploy on minimal/embedded systems |
| >1GB RAM baseline | Excludes SBCs, cheap VPS, containers with tight limits |
| >500s cold start on constrained hardware | Unusable on low-power devices |
| ~20M LoC TypeScript + native deps | Complex build, fragile dependency tree |
| GC pauses under memory pressure | Latency spikes during multi-channel message bursts |
| No single-binary distribution | Requires npm ecosystem on target |

PicoClaw (Go) solved the resource problem but sacrificed most of OpenClaw's feature surface: no browser automation, no canvas, no native apps, no plugin SDK, only 8 channels vs 14+.

**Rusty Claw fills the gap: full-featured AND lightweight.**

---

## 3. Target Users

1. **Self-hosters on constrained hardware** — Raspberry Pi, NAS, cheap VPS ($5/mo), home servers
2. **Power users migrating from OpenClaw** — Want the same features with lower overhead
3. **Embedded/IoT deployments** — AI assistant on edge devices, kiosks, vehicles
4. **Privacy-conscious users** — Single binary, no npm supply chain, auditable Rust code
5. **Developers building on top** — Plugin authors, channel integrators, tool builders

---

## 4. Architecture Overview

### 4.1 High-Level Architecture

```
Chat Channels (WhatsApp, Telegram, Slack, Discord, Signal, iMessage, Teams, ...)
                    |
                    v
         +--------------------+
         |   Rusty Claw       |
         |   Gateway          |  <-- Single Rust binary
         |   (Tokio async)    |
         +---------+----------+
                   |
        +----------+----------+----------+
        v          v          v          v
    Agent       CLI Tool    WebChat   Native App
    Runtime     (subcmd)     (UI)     Protocol
    (async)                           (WS compat)
```

### 4.2 Crate Architecture

```
rusty_claw/                          # Workspace root
  crates/
    rusty-claw-core/                 # Shared types, config, errors
    rusty-claw-gateway/              # WebSocket server, session mgmt, HTTP
    rusty-claw-agent/                # Agent runtime, tool loop, streaming
    rusty-claw-channels/             # Channel trait + built-in channels
      channels/
        telegram/
        whatsapp/
        discord/
        slack/
        signal/
        imessage/
        googlechat/
        msteams/
        matrix/
        webchat/
        line/
        bluebubbles/
    rusty-claw-providers/            # LLM provider abstraction
    rusty-claw-tools/                # Built-in tool implementations
    rusty-claw-plugins/              # Plugin SDK + plugin runtime
    rusty-claw-cli/                  # CLI entry point (clap)
    rusty-claw-web/                  # Control UI + WebChat (embedded assets)
    rusty-claw-browser/              # CDP browser automation
    rusty-claw-media/                # Media pipeline (images, audio, video)
    rusty-claw-tts/                  # Text-to-speech
    rusty-claw-canvas/               # Canvas/A2UI host
```

### 4.3 Async Runtime

- **Tokio** as the async runtime (multi-threaded by default, single-threaded on constrained hardware via feature flag)
- **Tower** for middleware layering (rate limiting, auth, logging)
- **axum** for HTTP endpoints (Control UI, webhooks, API)
- **tokio-tungstenite** for WebSocket server and client connections

### 4.4 Key Design Decisions

| Decision | Rationale |
|----------|-----------|
| Workspace of crates (not monolith) | Compile only what you need; feature-gate channels |
| `async fn` everywhere | Non-blocking I/O for high channel concurrency |
| `trait Channel` not `dyn Plugin` | Static dispatch where possible, dynamic only for plugins |
| Serde for all serialization | JSON config, WS protocol, API responses |
| Feature flags per channel | `--features telegram,discord` to minimize binary size |
| Embedded static assets | Control UI + WebChat bundled in binary via `include_dir` |
| WASM plugin support (Phase 3) | Sandboxed plugins via wasmtime, replacing Node.js plugin SDK |

---

## 5. Core Subsystems

### 5.1 Gateway (Wire Protocol)

**Must be wire-compatible with OpenClaw's protocol v3** so existing native apps (macOS, iOS, Android) can connect to Rusty Claw without modification.

#### Frame Types

```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
enum GatewayFrame {
    #[serde(rename = "req")]
    Request {
        id: String,
        method: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        params: Option<serde_json::Value>,
    },
    #[serde(rename = "res")]
    Response {
        id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<ErrorShape>,
    },
    #[serde(rename = "event")]
    Event {
        event: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        payload: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
}
```

#### Connection Lifecycle

1. Client connects via WebSocket
2. Server sends `connect.challenge` event with nonce
3. Client responds with `ConnectParams` (protocol negotiation, auth, device info)
4. Server validates, sends `HelloOk` with capabilities snapshot
5. Bidirectional req/res/event messaging begins
6. Periodic `tick` events for keepalive

#### Gateway Methods (Phase 1 Priority)

| Priority | Method Group | Methods |
|----------|-------------|---------|
| P0 | Sessions | `sessions.list`, `sessions.preview`, `sessions.patch`, `sessions.reset`, `sessions.delete` |
| P0 | Agent | `agent` (send message), `agent.wait`, `wake` |
| P0 | Config | `config.get`, `config.set` |
| P1 | Channels | `channels.status`, `channels.logout` |
| P1 | Models | `models.list` |
| P1 | Cron | `cron.list`, `cron.add`, `cron.remove` |
| P2 | Nodes | `node.pair.*`, `node.invoke`, `node.event` |
| P2 | Skills | `skills.status`, `skills.install` |
| P2 | Talk | `talk.mode`, `talk.config` |
| P3 | HTTP API | `chat.send`, `chat.abort`, `chat.history` |

#### State Versioning

Monotonic counters for `presence` and `health` state, matching OpenClaw's `StateVersion` type. Clients skip stale broadcasts using these counters.

---

### 5.2 Agent Runtime

The agent runtime orchestrates LLM interactions with tool-calling loops.

#### Core Loop

```rust
pub async fn run_agent(
    session: &mut Session,
    message: InboundMessage,
    config: &AgentConfig,
    tools: &ToolRegistry,
    provider: &dyn LlmProvider,
) -> Result<AgentRunResult> {
    // 1. Build system prompt (skills, memory, identity, time)
    // 2. Append user message to session transcript
    // 3. Loop:
    //    a. Send transcript to LLM provider (streaming)
    //    b. If response contains tool_use blocks:
    //       - Execute tools through policy pipeline
    //       - Append tool results to transcript
    //       - Continue loop
    //    c. If response is text-only:
    //       - Emit block replies (chunked for progressive delivery)
    //       - Break loop
    // 4. Persist transcript, return result with usage metadata
}
```

#### Streaming

The agent runtime produces a stream of typed events:

```rust
pub enum AgentEvent {
    BlockReply { text: String, is_final: bool },
    ReasoningStream { text: String },
    ToolCall { tool: String, params: serde_json::Value },
    ToolResult { tool: String, result: ToolOutput },
    PartialReply { delta: String },
    Usage { input_tokens: u64, output_tokens: u64 },
    Error { kind: AgentErrorKind, message: String },
}
```

Events are broadcast to all connected gateway clients via the `agent.event` event frame.

#### Model Resolution & Failover

```rust
pub struct ModelResolver {
    providers: Vec<ProviderConfig>,
    fallbacks: Vec<String>,
    auth_profiles: HashMap<String, AuthProfile>,
}

impl ModelResolver {
    /// Resolve model ID to a concrete provider + credentials.
    /// Rotates auth profiles on failure, cascades through fallbacks.
    pub async fn resolve(&self, model_id: &str) -> Result<ResolvedModel>;
}
```

Auth methods: API key, OAuth (with refresh), AWS SDK (SigV4), token.

---

### 5.3 Channel Abstraction

Every messaging platform implements the `Channel` trait:

```rust
#[async_trait]
pub trait Channel: Send + Sync + 'static {
    /// Unique channel identifier (e.g., "telegram", "discord")
    fn id(&self) -> &str;

    /// Channel metadata for UI display
    fn meta(&self) -> ChannelMeta;

    /// What this channel supports
    fn capabilities(&self) -> ChannelCapabilities;

    /// Start monitoring for inbound messages.
    /// Returns a receiver for inbound messages + a handle to stop monitoring.
    async fn start(&self, config: &ChannelConfig) -> Result<(InboundReceiver, ChannelHandle)>;

    /// Send a message to a target on this channel.
    async fn send(&self, target: &SendTarget, message: OutboundMessage) -> Result<SendResult>;

    /// Channel-specific status/health
    async fn status(&self) -> ChannelStatus;
}
```

#### Channel Capabilities

```rust
pub struct ChannelCapabilities {
    pub chat_types: Vec<ChatType>,          // dm, group, channel, thread
    pub media: MediaCapabilities,           // image, audio, video, document, max sizes
    pub features: ChannelFeatureFlags,      // polls, reactions, threads, typing, read_receipts
    pub streaming: Option<StreamingConfig>, // progressive message delivery
}
```

#### Inbound Message

```rust
pub struct InboundMessage {
    pub channel: String,
    pub account_id: String,
    pub chat_type: ChatType,
    pub sender: Sender,
    pub text: Option<String>,
    pub media: Vec<MediaAttachment>,
    pub reply_to: Option<String>,
    pub thread_id: Option<String>,
    pub raw: serde_json::Value,          // platform-specific raw payload
}
```

#### Channel Priority

| Phase | Channels |
|-------|----------|
| Phase 1 | Telegram, Discord, WebChat |
| Phase 2 | WhatsApp (Baileys port or bridge), Slack, Signal |
| Phase 3 | iMessage (BlueBubbles), Google Chat, Microsoft Teams |
| Phase 4 | Matrix, LINE, Zalo, custom channels via plugin |

---

### 5.4 LLM Provider Abstraction

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Provider identifier
    fn id(&self) -> &str;

    /// Supported API protocol
    fn api(&self) -> ModelApi;

    /// Stream a chat completion.
    /// Returns an async stream of completion chunks.
    async fn stream(
        &self,
        request: &CompletionRequest,
        credentials: &Credentials,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<CompletionChunk>> + Send>>>;

    /// List available models from this provider.
    async fn list_models(&self, credentials: &Credentials) -> Result<Vec<ModelInfo>>;
}

pub enum ModelApi {
    AnthropicMessages,
    OpenAiCompletions,
    OpenAiResponses,
    GoogleGenerativeAi,
    BedrockConverseStream,
    Ollama,
    GithubCopilot,
}
```

#### Provider Priority

| Phase | Providers |
|-------|-----------|
| Phase 1 | Anthropic (Messages API), OpenAI (Completions + Responses) |
| Phase 2 | Google Gemini, OpenRouter (OpenAI-compatible), Ollama |
| Phase 3 | AWS Bedrock, GitHub Copilot, custom HTTP providers |

---

### 5.5 Tool System

#### Tool Trait

```rust
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name as exposed to the LLM
    fn name(&self) -> &str;

    /// JSON Schema for tool parameters
    fn parameters_schema(&self) -> serde_json::Value;

    /// Tool description for the LLM
    fn description(&self) -> &str;

    /// Execute the tool with given parameters
    async fn execute(
        &self,
        params: serde_json::Value,
        context: &ToolContext,
    ) -> Result<ToolOutput>;
}

pub struct ToolContext {
    pub session_key: String,
    pub workspace: PathBuf,
    pub sandbox: SandboxConfig,
    pub config: Arc<Config>,
}
```

#### Tool Policy Pipeline

Tools pass through a policy pipeline before execution:

```
Tool Request
    |
    v
[1] Profile Filter (minimal/coding/messaging/full)
    |
    v
[2] Allow/Deny Lists (from config)
    |
    v
[3] Group Tool Policies (per-group restrictions)
    |
    v
[4] Sandbox Enforcement (workspace path restrictions)
    |
    v
[5] before_tool_call Hook (plugins can block/modify)
    |
    v
Tool Execution
    |
    v
[6] after_tool_call Hook (observe result)
```

#### Built-in Tools

| Phase | Tools |
|-------|-------|
| Phase 1 | `exec` (bash), `read`, `write`, `edit`, `web_fetch`, `web_search` |
| Phase 2 | `sessions_list`, `sessions_send`, `sessions_spawn`, `memory_get`, `memory_search`, `cron` |
| Phase 3 | `browser` (CDP), `canvas`, `image_generate`, `tts` |
| Phase 4 | Channel-specific action tools (Discord actions, Slack actions, etc.) |

---

### 5.6 Session Model

#### Session Key

Sessions are identified by composite keys encoding the routing context:

```rust
pub struct SessionKey {
    pub channel: String,
    pub account_id: String,
    pub chat_type: ChatType,
    pub peer_id: String,
    pub scope: SessionScope,
}

pub enum SessionScope {
    PerSender,      // default: each sender gets own session
    Global,         // shared session across all senders
    PerPeer,        // one session per peer
}
```

#### Session Storage

Sessions are stored as append-only JSONL transcript files:

```
~/.rusty_claw/sessions/
  sessions.json                     # Session index (metadata)
  transcripts/
    <session-key-hash>.jsonl        # Transcript entries
```

Each transcript entry:

```rust
pub enum TranscriptEntry {
    UserMessage { role: String, content: Vec<ContentBlock>, timestamp: DateTime<Utc> },
    AssistantMessage { role: String, content: Vec<ContentBlock>, usage: Option<Usage> },
    ToolCall { tool: String, params: serde_json::Value },
    ToolResult { tool: String, output: ToolOutput },
    SystemEvent { event: String, data: serde_json::Value },
}
```

#### Session Metadata

```rust
pub struct SessionMeta {
    pub key: SessionKey,
    pub label: Option<String>,
    pub model: Option<String>,             // per-session model override
    pub thinking_level: ThinkingLevel,
    pub last_channel: Option<String>,
    pub last_updated_at: DateTime<Utc>,
    pub last_reset_at: Option<DateTime<Utc>>,
    pub spawned_by: Option<String>,        // sub-agent lineage
    pub spawn_depth: u32,
}
```

---

### 5.7 Config System

#### Config File: `~/.rusty_claw/config.json`

JSON5 format (via `json5` crate), matching OpenClaw's structure for migration compatibility:

```rust
#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub agents: Option<AgentsConfig>,
    pub models: Option<ModelsConfig>,
    pub channels: Option<ChannelsConfig>,
    pub tools: Option<ToolsConfig>,
    pub gateway: Option<GatewayConfig>,
    pub session: Option<SessionConfig>,
    pub cron: Option<CronConfig>,
    pub browser: Option<BrowserConfig>,
    pub audio: Option<AudioConfig>,
    pub hooks: Option<HooksConfig>,
    pub plugins: Option<PluginsConfig>,
    pub skills: Option<SkillsConfig>,
    pub memory: Option<MemoryConfig>,
    pub logging: Option<LoggingConfig>,
    pub ui: Option<UiConfig>,
    pub web: Option<WebConfig>,
    // ... remaining sections
}
```

#### Config Loading

1. Read `~/.rusty_claw/config.json` (or `RUSTY_CLAW_CONFIG` env override)
2. Process `$include` directives for modular config
3. Substitute `${ENV_VAR}` references
4. Deserialize + validate with serde
5. Apply runtime defaults
6. Support hot-reload via file watcher (`notify` crate)

#### OpenClaw Config Migration

Provide a one-time migration tool:

```bash
rusty-claw migrate --from ~/.openclaw/openclaw.json
```

This reads an OpenClaw config and produces a compatible Rusty Claw config, mapping channel tokens, provider keys, and session settings.

---

### 5.8 Plugin System

#### Phase 1: Native Rust Plugins (compiled in)

```rust
pub trait Plugin: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn register(&self, api: &mut PluginApi);
}

pub struct PluginApi {
    pub fn register_tool(&mut self, tool: Box<dyn Tool>);
    pub fn register_hook(&mut self, event: HookEvent, handler: HookHandler);
    pub fn register_channel(&mut self, channel: Box<dyn Channel>);
    pub fn register_provider(&mut self, provider: Box<dyn LlmProvider>);
    pub fn register_http_route(&mut self, path: &str, handler: HttpHandler);
    pub fn register_gateway_method(&mut self, method: &str, handler: GatewayMethodHandler);
    pub fn register_command(&mut self, command: ChatCommand);
}
```

#### Phase 3: WASM Plugin Sandbox

Using `wasmtime` for sandboxed plugin execution:

- Plugins compiled to `.wasm` modules
- WASI preview2 for filesystem/network access (scoped)
- Component Model for typed tool/hook interfaces
- Hot-loadable without restart

#### Hook Events

Match OpenClaw's 17 lifecycle hooks:

```rust
pub enum HookEvent {
    BeforeAgentStart,
    LlmInput,
    LlmOutput,
    AgentEnd,
    BeforeCompaction,
    AfterCompaction,
    BeforeReset,
    MessageReceived,
    MessageSending,
    MessageSent,
    BeforeToolCall,
    AfterToolCall,
    ToolResultPersist,
    SessionStart,
    SessionEnd,
    GatewayStart,
    GatewayStop,
}
```

---

## 6. Security Model

### 6.1 DM Pairing (Default)

Matches OpenClaw's pairing model:
- Unknown senders receive a pairing code
- Owner approves via CLI: `rusty-claw pairing approve <channel> <code>`
- Approved senders added to persistent allowlist
- Explicit opt-in required for open DMs (`dm_policy: "open"`)

### 6.2 Sandbox

```rust
pub struct SandboxConfig {
    pub mode: SandboxMode,              // off, non-main, all
    pub workspace: PathBuf,
    pub restrict_to_workspace: bool,
    pub allowed_tools: HashSet<String>,
    pub denied_tools: HashSet<String>,
}

pub enum SandboxMode {
    Off,                                // Host exec for all sessions
    NonMain,                            // Sandbox non-main sessions
    All,                                // Sandbox everything
}
```

Non-main sessions (groups, channels) can optionally run inside:
- **Workspace restriction** (PicoClaw-style path enforcement) — default
- **Docker containers** (OpenClaw-style per-session isolation) — opt-in
- **Landlock/seccomp** (Linux-native sandboxing) — Rust advantage, no Docker needed

### 6.3 Dangerous Command Blocking

Built-in blocklist for exec tool (matches PicoClaw's guard + extends):
- `rm -rf /`, `mkfs`, `dd if=`, fork bombs
- `shutdown`, `reboot`, `poweroff`
- Configurable via `tools.exec.deny_patterns`

### 6.4 Auth

- Token-based gateway auth (generated on first run)
- Password auth for public exposure (Tailscale Funnel)
- Trusted proxy support (X-Forwarded-For)
- Device pairing with public key signatures

---

## 7. CLI Interface

```
rusty-claw [COMMAND]

Commands:
  gateway       Start the gateway server
  agent         Chat with the agent (one-shot or interactive)
  onboard       Interactive setup wizard
  config        Configuration management (show, get, set)
  status        Show system status
  doctor        Diagnose common issues
  migrate       Migrate from OpenClaw config
  channels      Channel management (login, status, logout)
  sessions      Session management (list, reset, delete)
  cron          Scheduled task management
  pairing       Manage DM pairing approvals
  nodes         Remote node management
  skills        Skill management (list, install, update)
  update        Self-update

Options:
  -c, --config <PATH>    Config file path
  -v, --verbose          Verbose logging
      --port <PORT>      Gateway port (default: 18789)
      --ui               Enable Control UI
  -h, --help             Print help
  -V, --version          Print version
```

---

## 8. Performance Targets

| Metric | OpenClaw (Node.js) | PicoClaw (Go) | Rusty Claw Target |
|--------|--------------------|---------------|--------------------|
| RAM (idle, 1 channel) | >1 GB | <10 MB | **<30 MB** |
| RAM (3 channels, active) | >1.5 GB | ~15 MB | **<50 MB** |
| Cold start (x86_64) | ~5s | <1s | **<1s** |
| Cold start (ARM 0.8GHz) | >500s | <1s | **<2s** |
| Binary size | ~200MB npm package | ~15 MB | **<20 MB** (stripped) |
| Concurrent WS connections | ~1000 | ~5000 | **>10,000** |
| Message throughput | ~100 msg/s | ~500 msg/s | **>1000 msg/s** |

---

## 9. Compatibility Goals

### 9.1 Wire Protocol Compatibility

The gateway MUST implement OpenClaw's WebSocket protocol v3 such that:
- The OpenClaw macOS app can connect to Rusty Claw
- The OpenClaw iOS app can connect to Rusty Claw
- The OpenClaw Android app can connect to Rusty Claw
- OpenClaw's WebChat can connect to Rusty Claw

This means exact JSON frame format, method names, event names, and handshake sequence.

### 9.2 Config Compatibility

- Read OpenClaw's `openclaw.json` format (via migration tool)
- Same `~/.rusty_claw/workspace/` layout: `AGENTS.md`, `SOUL.md`, `TOOLS.md`, `skills/`, `sessions/`

### 9.3 Channel Token Reuse

Users should be able to reuse their existing channel tokens/credentials:
- Telegram bot tokens
- Discord bot tokens
- Slack bot/app tokens
- Signal credentials
- WhatsApp session data (Baileys-compatible, if feasible)

---

## 10. Development Phases

### Phase 1: Foundation (Weeks 1-6)

**Goal:** Working gateway with 1 channel (Telegram) and 1 provider (Anthropic), basic agent loop.

- [ ] Project scaffolding (Cargo workspace, CI, linting)
- [ ] Core types crate (`rusty-claw-core`)
- [ ] Config loading and validation
- [ ] Gateway WebSocket server (protocol v3 frames)
- [ ] Gateway handshake + auth (token mode)
- [ ] Session storage (JSONL transcripts)
- [ ] Agent runtime (basic tool loop with streaming)
- [ ] Anthropic Messages API provider
- [ ] Basic tools: `exec`, `read`, `write`, `edit`
- [ ] Telegram channel (grammY-equivalent via `teloxide`)
- [ ] CLI: `gateway`, `agent`, `onboard`, `status`
- [ ] Logging (`tracing` crate)

**Milestone:** Send a message on Telegram, get an AI response with tool use.

### Phase 2: Feature Parity Core (Weeks 7-14)

**Goal:** Multi-channel, multi-provider, full tool set, Control UI.

- [ ] Discord channel (`serenity` crate)
- [ ] WebChat channel (embedded in gateway HTTP)
- [ ] Slack channel
- [ ] OpenAI provider (Completions + Responses API)
- [ ] Google Gemini provider
- [ ] OpenRouter / generic OpenAI-compatible provider
- [ ] Ollama provider (local models)
- [ ] Web tools: `web_fetch`, `web_search`
- [ ] Session tools: `sessions_list`, `sessions_send`, `sessions_spawn`
- [ ] Memory tools: `memory_get`, `memory_search`
- [ ] Cron system
- [ ] DM pairing system
- [ ] Control UI (embedded Svelte/React SPA)
- [ ] Config hot-reload
- [ ] Gateway methods: full sessions, config, channels, models, cron
- [ ] Model failover and auth profile rotation
- [ ] CLI: `channels`, `sessions`, `cron`, `pairing`, `doctor`, `migrate`

**Milestone:** Drop-in replacement for OpenClaw for Telegram + Discord + Slack users.

### Phase 3: Advanced Features (Weeks 15-22)

**Goal:** Browser, canvas, plugins, remaining channels.

- [ ] CDP browser automation (`chromiumoxide` or `headless-chrome` crate)
- [ ] Canvas/A2UI host
- [ ] WhatsApp channel (bridge approach or Baileys-equivalent)
- [ ] Signal channel (`presage` crate or signal-cli bridge)
- [ ] iMessage via BlueBubbles
- [ ] Google Chat, Microsoft Teams
- [ ] TTS integration (ElevenLabs)
- [ ] Voice transcription (Whisper via Groq/local)
- [ ] Image generation tool
- [ ] Plugin system (native Rust plugins)
- [ ] Skills platform (bundled + workspace skills)
- [ ] Node protocol (remote device integration)
- [ ] Tailscale Serve/Funnel integration
- [ ] Docker sandbox mode

**Milestone:** Full OpenClaw feature parity minus native apps.

### Phase 4: Ecosystem & Polish (Weeks 23-30)

**Goal:** Production-ready, plugin ecosystem, native app compatibility verified.

- [ ] WASM plugin sandbox (wasmtime)
- [ ] Matrix channel
- [ ] LINE, Zalo channels
- [ ] Channel-specific action tools (Discord, Slack, Telegram, WhatsApp)
- [ ] Wizard-driven onboarding (full interactive setup)
- [ ] Self-update mechanism
- [ ] Comprehensive test suite (unit, integration, e2e)
- [ ] Performance benchmarking suite
- [ ] Cross-compilation CI (x86_64, aarch64, armv7, riscv64)
- [ ] Documentation site
- [ ] OpenClaw native app compatibility testing
- [ ] Security audit
- [ ] v1.0 release

---

## 11. Technology Stack

| Component | Choice | Rationale |
|-----------|--------|-----------|
| Language | Rust 2024 edition | Memory safety, performance, single binary |
| Async runtime | Tokio | Industry standard, mature ecosystem |
| HTTP framework | axum | Tower-based, composable, fast |
| WebSocket | tokio-tungstenite | Mature, well-tested |
| Serialization | serde + serde_json | Universal Rust serialization |
| Config format | json5 | OpenClaw compatibility |
| CLI framework | clap v4 | Derive-based, feature-rich |
| Logging | tracing + tracing-subscriber | Structured, async-aware |
| Telegram | teloxide | Most complete Rust Telegram library |
| Discord | serenity + songbird | Most complete Rust Discord library |
| HTTP client | reqwest | Async, TLS, connection pooling |
| Database (sessions) | JSONL files (Phase 1), optional SQLite via rusqlite (Phase 2) | Simple, portable, OpenClaw-compatible |
| Browser automation | chromiumoxide | CDP protocol in Rust |
| TLS | rustls | No OpenSSL dependency, pure Rust |
| Cross-compile | cross | Docker-based cross compilation |
| WASM plugins | wasmtime | Leading Wasm runtime |

---

## 12. Non-Goals (Explicitly Out of Scope)

1. **Native macOS/iOS/Android apps** — Reuse OpenClaw's existing apps via protocol compatibility
2. **npm/Node.js compatibility layer** — Not a Node.js wrapper, clean Rust rewrite
3. **100% API parity on day 1** — Phased rollout, gateway protocol compatibility is priority
4. **PicoClaw's <10MB target** — We target <50MB with full features; PicoClaw wins on ultra-minimal
5. **Custom LLM inference** — We call provider APIs, not run local inference (Ollama bridges this)
6. **GUI configuration tool** — Control UI serves this purpose; no native settings app

---

## 13. Success Metrics

| Metric | Target |
|--------|--------|
| OpenClaw macOS app connects and works | Yes (Phase 2) |
| RAM usage on Raspberry Pi 4 | <50 MB idle |
| Cold start on Raspberry Pi 4 | <2 seconds |
| Binary size (stripped, x86_64) | <20 MB |
| Telegram + Discord + Slack working | Phase 2 complete |
| All 14+ channels working | Phase 4 complete |
| Plugin SDK available | Phase 3 (native), Phase 4 (WASM) |
| CI passing on all targets | x86_64, aarch64, armv7, riscv64 |
| Zero `unsafe` in application code | Ongoing (deps may use unsafe internally) |

---

## 14. Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| WhatsApp integration complexity (Baileys is JS-only) | No WhatsApp channel | Bridge approach: run minimal Baileys bridge, communicate via IPC |
| OpenClaw protocol changes | Native app incompatibility | Pin to protocol v3, version-negotiate, track upstream |
| Rust async ecosystem gaps | Missing channel libraries | Fall back to HTTP/webhook-based integrations |
| WASM plugin adoption | Low plugin ecosystem | Native Rust plugins as primary, WASM as optional sandbox |
| Single-developer velocity | Slow Phase 3-4 | Prioritize ruthlessly; community contributions on channels |

---

## 15. Open Questions

1. **WhatsApp strategy:** Port Baileys to Rust, run as a sidecar bridge, or use the WhatsApp Business API?
2. **Session format:** Stay with JSONL (OpenClaw-compatible) or move to SQLite for query performance?
3. **Plugin ABI:** Stabilize a C ABI for dynamic `.so` plugins, or go WASM-only for sandboxing?
4. **Control UI:** Fork OpenClaw's Svelte UI, or build a minimal Rust-templated alternative?
5. **Workspace compatibility:** Should `~/.rusty_claw/` mirror `~/.openclaw/` exactly, or have its own layout?

---

## Appendix A: Reference Implementation Map

| OpenClaw Module | Rusty Claw Crate | Notes |
|----------------|------------------|-------|
| `src/gateway/` | `rusty-claw-gateway` | Wire protocol + WS server |
| `src/agents/` | `rusty-claw-agent` | Agent runtime + tool loop |
| `src/channels/` | `rusty-claw-channels` | Channel trait + implementations |
| `src/providers/` | `rusty-claw-providers` | LLM provider abstraction |
| `src/agents/tools/` | `rusty-claw-tools` | Built-in tool implementations |
| `src/plugins/` + `src/plugin-sdk/` | `rusty-claw-plugins` | Plugin system |
| `src/config/` | `rusty-claw-core` (config module) | Config loading + validation |
| `src/sessions/` | `rusty-claw-core` (session module) | Session storage |
| `src/browser/` | `rusty-claw-browser` | CDP browser automation |
| `src/media/` | `rusty-claw-media` | Media pipeline |
| `src/tts/` | `rusty-claw-tts` | Text-to-speech |
| `src/canvas-host/` | `rusty-claw-canvas` | Canvas/A2UI |
| `src/cli/` | `rusty-claw-cli` | CLI entry point |
| `ui/` | `rusty-claw-web` | Control UI + WebChat |

## Appendix B: Cargo Feature Flags

```toml
[features]
default = ["telegram", "discord", "webchat", "anthropic", "openai"]

# Channels (opt-in to minimize binary)
telegram = ["teloxide"]
discord = ["serenity"]
slack = []
whatsapp = []
signal = []
imessage = []
googlechat = []
msteams = []
matrix = []
webchat = []
line = []
bluebubbles = []

# Providers
anthropic = []
openai = []
google = []
ollama = []
bedrock = []
copilot = []

# Features
browser = ["chromiumoxide"]
canvas = []
tts = []
wasm-plugins = ["wasmtime"]

# Runtime
single-thread = []          # For constrained hardware
```
