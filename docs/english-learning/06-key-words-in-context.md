# Grok Build 关键单词：从词频到源码语境

这篇材料由 [词汇分析脚本](./scripts/README.md) 的实际结果整理而成。它关注的不是一般英语中的常见程度，而是这些词在 Grok Build 的 Rust 注释和英文文档里怎样描述架构、数据流、失败与安全边界。

## 分析口径

2026-08-09 在当前工作树运行：

```sh
python3 docs/english-learning/scripts/analyze_vocabulary.py \
  --top 60 \
  --examples 0
```

得到以下语料规模：

| 指标 | 数量 |
| --- | ---: |
| 扫描的 Rust 文件 | 2,454 |
| 扫描的 Markdown 文件 | 180 |
| 提取的 prose 片段 | 123,878 |
| 去除停用词后的词元 | 1,071,085 |

Rust 只统计 `//`、`///` 和 `//!` 注释，不统计标识符或可执行代码；Markdown 会跳过代码围栏。脚本内显式列出的词形会合并，例如 `sessions` 计入 `session`；歧义仍需回到原文判断。仓库继续变化后数字会改变，应以重新运行脚本的结果为准。

## 先看总体结果

下面不是未经筛选的全榜，而是与 Agent runtime 最相关的高频概念：

| 单词 | 出现次数 | 覆盖文件 | 为什么优先学 |
| --- | ---: | ---: | --- |
| `session` | 8,957 | 1,153 | 会话是运行时的主要生命周期边界 |
| `tool` | 6,519 | 796 | Agent 通过工具产生外部动作 |
| `prompt` | 5,536 | 784 | 用户输入、system prompt 和模板装配的共同核心 |
| `turn` | 4,649 | 679 | 一次输入触发的处理单元 |
| `agent` | 3,994 | 693 | 产品与运行实体的核心概念 |
| `model` | 3,621 | 669 | 采样、能力和上下文预算的来源 |
| `command` | 3,078 | 569 | Actor mailbox 和 shell 操作都使用该词 |
| `state` | 3,046 | 734 | 异步架构首先要回答谁拥有状态 |
| `event` | 2,760 | 544 | 流式响应与组件协调的主要表达 |
| `terminal` | 2,445 | 609 | 同时表示终端设备和终态，必须结合语境 |
| `task` | 2,374 | 460 | 同时涉及 Tokio task、后台任务和用户任务 |
| `request` | 1,936 | 534 | sampler、协议和 tool call 的输入侧 |
| `response` | 1,836 | 480 | 流式收集后的响应或协议回复 |
| `handle` | 1,620 | 549 | Actor 模式中共享访问入口的常用命名 |
| `dispatch` | 1,187 | 277 | 把动态输入路由到正确实现 |
| `context` | 1,173 | 398 | 既表示模型上下文，也表示运行环境 |
| `stream` | 1,112 | 323 | 模型响应和工具进度的增量传输方式 |

`file`、`path`、`test` 也位于总榜前列，但它们不够体现这个项目特有的 Agent 架构，因此没有作为本篇主线。

## 一、会话与 Agent

### 1. `session`

**翻译**：会话；持续存在的一段交互及其状态。

**项目语义**：session 不只是聊天窗口。它关联 chat history、tool context、模型配置、持久化数据和运行中的 turn。

常用搭配：

```text
create / load / resume a session
session state
session actor
session-level policy
keep the session resident
```

原文来自 [handle.rs:16](../../crates/codegen/xai-grok-shell/src/session/handle.rs)：

> A grok session has no terminal status field on its own — it is a resumable log on disk — so "liveness" is residency + turn-state, not a pid.

理解：session 的生命状态不能只用进程是否存在判断；磁盘日志可以恢复，内存驻留和 turn 状态才共同决定当前 liveness。

主动例句：

> The leader keeps the session resident while a turn is running or an input is queued.

### 2. `agent`

**翻译**：智能体；拥有模型、prompt、tools 和策略的执行实体。

**项目语义**：有时指产品类别，有时特指由 `AgentBuilder` 构造、绑定某个 session context 的运行对象。

常用搭配：

```text
coding agent
agent runtime
agent definition
build an agent
subagent
```

原文来自 [agent.rs:14](../../crates/codegen/xai-grok-agent/src/agent.rs)：

> A fully built agent: definition + session context. NOT portable — tied to a specific session via its ToolBridge, rendered system prompt, and session-level policies.

理解：构建完成的 Agent 不是可随意搬到另一 session 的无状态配置，它已经绑定 ToolBridge、渲染后的 prompt 和 session 级策略。

主动例句：

> `AgentBuilder` combines an agent definition with session-specific context.

