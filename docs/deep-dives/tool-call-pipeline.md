# 源码精读：一次 Tool Call 如何穿过系统

这篇文章把「模型要求调用工具」追到「工具结果再次成为模型上下文」的完整链路拆开。它适合在以下情况阅读：

- 想新增或修改一个内置工具；
- 工具没有出现在模型可见的 `tools` 列表中；
- 一次工具调用被拒绝、取消、卡住或结果没有回到对话；
- 需要分辨 MCP 工具、内置工具和 Tool Protocol 的职责边界。

先读 [03-agent-loop.md](../03-agent-loop.md) 了解大循环，再回到本文逐段跟源码。本文只描述当前公开树可见的代码；运行时配置会影响某些分支是否启用。

---

## 1. 先记住：工具不是一个函数调用

在这个项目中，一个工具调用同时跨过四个边界：

```text
模型响应（名字 + JSON 参数）
  -> SessionActor：本轮策略、权限、并发和 UI/ACP 事件
  -> ToolBridge / FinalizedToolset：名称解析、资源和运行时 dispatch
  -> 具体 Tool：类型化参数、执行、进度流、终态结果
  -> SessionActor / ChatState：写入 tool result，供下一次 sampling 使用
```

因此「能编译的工具」不等于「模型能调用的工具」。至少要同时满足：

1. 被注册进当前 session 的 `ToolRegistryBuilder`；
2. 在该 Agent 的配置、allowlist 和 feature gate 下仍然可用；
3. 能导出正确的模型侧 `ToolDefinition`（名称、描述、JSON Schema）；
4. 执行阶段通过权限、模式和取消策略；
5. 以模型可消费的文本/内容块完成一次终态返回。

这些是排障时的检查顺序，不要一开始只盯着 `impl Tool`。

---

## 2. 类型和运行时分别负责什么

### 2.1 `xai-tool-runtime`：最小可移植契约

源码入口：[xai-tool-runtime/src/tool.rs](../../crates/common/xai-tool-runtime/src/tool.rs)。

`Tool` 是所有本地工具的统一 trait，关键关联类型是：

| 成员 | 职责 | 工具作者要关注什么 |
|---|---|---|
| `Args` | 从 JSON 解码的类型化入参，同时生成 schema | 使用 `Deserialize + JsonSchema`；schema 就是模型调用 API 的一部分 |
| `Output` | 结构化终态输出 | 实现 `ToolOutput`，区分给模型的内容和 UI/协议数据 |
| `id()` | 稳定路由 ID | 变更它会影响 dispatch、配置和兼容层 |
| `description()` | 模型看到的说明 + 参数 schema | 这是模型决定何时调用工具的主要依据 |
| `capabilities()` | 并发、作用域、流式等元数据 | 写操作、会话范围和流式能力需与真实行为一致 |
| `run()` / `execute()` | 具体执行 | 简单工具实现 `run`；需要持续输出时覆写 `execute` |

`execute()` 是运行时真正调用的入口。默认实现会把 `run()` 包成仅含一个终态项的流；所以实现 `run()` 已经足够，但不要同时假定调用方会直接调它。

流有一个重要不变量：可以有零到多个 `Progress`，最后必须有且只有一个 `Terminal(Result<Output, ToolError>)`。这让终端输出、长任务状态或分段内容可以在不破坏最终结果语义的前提下流动。`ToolDispatch::call_terminal()` 还会验证流没有悄悄结束而缺少终态。

### 2.2 `Tool` 与 `ToolDispatch`：为何有两层 trait

`Tool` 带关联类型，无法直接做成统一的 trait object。运行时改用 object-safe 的 `ToolDispatch`（[dispatch.rs](../../crates/common/xai-tool-runtime/src/dispatch.rs)）：

```text
模型 JSON args
  -> dispatch 按 ToolId 找实现
  -> serde 解码为 T::Args
  -> T::execute(ctx, args)
  -> 擦除为 ToolStream<TypedToolOutput>
```

这样具体工具仍拥有 Rust 类型检查，动态注册表和 MCP 适配层则只处理 JSON/擦除后的输出。修改工具接口时，先问改动是应该留在某个具体 `Tool` 中，还是必须成为所有 dispatch 实现都要遵守的运行时契约；后者影响面大得多。

