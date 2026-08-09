# 14 · 项目学习地图：从入口到可验证改动

这不是需要从头读到尾的第十四章，而是一张导航图。仓库文档已经覆盖架构、Agent Loop、协议、持久化、扩展和测试；本页负责回答更实际的问题：**我今天应该沿哪条链路学习，做到什么才算真的掌握？**

第一次使用时先选一条用户可观察行为，不要选择一个宽泛名词。比如“Prompt 为什么在队列里没有立即运行”比“学习 SessionActor”更容易验证。

## 一屏心智模型

```text
TUI / Headless / IDE
        │ ACP、进程内调用或 Leader 路由
        ▼
xai-grok-shell：Agent Host
        │
        ├─ SessionHandle --SessionCommand--> SessionActor/run_session
        │                                      │
        │                                      ├─ 输入队列 / running task
        │                                      ├─ Turn completion
        │                                      ├─ cancel / shutdown / timer
        │                                      └─ session/update
        │
        └─ handle_prompt：一次 Turn
                 ├─ ChatState：对话权威状态
                 ├─ Sampler：模型 HTTP 流
                 ├─ Tool runtime：内置工具 / MCP
                 ├─ Workspace：文件、VCS、权限、sandbox
                 └─ Persistence / Replay：持久化与客户端通知顺序
```

先固定六个 owner：

| 问题 | 长期 owner | 首个源码入口 |
|---|---|---|
| 客户端输入和显示 | Pager/ACP client | [`xai-grok-pager/src/app/`](../crates/codegen/xai-grok-pager/src/app) |
| 会话级排队、取消和关闭 | `SessionActor` | [`session/acp_session.rs`](../crates/codegen/xai-grok-shell/src/session/acp_session.rs) |
| 一轮 Prompt 的执行 | running `AgentTask` / `handle_prompt` | [`turn.rs`](../crates/codegen/xai-grok-shell/src/session/acp_session_impl/turn.rs) |
| 对话历史和请求素材 | `ChatStateActor` | [`xai-chat-state/src/`](../crates/codegen/xai-chat-state/src) |
| 文件、Git 和权限状态 | Workspace session | [`xai-grok-workspace/src/`](../crates/codegen/xai-grok-workspace/src) |
| 流式通知和落盘历史 | ReplayBuffer / persistence actor | [`replay_events.rs`](../crates/codegen/xai-grok-shell/src/session/replay_events.rs)、[`persistence.rs`](../crates/codegen/xai-grok-shell/src/session/persistence.rs) |

当一个现象跨越多行或多个 task 时，先问“谁拥有最终真相”，再问“消息怎样到它那里”。不要从出现同名字段的第一个文件开始修改。

## 三遍阅读法

### 第一遍：只认边界

目标是在 60--90 分钟内画出上面的图，而不是理解全部实现。

1. 读 [架构总览](./02-architecture.md) 的总览、分层职责和一次请求时序。
2. 用 [模块地图](./07-module-map.md) 找到客户端、Session、Sampler、Tool 和 Workspace 的目录。
3. 运行 [`mini_session_actor`](./rust-essentials/labs/async-demos/src/bin/mini_session_actor.rs)：

```sh
cargo run --locked \
  --manifest-path docs/rust-essentials/labs/async-demos/Cargo.toml \
  --bin mini_session_actor
```

4. 对照真实 [`run_session` 精读](./deep-dives/run-session.md)，只标出 mailbox、pending queue、running task、completion 和 shutdown。

第一遍的完成证据是一张不超过 15 个节点的图，并能说明普通函数调用、channel、spawn task 和协议边界分别是哪条箭头。

### 第二遍：只跟一条旅程

从下表选一行。一次只跟一个具体值，例如一个 `prompt_id`、一个 tool call id 或一个 session id。

| 想解释的旅程 | 先读 | 主要源码 | 最小验证方向 |
|---|---|---|---|
| 用户按 Enter 后怎样启动 Turn | [`message-flow`](./deep-dives/message-flow.md)、[`run-session`](./deep-dives/run-session.md) | Pager action → `SessionCommand::Prompt` → `queue_input` → `handle_prompt` | `mini_session_actor` + Session focused test |
| 模型请求如何流式返回 | [`sampling-lifecycle`](./deep-dives/sampling-lifecycle.md) | `sampler_turn.rs`、`xai-grok-sampler` actor/stream/retry | sampler fixture，断言事件数和终态 |
| 一次工具调用怎样执行和写回 | [`tool-call-pipeline`](./deep-dives/tool-call-pipeline.md) | `tool_calls.rs`、`xai-tool-runtime`、ToolBridge | tool runtime integration test |
| 文件修改如何受权限保护并可恢复 | [`permissions-and-sandbox`](./deep-dives/permissions-and-sandbox.md)、[`workspace-state`](./deep-dives/workspace-state-and-worktree-lifecycle.md) | workspace permission/session/checkpoint | temp workspace fixture |
| 长会话怎样压缩和恢复 | [上下文管理](./04-context-management.md)、[`persistence-and-replay`](./deep-dives/persistence-and-replay.md) | ChatState、compaction、storage/jsonl | fixture + 恢复后状态断言 |
| Cancel/Shutdown 怎样跨层收尾 | [`cancellation-and-shutdown`](./deep-dives/cancellation-and-shutdown.md) | `run_loop.rs`、Sampler、工具进程、workflow | oneshot/barrier + deadline |
| UI 为什么显示或漏掉一条更新 | [`pager-rendering`](./deep-dives/pager-rendering.md) | ACP handler、ReplayBuffer、render blocks | reducer/render snapshot test |
| 配置和模型最终选了哪一份 | [`configuration-and-runtime-resolution`](./deep-dives/configuration-and-runtime-resolution.md)、[`authentication-and-model-resolution`](./deep-dives/authentication-and-model-resolution.md) | ConfigLayers、ModelsManager、SamplerConfig | resolution table test |