### 3. `actor`

**翻译**：Actor；通过消息处理并独占或协调状态的长期运行实体。

**统计**：807 次，覆盖 214 个文件。它没有进入全榜最前列，却是理解本项目异步架构的必学词。

常用搭配：

```text
session actor
actor loop
actor state
send a command to the actor
the actor owns ...
```

原文来自 [acp_session.rs:3](../../crates/codegen/xai-grok-shell/src/session/acp_session.rs)：

> Each session runs as an actor with its own chat history and tool context. The agent owns the client connection and routes commands and events via channels.

理解：每个 session 以 Actor 形式运行并持有自己的会话数据；Agent 负责客户端连接，通过 channel 路由 command 和 event。

主动例句：

> The actor serializes state changes by processing commands through its mailbox.

### 4. `handle`

**翻译**：句柄；访问或控制另一个长期对象的轻量入口。

**项目语义**：`SessionHandle` 可以 clone 和传递，但 clone handle 不会复制 Actor 的权威状态。

常用搭配：

```text
clonable handle
shared handle
gateway handle
hold a handle
return a handle
```

原文来自 [handle.rs:1](../../crates/codegen/xai-grok-shell/src/session/handle.rs)：

> `SessionHandle` — the `Clone + Send` proxy for interacting with a session actor. Callers hold a `SessionHandle` and send `SessionCommand` messages via the internal channel.

理解：这里的 `proxy` 和 `handle` 都强调“通过入口间接操作”，而不是直接拥有或复制 Actor 状态。

主动例句：

> Multiple tasks can clone the handle and send commands to the same actor.

## 二、一次请求如何运行

### 5. `prompt`

**翻译**：提示词；发给模型、用于引导生成的输入。

**项目语义**：需要结合限定词区分 `system prompt`、`user prompt`、prompt template 和一次 `Prompt` command。

常用搭配：

```text
system prompt
user prompt
prompt template
prompt assembly
render a prompt
```

原文来自 [prompt/mod.rs:1](../../crates/codegen/xai-grok-agent/src/prompt/mod.rs)：

> System prompt assembly — template rendering, AGENTS.md, and skills.

理解：system prompt 不是一段固定字符串；它经过 template rendering，并装入项目规则和 skills。

主动例句：

> The runtime assembles the system prompt from a template, project instructions, and available skills.

### 6. `turn`

**翻译**：轮次；一次用户输入触发的处理单元。

**项目语义**：一个 turn 可以包含多次模型 sampling、tool call 和 tool result，不等于一次 HTTP request。

常用搭配：

```text
prompt turn
active turn
turn start / turn end
cancel a turn
end-of-turn result
```

原文来自 [commands.rs:76](../../crates/codegen/xai-grok-shell/src/session/commands.rs)：

> Result of a prompt turn, containing the stop reason, accumulated token count, and an optional turn-end signals snapshot.

理解：turn result 汇总整个轮次的停止原因、累计 token 和结束时信号，而不是单个网络响应。

主动例句：

> A turn may continue after a tool result causes the model to sample again.

### 7. `model`

**翻译**：模型；负责生成回复或执行辅助推理的 LLM。

**项目语义**：常与 model ID、model catalog、context window 和 override 搭配。不同 session 或 subagent 可以继承或覆盖模型选择。

原文来自 [config.rs:561](../../crates/codegen/xai-grok-agent/src/config.rs)：

> Model override for an agent definition. Two states: `Inherit` — use the parent session's model (default). `Override(String)` — use a specific model ID, resolved against available models at subagent spawn time.

理解：`inherit` 是继承父 session 的模型；`override` 是在 subagent 启动时解析指定 model ID。

主动例句：

> The subagent inherits the parent model unless its definition provides an override.

### 8. `request`

**翻译**：请求；送入某个服务、Actor 或 API 的输入。

**项目语义**：常见的有 sampling request、HTTP request、tool request 和 Actor query。阅读时要先确认边界。

常用搭配：

```text
build / send a request
per-request configuration
request body
request header
in-flight request
```

原文来自 [sampler/config.rs:3](../../crates/codegen/xai-grok-sampler/src/config.rs)：

> `SamplerConfig` is the per-request configuration handed to the sampler.

理解：`per-request` 表示配置作用于单次 sampling request，不应自动推断成全局配置。

主动例句：

> Chat state builds a model request from the current conversation and sampling configuration.

### 9. `response`

**翻译**：响应；对 request 的回复。

**项目语义**：模型响应通常以 stream 增量返回，最终才收集成 `ConversationResponse`；Actor query 也可能通过 oneshot 返回 response。

