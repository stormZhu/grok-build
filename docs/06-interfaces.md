# 06 · 接口分析

本文梳理 Grok Build **对外集成面**与**内部核心 API 面**，便于对接、抓协议与二次开发定位。

---

## 1. 接口分层图

```
┌─────────────── 外部集成 ───────────────────────────────────┐
│  ACP (stdio/JSON-RPC)   MCP clients   REST (xAI proxy)    │
│  Browser OAuth / OIDC   Hooks scripts  Plugin marketplace │
└────────────────────────────┬──────────────────────────────┘
┌────────────────────────────▼──────────────────────────────┐
│  Pager CLI / Leader socket / Headless API                  │
└────────────────────────────┬──────────────────────────────┘
┌────────────────────────────▼──────────────────────────────┐
│  SessionActor 命令/事件 · ChatState · Sampler · ToolBridge │
└────────────────────────────┬──────────────────────────────┘
┌────────────────────────────▼──────────────────────────────┐
│  tool-protocol (Computer Hub) · workspace RPC · sampling types │
└───────────────────────────────────────────────────────────┘
```

---

## 2. Agent Client Protocol (ACP)

### 2.1 角色

- **标准**：编辑器 / 客户端 ↔ Agent  
- **Rust 依赖**：`agent-client-protocol`（workspace 中带 `unstable` feature）  
- **实现宿主**：`xai-grok-shell` SessionActor + `xai-acp-lib` gateway  

### 2.2 入口

```sh
grok agent stdio          # IDE 嵌入
# 内部: run_stdio_agent
```

### 2.3 会话数据流

```
Client                         Agent (SessionActor)
  │  session/prompt              │
  │─────────────────────────────►│ handle_prompt
  │  session/update (stream)     │
  │◄─────────────────────────────│ AgentMessageChunk / ToolCall*
  │  session/cancel              │
  │─────────────────────────────►│ cancel turn
```

### 2.4 扩展通知

除标准 ACP 更新外，shell 有 **XaiSessionUpdate**（`extensions/notification.rs`），例如：

- ImageCompressed / ImageDropped  
- 重试状态、用量、自定义 UI 元数据  
- 与 pager 对齐的扩展 chunk meta（如 hideFromScrollback）  

集成方应区分：**标准 ACP 可移植** vs **xAI 扩展需 pager/兼容客户端**。

### 2.5 相关源码

| 路径 | 内容 |
|------|------|
| `xai-acp-lib` | ACP 库封装 |
| `session/acp_session.rs` | 会话 actor 根 |
| `session/acp_conversion.rs` | 类型转换 |
| `agent/mvp_agent/acp_agent.rs` | ACP agent 侧 |

---

## 3. Sampling / LLM 接口

### 3.1 类型层（无 I/O）

**Crate**：`xai-grok-sampling-types`

核心类型：

```rust
// 逻辑对话
ConversationItem::{ System, User, Assistant, ToolResult, /* hosted… */ }
ConversationRequest {
    items, tools: Vec<ToolSpec>, hosted_tools: Vec<HostedTool>,
    tool_choice, model, temperature, max_output_tokens, …
}
ConversationResponse { items, usage, stop_reason, … }
ToolSpec { name, description, parameters /* JSON Schema */ }
HostedTool::{ WebSearch, /* … */ }  // 服务端执行
StopReason::{ Stop, Length, ToolCalls, ContentFilter }
```

另含 Messages API 侧 `StopReason`（EndTurn、ToolUse、Refusal…）与 doom-loop 信号类型。

### 3.2 运行时层

**Crate**：`xai-grok-sampler`

| 层 | API | 说明 |
|----|-----|------|
| L1 | `SamplingClient` | 原始 HTTP 流 |
| L2 | `stream_chat_completions` / `stream_responses` / `stream_messages` | → `SamplingEvent` |
| L3 | `SamplerHandle` / `SamplerActor` | 并发、重试、取消 |

配置要点（`SamplerConfig`）：

- `ApiBackend`（Completions / Responses / Messages 等）  
- `AuthScheme` / `BearerResolver` / `HeaderInjector`  
- `RetryPolicy`、origin client info、UA  

错误分类：`SamplingError`、context length 检测、空响应、`format_sampling_error`。

### 3.3 线上游

默认走 xAI **cli-chat-proxy** / API（`GROK_CLI_CHAT_PROXY_BASE_URL`、`XAI_API_KEY` 等）。  
自定义模型见用户手册 `11-custom-models.md` 与 config。

### 3.4 ChatState 对外 Handle