### 2.3 `ToolBridge`：Session 和注册表之间的适配器

[xai-grok-tools/src/bridge.rs](../../crates/codegen/xai-grok-tools/src/bridge.rs) 中的 `ToolBridge` 持有 `FinalizedToolset`。它不是一个额外的业务逻辑层，而是 session 消费工具能力的入口：

- `finalize_builder()` 将 `ToolRegistryBuilder + ToolServerConfig + SessionContext` 固化成可执行工具集；
- `tool_definitions()` 取模型侧定义；
- `call_new_tool()`（在同文件后段）经注册表执行一次调用；
- `register_mcp_tools()` 将运行时发现的 MCP 工具插入同一工具面；
- `kill_foreground_commands()` 能在 registry 被工具执行占用时仍取消前台终端命令。

注意 `ToolBridgeResult` 同时保留两份东西：结构化 `output` 给 ACP/UI/追踪使用，`prompt_text` 给下一轮模型上下文使用。只修其中一份经常会造成“界面看起来正确但模型没有得到关键信息”，或反过来的问题。

---

## 3. 注册：从 Rust 类型到模型的 `tools` 数组

### 3.1 内置工具在哪里加入

[registry/types.rs](../../crates/codegen/xai-grok-tools/src/registry/types.rs) 的 `ToolRegistryBuilder` 是内置工具目录。它用 `register::<T>()` 或 `register_with_params::<T, P>()` 保存：

- 工具构造方式；
- 参数资源的注册方式；
- `ToolMetadata`、client-facing 名称、`ToolKind`；
- 将具体 `Tool` 装入本地运行时 registry 的闭包。

`finalize()` 再根据 `ToolServerConfig`、Agent allowlist/denylist、功能开关和 `SessionContext` 组装 `FinalizedToolset`。因此一个工具可能“代码中注册了”但当前 Agent 不可见，例如该 Agent 的定义禁用了它，或其依赖的 backend 没有配置。

常见入口：

| 场景 | 首先看 |
|---|---|
| 新 Grok Build 内置工具 | `implementations/grok_build/<tool>/` 与 `registry/types.rs` |
| Codex / OpenCode 兼容工具 | `implementations/codex/`、`implementations/opencode/` |
| 动态 MCP 工具 | `ToolBridge::register_mcp_tools()` 与 shell 的 `session/.../mcp.rs` |
| 只改变模型看到的名称 | `ToolDefinition`、Agent 的 `toolNameOverrides` / `paramNameOverrides` |
| 只改变能否使用 | Agent definition、`ToolServerConfig`、plan/permission gate |

### 3.2 生成 definition 时的两种名字

区分三个标识能避免很多误判：

| 标识 | 使用位置 |
|---|---|
| `ToolId` | runtime 的稳定路由 ID |
| `ToolKind` | 产品语义分类，例如用于判断是否是写文件/任务类能力 |
| client-facing name | 发送给模型、显示在 tool call 中的名字；可由兼容或 Agent 配置改写 |

模型发回的是 client-facing name，随后注册表再映射到真实工具。新增工具时需在正确层选择稳定性：内部重构可以不改变对模型暴露的名字；破坏名字或参数 schema 则属于模型/插件/用户配置可观察的 API 变更。

---

## 4. 执行：`SessionActor` 是策略所有者

源码入口：[tool_calls.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/tool_calls.rs)，调用点在 [turn.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/turn.rs) 的 `process_conversation_turn()`。

模型采样完成后，`process_conversation_turn()` 先把 assistant 输出写入对话；若含有 tool calls，则调 `execute_tool_calls()`。这一步并不是直接 `bridge.call(...)`，因为 session 还要管理用户可见和安全敏感的行为：

```text
assistant tool calls
  -> 拆分批次（需要串行的调用与可并行的调用）
  -> prepare_tool_call：解析名称、参数、模式/权限前置检查
  -> 发出 tool-call-start 通知与 tracing span
  -> 调 ToolBridge / runtime dispatch，消费 Progress 与 Terminal
  -> 更新前台任务、文件 hunk、UI/ACP 事件
  -> 将成功、错误、拒绝或取消规范化成 tool result
  -> ChatState 追加 tool result
  -> agentic loop 回到下一次 sampling
```