原文来自 [collect.rs:16](../../crates/codegen/xai-grok-sampler/src/stream/collect.rs)：

> Drain a `SamplingEvent` stream, returning the final response. Returns `Ok((response, metrics))` on the first `SamplingEvent::Completed` and `Err(error)` on the first `SamplingEvent::Failed`.

理解：`drain` 在这里表示持续消费 stream，直到得到 completed response 或 failed error。

主动例句：

> The collector consumes intermediate events and returns the final response.

### 10. `command`

**翻译**：命令；要求某个组件执行或查询操作的消息。

**一词多义**：既可能是 `SessionCommand` 这样的 Actor 消息，也可能是 shell command。看到它时应确认 command 的 owner。

原文来自 [commands.rs:1](../../crates/codegen/xai-grok-shell/src/session/commands.rs)：

> `SessionCommand` defines the message protocol used to drive a session actor.

理解：`drive` 表示通过一组 command 让 Actor 查询或改变状态；这里是进程内消息协议，不是网络 command protocol。

主动例句：

> The handle embeds a oneshot sender in the command when the caller expects a reply.

### 11. `event`

**翻译**：事件；已经发生的状态变化、进度或结果通知。

**项目语义**：command 往往表达意图，event 往往表达事实或增量。两者不能仅按类型名互换。

原文来自 [events.rs:1](../../crates/codegen/xai-chat-state/src/events.rs)：

> Events emitted by the `ChatStateActor` to the session main loop. Persistence is handled internally by the actor — these events are for session-level coordination only.

理解：这些 event 用于 session 层协调，不负责持久化本身；owner 边界在原文中写得很明确。

主动例句：

> The sampler emits progress events before the terminal completion event.

### 12. `task`

**翻译**：任务。

**一词多义**：可能表示 Tokio asynchronous task、后台 shell task、subagent task 或用户层待办。应观察 `spawn`、`JoinHandle`、task ID 等邻近词。

原文来自 [request_task.rs:1](../../crates/codegen/xai-grok-sampler/src/actor/request_task.rs)：

> Per-request streaming task. Spawned by the actor's `Submit` handler. Owns the retry loop and consumes a Layer 2 stream from the matching backend transform.

理解：这里的 task 是 Actor 启动的异步执行单元；它拥有 retry loop 并消费 backend stream。

主动例句：

> The actor spawns one task for the request and keeps its cancellation token.

## 三、状态与上下文

### 13. `state`

**翻译**：状态；系统在某一时刻需要保留的数据。

**项目语义**：阅读异步代码时，最重要的问题不是“哪里出现同名字段”，而是 `who owns the authoritative state`。

常用搭配：

```text
mutable state
shared state
actor-owned state
state transition
state snapshot
```

原文来自 [bridge.rs:48](../../crates/codegen/xai-grok-tools/src/bridge.rs)：

> Owns the registry and dispatches tool calls via `call_new_tool()`. All state lives in `Resources` on the registry — no separate `ToolState`.

理解：原文不仅说“有 state”，还明确权威 state 放在哪里，并否定了另一个可能的 owner。

主动例句：

> The actor protects mutable state by applying transitions in one message loop.

### 14. `context`

**翻译**：上下文；理解或执行某项操作所需的相关信息。

**一词多义**：

- `context window`：模型输入预算；
- `PromptContext`：渲染 prompt 的结构化输入；
- `ToolCallContext`：执行工具所需的运行环境；
- `session context`：与某 session 绑定的整体环境。

原文来自 [prompt/context.rs:1](../../crates/codegen/xai-grok-agent/src/prompt/context.rs)：

> `PromptContext` captures the agent-specific inputs to prompt rendering as a serializable struct. Users can dump it as JSON and inspect individual sections.

理解：context 不是含糊的“背景”；此处是可序列化、可检查的结构化输入。

主动例句：

> Compaction frees context-window space while preserving the information needed for later turns.

### 15. `terminal`

**翻译一**：终端设备或终端界面。

根 [README:13](../../README.md) 的原文：

> Grok Build is SpaceXAI's terminal-based AI coding agent.

**翻译二**：终态的、最终的；到达后不再产生同类后续项。

[tool.rs:118](../../crates/common/xai-tool-runtime/src/tool.rs) 的原文：

> Intermediate progress. Zero or more per stream. Terminal result. Exactly one per stream, always last.

辨别方法：`terminal-based`、TUI、shell 指终端设备；`terminal result/event/state` 指生命周期终态。

主动例句：

> A tool stream may emit many progress items, but it must end with exactly one terminal result.

## 四、边界与执行

### 16. `tool`

**翻译**：工具；模型可以调用的外部能力。