```text
ChatStateHandle
  push_user / push_tool_result / record_token_usage
  build_request(tools, memory_reminder, …) -> ConversationRequest
  get_conversation* / get_prompt_index / get_sampling_config
  compaction 相关 mutation API
```

Session **只通过 Handle** 碰 conversation，保证单写者。

---

## 4. Tool 接口

### 4.1 运行时契约 — `xai-tool-runtime`

```rust
trait Tool { /* name, schema, call → ToolStream */ }
ToolDispatch
ToolCallContext { cwd, cancellation, session, trace, … }
ToolNotification / ToolNotificationHandle
ToolSearchIndex
```

统一：

- 同步/流式结果  
- 进度与通知  
- 错误 `ToolError` / `ToolErrorKind`  

### 4.2 线协议 — `xai-tool-protocol`（Computer Hub）

JSON-RPC 2.0 风格，方法目录 `methods::Method`，帧类型包括：

| 类别 | 示例 |
|------|------|
| 握手 | Hello / HelloAck，`PROTOCOL_VERSION` |
| 会话 | SessionOpen/Close/Bind/AttachServer… |
| 工具 | ToolCall、ToolsList、ToolsSearch、ToolCallProgress |
| 服务 | Serve、ServerBind/Unbind、ServersList |
| 订阅 | SubscribeNotifications / ToolNotificationFrame |
| 观测 | LogsDonate / MetricsDonate / TracesDonate |
| 钩子 | HookFrame / HookReplyFrame |
| 系统 | SystemNotify、Ping/Pong |

工具注册：`ToolRegistration` / `ToolServerRegistration` + capabilities（streaming、scope、hooks）。

**Workspace** 可暴露 Hub server，使工具执行与会话在独立进程拓扑中运行。

### 4.3 产品工具桥 — `xai-grok-tools::ToolBridge`

```rust
ToolBridge::finalize_builder(builder, config, session_ctx)
tool_definitions() / tool_definitions_builtins_only()
call_new_tool(...) -> ToolBridgeResult { output, prompt_text }
tool_for_kind(ToolKind)  // 按能力查名
```

内置工具族（`implementations/grok_build/` 等）：

| 能力域 | 代表工具 |
|--------|----------|
| 文件 | `read_file`, `search_replace`, `list_dir`, `grep` |
| Shell | `run_terminal_command` / bash 变体 |
| 任务 | `task`, `get_task_output`, `kill_*`, `monitor`, `scheduler_*` |
| 计划 | `todo_write`, `enter_plan_mode`, `exit_plan_mode`, `ask_user_question`, `update_goal` |
| 网络 | `web_search`, `web_fetch` |
| 媒体 | `image_gen`, `image_edit`, `video_gen` |
| 记忆 | `memory_search`, `memory_get` |
| MCP | `search_tool`, `use_tool`（及 `server__tool` 直调） |
| 兼容 | Codex apply_patch / OpenCode 风格读写 |

**输出上限**：`DEFAULT_TOOL_OUTPUT_BYTES`（40KB）、shell 字符上限、MCP `max_output_bytes`。

### 4.4 MCP

- 配置：用户/项目 config 中 MCP servers  
- 客户端：`xai-grok-mcp`  
- 会话：`mcp_state`、dispatcher、auto-restart、permission persistence  
- 命名：`server_name__tool_name`（双下划线）  
- 初始化策略：`Blocking` vs `Progressive`  

---

## 5. Agent 构建接口 — `xai-grok-agent`

```rust
// 定义
AgentDefinition::from_file(path)?
discovery::discover(&cwd)
discovery::by_name("code-reviewer")

// 构建
AgentBuilder::new(cwd, terminal, notification_handle)
    .from_definition(def)
    // 或 .with_name().with_tools()…
    .with_prompt_audience(Primary|Subagent)
    .build().await? -> Agent

// 使用
agent.system_prompt()
agent.tool_definitions().await
agent.tool_bridge()
agent.compaction_policy()
agent.hosted_tools()
```

错误：`AgentBuildError::{ ParseError, MissingField, UnknownToolOverride, … }`。

---

## 6. Session 命令接口（内部）

Session 通过 `SessionCommand`（`session/commands.rs`）接收：

典型类别：

- Prompt（blocks、mode、schema、verbatim…）  
- Cancel  
- Shutdown  
- Model switch / config reload  
- MCP 管理  
- Rewind / fork / compact  
- Permission 响应  
- Subagent 控制  

对外（ACP/Leader）会映射到这些命令。完成以 `PromptTurnResult` / `TurnOutcome` 返回。

`TurnOutcome` 概念：