在本项目中，`SessionActor` 适合拥有以下策略，而具体工具不应绕过它们：

- 用户批准、拒绝和 permission mode；
- plan mode / stop gate 等会话级限制；
- 相同文件编辑的并发序列化（见 [tool_dispatch.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/tool_dispatch.rs)）；
- 用户取消、前台终端杀死与后台任务管理；
- ACP notification、telemetry 和 replay；
- 何时把错误作为「本轮终止」或「可让模型修正后继续」。

因此，工具实现中直接修改 ChatState、直接发送 ACP 事件或自行吞掉取消信号，通常都跨越了职责边界。除非该工具是明确的基础设施例外，否则应该把策略请求交回 session/runtime。

### 4.1 `PreparedToolCall` 的意义

`prepare_tool_call` 把不可信的模型输出转换为执行前的受控对象。它会在真正执行前集中处理名称和原始参数、工具类别以及调用条件。这样后续批处理、span、权限和错误呈现都可使用统一数据，避免各个具体工具重复解析模型 JSON。

当某个模型调用“根本没进工具函数”时，优先在这里和其上游的模型输出/definition 对照，不要只给 `run()` 加日志。

### 4.2 并发并非默认安全

`execute_tool_calls()` 可以批量运行调用，但 session 仍会区分必须串行的工作，且 `tool_dispatch.rs` 为同一文件的并发编辑提供序列化保护。工具能力声明、`ToolKind` 和 dispatch policy 必须如实表达读写行为。

一个实用准则：若两个调用的交换顺序可能改变工作区内容、权限提示顺序或模型下一步会看到的事实，就把它当作有依赖，而不是为了吞吐量强行并发。

---

## 5. 结果如何回到模型

一次工具调用有至少四种可观察结果：成功、工具错误、用户/策略拒绝、取消。它们不应简单地都变成 Rust `Err`：

- **工具域错误**（参数错误、命令失败、远端失败）往往应形成模型可读的 error result，让 Agent 能修正参数或换方法；
- **拒绝** 需要保留拒绝原因，否则模型会无意义重试；
- **取消** 由 session 将正在运行的工作停止，并给本轮一个一致结局；
- **成功** 产生结构化输出和 prompt-ready 文本，并可带 `system-reminder` 等后处理。

`ToolRunResult` / `ToolBridgeResult` 的 `prompt_text` 是重新采样时最关键的载荷。它通常被截断或清洗，以保护上下文窗口和避免把无界终端输出塞进模型。默认大小常量在 [xai-grok-tools/src/lib.rs](../../crates/codegen/xai-grok-tools/src/lib.rs)；不要为了让一个工具“多返回一点”随意解除全局上限，应设计摘要、分页、文件引用或后续读取接口。

最终由 [xai-chat-state](../../crates/codegen/xai-chat-state/src/actor/mutations.rs) 持久化 conversation item。ChatState 才是对话历史的权威源；下一次 `build_conversation_request` 再决定哪些项目进模型请求。详情见 [04-context-management.md](../04-context-management.md)。

---

## 6. 内置工具、MCP 和 Tool Protocol 不要混为一谈

| 概念 | 解决的问题 | 典型代码 |
|---|---|---|
| 内置 Tool | 在当前进程内执行一个具体能力 | `xai-grok-tools/implementations/` |
| MCP | 发现和调用外部 MCP server 提供的工具 | `xai-grok-mcp`、shell `session/.../mcp.rs` |
| Tool Protocol | Computer Hub 等服务之间的线协议、session/通知/hook 帧 | `xai-tool-protocol` |
| Tool runtime | 不依赖具体产品的类型化执行与流式抽象 | `xai-tool-runtime` |

MCP 工具最终也会被注册到当前 `ToolBridge`，所以从 session 视角看它们共享「模型调用 → 结果回写」的主路径；但连接管理、命名空间、server 生命周期和错误来源不同。修改线协议帧时应检查 `xai-tool-protocol` 的序列化测试和所有 adapter，不能只用一次本地 Tool 调用作验证。