**项目语义**：工具具有 typed args/output，但会在 wire 或动态 registry 边界转换；并非所有普通 helper function 都是 model-visible tool。

原文来自 [tool.rs:32](../../crates/common/xai-tool-runtime/src/tool.rs)：

> The unified tool trait used by every tool source. Implement either `run` (blocking) or `execute` (streaming). The runtime only ever invokes `execute`.

理解：实现者可以提供 blocking `run` 或 streaming `execute`，但 runtime 的统一入口始终是 `execute`。

主动例句：

> The runtime exposes registered tools to the model and executes typed calls through a common trait.

### 17. `stream`

**翻译**：流；随着时间逐项产生的数据序列。

**项目语义**：模型生成、SSE 和 tool progress 都依赖 stream。stream 结束与产生一个明确 terminal item 是两个不同事实。

原文来自 [tool.rs:114](../../crates/common/xai-tool-runtime/src/tool.rs)：

> Stream of items a tool produces during a single call. Shape: `[Progress(_)*, Terminal(Result<T, ToolError>)]`.

理解：零个或多个 progress 后必须是一个 terminal result。

主动例句：

> The consumer reads the stream incrementally instead of waiting for the entire output.

### 18. `dispatch`

**翻译**：分派、调度；根据标识或类型把输入送到正确实现。

**项目语义**：常与 tool ID、backend、event 或 command 搭配。它比普通 `call` 多一层运行时选择。

常用搭配：

```text
dispatch a tool call
dispatch an event
dispatch to a backend
dispatch interface
```

原文来自 [dispatch.rs:24](../../crates/common/xai-tool-runtime/src/dispatch.rs)：

> Implementations route the `tool_id` to the correct tool, decode `args` against the tool's typed `Args`, and return the streaming result as `TypedToolOutput`.

理解：dispatch 同时包含路由、参数解码和返回统一动态输出，不只是调用一个已知函数。

主动例句：

> `ToolDispatch` routes the dynamic tool ID to a concrete typed implementation.

## 五、失败与恢复

### 19. `fallback` / `fall back to`

**翻译**：后备方案；主路径不可用时采用替代值或替代路径。

**统计说明**：脚本对名词 `fallback` 统计为 980 次、覆盖 415 个文件；它不会把 `falls back` 静默合并，因此学习时应把两者作为词族掌握，但不要混淆数字口径。

常用搭配：

```text
conservative fallback
fallback value
fallback path
fall back to a default
```

原文来自 [handle.rs:252](../../crates/codegen/xai-grok-shell/src/session/handle.rs)：

> Falls back to `true` (conservative: keep the session resident, never unload) if the actor is unreachable.

理解：fallback 不是随便选一个默认值。`true` 明确保护“不要误卸载仍可能繁忙的 session”这一风险方向。

主动例句：

> If the actor cannot reply, `is_busy` falls back to a conservative value.

### 20. `retry`

**翻译**：重试；失败后再次尝试同一操作或修正后的操作。

**项目语义**：retry 必须有 classification、budget、backoff 和 terminal decision；不是所有 error 都应该 retry。

常用搭配：

```text
retry loop
retry budget
retry policy
retryable error
exponential backoff
```

原文来自 [retry.rs:1](../../crates/codegen/xai-grok-sampler/src/retry.rs)：

> Retry classification, backoff, and decision-making. Pure logic only: no I/O, no notifications, no logging side-effects. The actor wraps this with the actual retry loop.

理解：纯函数层负责分类和决策，Actor 的 per-request task 才负责真正等待并再次执行。

主动例句：

> The retry policy treats transient transport failures differently from authentication errors.

## 六、频率较低但必须掌握的词

这些词没有全部进入总榜前 60，但承载了重要设计语义。

### 21. `compaction`

**统计**：746 次，166 个文件。**翻译**：压缩；用摘要重写较早对话，以释放 context window。

原文来自 [compaction.rs:3](../../crates/codegen/xai-grok-agent/src/compaction.rs)：

> Controls when and how the session's conversation is compacted to free up context window space, and whether a memory flush runs before each compaction.

例句：`Compaction preserves a summary while removing older detailed history from the request context.`

### 22. `persistence`

**统计**：244 次，131 个文件。**翻译**：持久化；让状态跨进程或 session 重载继续存在。

原文来自 [chat-state persistence.rs:1](../../crates/codegen/xai-chat-state/src/persistence.rs)：

> The actor owns persistence exclusively (`Box<dyn ChatPersistence>`), so the trait uses `&mut self` — no locks, no atomics, no shared state.

例句：`Persistence records durable conversation updates, while UI state can be rebuilt from them.`