第二遍不要记录“这个函数很复杂”。每一步只记录四件事：输入值、owner、边界类型、失败出口。

### 第三遍：做一个受控改动

按 [阶段化贡献项目](./13-contribution-projects.md) 选择真实的小问题，并使用 [贡献者工作流](./deep-dives/contributor-workflow.md)：

1. 写出当前行为和一个反例。
2. 找到最终 owner，而不是最近的 UI 症状。
3. 先确认现有测试是否已覆盖。
4. 用失败测试或可重复复现证明缺口。
5. 做最小实现，运行 focused test 和直接依赖检查。
6. 写清未覆盖的平台、协议或并发路径。

第一次改动优先选择纯转换、错误上下文、测试 fixture 或文档锚点。不要把第一次提交放在 Session 调度、公共 wire 类型或 `unsafe` 上。

## 六次 45 分钟学习安排

| 次数 | 具体问题 | 动手产出 |
|---|---|---|
| 1 | 请求从哪个进程/入口进入？ | 画 host mode 与 SessionActor 的边界 |
| 2 | SessionActor 同时等待什么？ | 运行 mini actor，填写 `select!` 分支表 |
| 3 | 一次 Turn 在哪里真正调用模型？ | 画 `handle_prompt → sampler → completion` |
| 4 | 一次 tool call 谁负责权限、执行和终态？ | 跟一个具体工具，标出强类型/JSON 边界 |
| 5 | 状态如何落盘、重放和回到 UI？ | 区分 ChatState、JSONL persistence、ReplayBuffer |
| 6 | 选择一个小改动怎样证明正确？ | 提交一页证据模板和 focused test 输出 |

每次结束时闭卷回答：“权威状态在哪里？输入从哪里来？成功和失败分别回到哪里？什么证据能推翻我的解释？”答不出时缩小问题，不延长阅读文件列表。

## 项目级可运行镜像

| 先运行 | 再读生产源码 | 固定的不变量 |
|---|---|---|
| [`mini_session_actor`](./rust-essentials/labs/async-demos/src/bin/mini_session_actor.rs) | [`run-session`](./deep-dives/run-session.md) | mailbox、单一 running Turn、completion 回流 |
| [`mini_tool_pipeline`](./rust-essentials/labs/async-demos/src/bin/mini_tool_pipeline.rs) | [`tool-call-pipeline`](./deep-dives/tool-call-pipeline.md) | JSON/强类型边界、权限、`Progress* -> Terminal` |
| [`mini_replay_order`](./rust-essentials/labs/async-demos/src/bin/mini_replay_order.rs) | [`persistence-and-replay`](./deep-dives/persistence-and-replay.md) | chunk 合并、非流式事件和 completion 前 flush |
| [`mini_cancel_shutdown`](./rust-essentials/labs/async-demos/src/bin/mini_cancel_shutdown.rs) | [`cancellation-and-shutdown`](./deep-dives/cancellation-and-shutdown.md) | Cancel 结束 Turn；Shutdown 收回 Session 资源 |

这些程序只保留主干契约。每次运行后必须写出“生产代码多了哪些 owner、错误和持久化边界”，否则缩小模型会反过来遮蔽真实复杂度。

## 固定学习记录模板

```text
问题：一个具体、可观察的问题
入口：公开入口或用户动作
值：本次追踪的 prompt_id / session_id / tool_call_id
owner：谁拥有最终状态
路径：函数 -> await ~~> channel ==>> wire -JSON->
成功：终态由谁确认
失败：错误、取消、关闭分别去哪里
证据：定义、调用点、测试或日志各至少一个
验证：命令实际运行了多少测试，证明什么
未知：仍未验证的假设
```

## 常用导航命令

```sh
# 从 Session 的三个核心符号开始
rg -n "struct SessionHandle|enum SessionCommand|async fn run_session" \
  crates/codegen/xai-grok-shell/src/session

# 找一次 Prompt 的排队、启动和完成
rg -n "queue_input|maybe_start_running_task|handle_prompt|handle_completion" \
  crates/codegen/xai-grok-shell/src/session

# 先列测试，避免过滤器匹配零项
cargo test -p xai-grok-shell -- --list | rg -i "session|prompt|cancel|shutdown"
```

命令输出是导航证据，不是行为证据。一个 `rg` 命中或 `cargo check` 成功不能证明异步顺序、取消或落盘语义；这些需要执行到目标路径的测试或可重复实验。

## 什么时候算“会了”

- 能从用户动作定位到长期状态 owner。
- 能区分 Session 外层循环、Turn 内层循环和独立 spawned task。
- 能为一个 prompt 画出 command、queue、completion 和 client update。
- 能说明 Tool、Workspace、ChatState、Persistence 为什么不是 SessionActor 的同义词。
- 能选择一个 focused test，并解释它没有覆盖什么。
- 能完成一个小改动而不扩大公共 API 或并发不变量。

做到这些以后再读 [Crate 全目录](./10-crate-catalog.md) 或专题精读会更轻松；不要把记住 80 多个 package 名称当作前置条件。