---

## 7. 新增一个简单内置工具的最小清单

以只读、无后台任务的工具为例，按以下顺序实现和验证：

1. 在 `implementations/grok_build/<name>/` 定义 `Args`（`Deserialize + JsonSchema`）和输出类型；
2. 实现 `Tool`：稳定 `id`、准确 `description`、正确的 `capabilities` 和 `run` 或 `execute`；
3. 需要 session 资源时，用现有 `Resources` / typed params 机制注入，不要新建全局可变单例；
4. 在 `ToolRegistryBuilder` 注册，必要时加入 feature/config gate；
5. 检查 Agent allowlist、`disallowedTools`、名字覆盖和 plan mode 是否使其不可见；
6. 为 schema/definition、成功、错误和权限/取消边界写测试；
7. 运行 `cargo fmt --all`、目标 crate 的 `cargo test -p ...` 与 `cargo clippy -p ...`；
8. 从实际 session 做一次 smoke test，确认模型能看见 definition、调用后能读到结果。

复杂工具（流式、文件编辑、任务、远程网络）还需检查重试、输出截断、并发、权限及取消。不要把这些责任藏在一个很长的 `run()` 中；优先沿已有同类实现复制其边界和测试方式。

---

## 8. 高频故障的逆向定位表

| 现象 | 优先检查 | 常见根因 |
|---|---|---|
| 模型从未调用新工具 | definition、描述、schema、Agent allowlist | 没注册、被 gate 过滤、描述不可用、名称不一致 |
| 模型调用报 unknown tool | client-facing name 到 registry 映射 | name override/MCP namespace/兼容名称未同步 |
| 工具被调用但函数未进入 | `prepare_tool_call`、permission/plan gate | 预检拒绝、参数无法解析、模式不允许 |
| UI 看到结果，模型下轮却不知道 | `ToolRunResult.prompt_text`、ChatState mutation | 只填了 UI 输出、未追加 tool result、过度截断 |
| 模型不停重试失败调用 | 错误结果文本、ToolError 映射、description | 错误不可操作、把拒绝伪装成成功、参数约束不清楚 |
| 两个编辑工具互相覆盖 | capabilities、`tool_dispatch.rs`、文件锁 | 错报只读/可并发，或绕过 session 调度 |
| 取消后命令仍在跑 | bridge 的 terminal 处理、tool cancellation | 工具没有传播取消或后台/前台任务分类错误 |

---

## 9. 建议的阅读断点

第一次阅读不要试图把 `tool_calls.rs` 的所有特殊分支记住。按下面三次停顿建立模型：

1. 读 `Tool` trait 和一个小的只读实现，理解 **类型和 schema**；
2. 读 registry `register_with_params` 到 `finalize`，理解 **为什么这个 session 看得到工具**；
3. 读 `process_conversation_turn` 中 `execute_tool_calls` 的调用点与 `tool_calls.rs`，理解 **为什么 session 是策略所有者**。

此时再读 MCP、后台任务、subagent 或 plan mode 的特殊分支，才不会把它们误当作所有工具的基础路径。

## 10. 可运行的缩小模型

先预测成功、拒绝和缺少 Terminal 三条路径，再运行：

```sh
cargo run --locked \
  --manifest-path docs/rust-essentials/labs/async-demos/Cargo.toml \
  --bin mini_tool_pipeline
```

[`mini_tool_pipeline.rs`](../rust-essentials/labs/async-demos/src/bin/mini_tool_pipeline.rs) 保留以下边界：

```text
模型 name + JSON args
  -> prepare：名称解析 + serde 强类型解码
  -> permission policy
  -> Progress* + exactly one Terminal
  -> UI summary + prompt_text
  -> ChatState tool result
```

程序还构造了一条只有 Progress、随后 channel 关闭的错误流，并断言它不能被当作成功。示例没有实现真实 `ToolRegistryBuilder`、schema 生成、MCP、文件读取或 ACP 通知；它用于掌握契约顺序，生产证据仍应来自 runtime/registry/session 的 focused tests。