### 23. `replay`

**统计**：579 次，165 个文件。**翻译**：重放；按已有事件或日志重新构建状态或重新发送更新。

原文来自 [replay.rs:1](../../crates/codegen/xai-grok-shell/src/session/helpers/replay.rs)：

> Replay pipeline for cross-compaction rewind. When rewinding to a prompt that precedes a compaction boundary, the in-memory conversation (and `chat_history.jsonl`) no longer contains the original messages. This module reconstructs the conversation by streaming `updates.jsonl` and handling `CompactionCheckpoint` / `RewindMarker` entries.

例句：`Replay reconstructs conversation state from durable updates instead of calling the model again.`

### 24. `cancellation`

**统计**：177 次，95 个文件。**翻译**：取消；请求停止尚未完成的异步工作。

原文来自 [request_task.rs:1](../../crates/codegen/xai-grok-sampler/src/actor/request_task.rs)：

> Cancellation is cooperative via `CancellationToken`.

`cooperative` 表示被取消的任务需要在可观察取消信号的位置主动退出，不是操作系统强行在任意指令处终止它。

例句：`Cancellation stops the active turn, while shutdown also releases session-level resources.`

### 25. `permission`

**统计**：1,140 次，272 个文件。**翻译**：权限；是否允许一次具体操作的决策。

原文来自 [permission/types.rs:5](../../crates/codegen/xai-grok-workspace/src/permission/types.rs)：

> A permission event capturing the decision made for a tool call. Used for telemetry to track permission patterns and user behavior.

例句：`The permission manager combines access kind, policy, and user choice into a final decision.`

### 26. `sandbox`

**统计**：352 次，114 个文件。**翻译**：沙箱；由操作系统或进程执行层强制实施的能力边界。

原文来自 [sandbox/lib.rs:8](../../crates/codegen/xai-grok-sandbox/src/lib.rs)：

> OS-level sandboxing for Grok Build via nono. Applied once at process startup. Covers in-process `tokio::fs` calls and child processes.

例句：`Permission approval does not bypass the operating-system sandbox.`

### 27. `invariant`

**统计**：273 次，164 个文件。**翻译**：不变量；在所有允许状态或执行路径中都必须成立的性质。

原文来自 [tool.rs:10](../../crates/common/xai-tool-runtime/src/tool.rs)：

> Stream invariant: at most arbitrarily many `Progress` items, ending in exactly one `Terminal`.

例句：`A focused test should verify the invariant, not merely exercise the happy path.`

### 28. `flush`

**统计**：724 次，200 个文件。**翻译**：排空、刷新；推动暂存数据写入目标位置，并等待达到约定边界。

原文来自 [persistence.rs:36](../../crates/codegen/xai-chat-state/src/persistence.rs)：

> Flush pending writes to disk.

在不同组件中，flush 可能指写磁盘、发送 replay buffer 或排空 telemetry sink，必须确认“把什么推进到哪里”和“是否等待 ack”。

例句：`The actor flushes pending updates before it reports shutdown completion.`

## 一词多义速查

| 单词 | 含义 A | 含义 B | 判断线索 |
| --- | --- | --- | --- |
| `terminal` | 终端设备 | 生命周期终态 | TUI/shell 与 result/event/state |
| `command` | Actor 消息 | shell 命令 | enum/channel 与 bash/terminal |
| `task` | Tokio task | 产品或后台任务 | spawn/await 与 task ID/output |
| `context` | 模型输入上下文 | 执行环境/结构化参数 | token/window 与 ToolCall/PromptContext |
| `handle` | 操作句柄 | 动词“处理” | 类型名/持有者与 `handle_event()` |
| `state` | 权威运行状态 | snapshot/cache 等派生状态 | owner、mutate、rebuild、snapshot |

## 建议学习顺序

第一轮只学习八个心智模型词：

```text
session -> agent -> actor -> handle -> command -> turn -> state -> event
```

第二轮学习模型与工具数据流：

```text
prompt -> request -> stream -> response -> tool -> dispatch -> terminal
```

第三轮学习失败与长期状态：

```text
fallback -> retry -> cancellation -> persistence -> replay -> compaction
```

每次选择 5 个词，不看本页完成以下输出：

```text
1. 用中文说出项目内的准确语义。
2. 写出两个常用搭配。
3. 用英文解释它与相邻概念的区别。
4. 回到一个源码原句验证自己的解释。
5. 写一句新的项目例句。
```

最终目标不是背出 `session = 会话`，而是能够自然说出：

> A session actor owns long-lived state, receives commands through a handle, emits events during a turn, and persists enough information to resume or replay the conversation safely.