- `Completed { snapshot, tools_called, structured_output, refusal }`  
- `Cancelled { category, context }`  
- `MaxTurnsReached { limit }`  
- 错误路径 → ACP Error  

---

## 7. Leader 接口

**模块**：`xai-grok-shell/src/leader/`

| 概念 | 说明 |
|------|------|
| Socket | 本机控制面路径（与 workspace URL 相关） |
| `LeaderClient` / `connect_or_spawn` | 连接或拉起 leader |
| `LeaderRegistration` / capabilities | 客户端模式与能力协商 |
| `ControlCommand` / `ControlPayload` | 控制消息 |
| 版本 | `leader_is_older_than` 防偏斜 |

用途：多 TUI / 工具共享同一 agent 后端、减少重复进程。

---

## 8. Workspace 接口

**Crate**：`xai-grok-workspace`

```rust
WorkspaceHandle / connect_local_workspace
WorkspaceClient
PermissionHandle  // AccessKind, Decision
FileStateHandle / HunkTracker
WorkspaceOp / WorkspaceOps
CapabilityMode
IsolationMode  // 含 worktree
```

权限是 **硬边界**：模型 tool call → PermissionHandle → 允许/拒绝/询问用户。  
Plan mode 的 edit gate 在 Session 层额外强化。

---

## 9. 鉴权接口

**模块**：`xai-grok-shell/src/auth/` + `xai-grok-auth`

| 方式 | 说明 |
|------|------|
| Browser / grok.com | 默认；`~/.grok/auth.json` |
| `XAI_API_KEY` | CI / 无浏览器；优先于 session |
| OIDC | 客户 IdP + PKCE；`GROK_OIDC_*` |
| External auth | 可插拔 refresher |

Session 侧：`session_token_auth_gate` 决定是否允许用 session token 刷新（防 BYOK 第三方向量泄漏）。

---

## 10. 配置接口

| 来源 | 路径/机制 |
|------|-----------|
| 用户配置 | `~/.grok/config.toml` |
| 项目配置 | `.grok/` 下多文件 |
| 环境变量 | `GROK_*`、`XAI_API_KEY` 等 |
| Managed config | 签名/团队下发（shell `managed_config`） |
| Requirements | 部署侧 requirements.toml |

类型：`xai-grok-config` / `xai-grok-config-types`。  
热更新：config watcher + reloader。

---

## 11. Hooks / Plugins 接口

### Hooks

- 配置 + 示例：`xai-grok-hooks/examples`  
- 事件：tool pre/post、session 生命周期等  
- 可返回 deny  

### Plugins

- 清单 / 信任 / 市场：`xai-grok-agent/src/plugins/*`、`xai-grok-plugin-marketplace`  
- 安装注册表、git install、local refresh  

### Lifecycle contributors

`xai-agent-lifecycle`：进程内扩展，**无线协议**，编译期或注册表安装。

---

## 12. 持久化与文件布局（会话接口的落盘面）

典型（以 `~/.grok` / 项目 `.grok` 为准，详见用户手册 sessions）：

| 产物 | 用途 |
|------|------|
| chat history (jsonl) | conversation 重放 |
| updates.jsonl | UI/回放更新流 |
| prompt 大文件 offload | 超大用户输入 |
| plan 文件 | plan mode |
| auth.json | 凭证 |
| worktrees/ | 隔离会话 |

**Rewind / fork** 基于 prompt_index + 持久化截断；compact 后边界需特殊处理（有专门测试）。

---

## 13. Headless / 可编程接口

```sh
grok -p "Explain this codebase"
# 支持输出格式、max-turns、模型等 CLI 标志（见 pager/shell 参数）
```

Headless 走同一 SessionActor 路径，无 TUI notification 渲染；适合 CI。

Introspection：`grok inspect`（`shell/inspect`）导出内部状态便于调试。

---

## 14. 接口选择建议

| 目标 | 推荐接口 |
|------|----------|
| 嵌入 IDE | ACP stdio |
| 脚本/CI | Headless CLI 或 ACP |
| 自定义工具进程 | tool-protocol / MCP |
| 只改行为不改代码 | AGENTS.md + Skills + Hooks + Agent md |
| 深度定制循环 | 不推荐 fork loop；优先 lifecycle + hooks |
| 对接自有 LLM 网关 | SamplerConfig / custom models + auth |

---

## 15. 相关用户文档（产品视角）

仓库内：`crates/codegen/xai-grok-pager/docs/user-guide/`

- `07-mcp-servers.md`、`15-agent-mode.md`、`16-subagents.md`  
- `14-headless-mode.md`、`18-sandbox.md`、`22-permissions-and-safety.md`  
