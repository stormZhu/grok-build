# 源码精读

这里收录针对单个函数、控制流或子系统的源码精读文章。它们不是新手学习主线中的连续章节，而是建立整体架构之后，按问题深入阅读的专题材料。

## 当前专题

- [run-session.md](./run-session.md)：`SessionActor` 的 `run_session` 事件循环、Prompt/Turn 链路与关闭流程；包含可运行的最小 Session Actor 映射实验。
- [message-flow.md](./message-flow.md)：用户从按下 Enter 到 UI/ACP 完成响应的完整消息流程，含队列、图片、skills、ChatState、sampling 和工具循环。
- [tool-call-pipeline.md](./tool-call-pipeline.md)：从模型工具调用追到权限、注册表、流式执行、结果回写与下一次采样；包含 JSON/权限/Terminal 缩小实验。
- [resources-and-capability-injection.md](./resources-and-capability-injection.md)：区分 `ToolCallContext`、Grok tools `Resources` 和 shell `ToolContext`，追踪能力注入、持久化与 rebuild。
- [permissions-and-sandbox.md](./permissions-and-sandbox.md)：`AccessKind`、权限策略、YOLO/auto、plan gate 与 OS sandbox 的安全边界。
- [workspace-state-and-worktree-lifecycle.md](./workspace-state-and-worktree-lifecycle.md)：`WorkspaceSession`、文件 before/after snapshot、FS/git/hunk rewind、durable checkpoint 与 worktree fork/apply/remove；包含权限/冲突/失败缩小实验。
- [pager-rendering.md](./pager-rendering.md)：ACP `SessionUpdate` 如何增量合并为 `RenderBlock`，再由 scrollback/layout 渲染到终端。
- [mcp-lifecycle.md](./mcp-lifecycle.md)：MCP 配置、连接、工具发现、snapshot/reminder、调用、OAuth 和断线恢复；包含单飞握手/取消恢复实验。
- [mcp-dispatcher.md](./mcp-dispatcher.md)：聚焦 `McpClientEvent` 的 50ms tumbling window、client identity eviction、ACP server status、shutdown intent 与 stdio/HTTP recovery 分流。
- [persistence-and-replay.md](./persistence-and-replay.md)：`updates.jsonl`、`chat_history.jsonl`、`ReplayBuffer` 的分层，以及恢复、重放、compaction、rewind 和 fork；包含流式更新顺序实验。
- [sampling-lifecycle.md](./sampling-lifecycle.md)：从 `ConversationRequest`、Sampler Actor 和 SSE transform，追到流事件、重试、认证/compact recovery 与 tool-loop 接续；包含确定性重试实验。
- [prompt-assembly.md](./prompt-assembly.md)：从 Agent Definition、ToolBridge 和 `PromptContext` 追到 system prompt、首轮 preamble、AGENTS/rules、skills 与动态 reminder。
- [extensions-and-lifecycle.md](./extensions-and-lifecycle.md)：生命周期 contributor、hooks、plugins、skills 与 memory 如何在明确 owner 边界接入 turn，并保持权限、持久化和 loop 控制权不漂移。
- [leader-control-plane.md](./leader-control-plane.md)：从 lock/socket 竞争、registration 和 ID namespace，追到多 client session route、版本偏斜、断线重连与进程清理。
- [host-modes-and-entrypoints.md](./host-modes-and-entrypoints.md)：区分 `grok -p` 单轮 ACP 驱动器、`grok agent headless` relay 宿主、stdio 子进程和 Leader host/client 的所有权、协议与退出条件。
- [cancellation-and-shutdown.md](./cancellation-and-shutdown.md)：从 Session cancel、replay flush、Sampler request-id、工具进程、子代理到 compaction/workflow/MCP，建立取消、超时和关闭的不变量与测试证据；包含 Cancel/Shutdown owner 实验。
- [authentication-and-model-resolution.md](./authentication-and-model-resolution.md)：从配置和模型目录合并追到 `SamplerConfig`、BYOK/session token 隔离、OIDC/external refresh 与 turn 级 401 恢复；包含 endpoint gate/脱敏实验。
- [configuration-and-runtime-resolution.md](./configuration-and-runtime-resolution.md)：从 `ConfigLayers`、TOML 深度合并和 requirements/MDM，追到 `new_from_toml_cfg`、runtime resolver、远端 settings refresh 与 session snapshot；包含 merge/source/snapshot 缩小实验。
- [subagents-and-workflows.md](./subagents-and-workflows.md)：从 `task` 请求追到 child `SessionActor`、definition/role/persona、fork/resume、worktree 和 cancellation，再追到 Rhai Workflow 的 host service、journal、预算与恢复状态机；包含 journal replay/预算实验。
- [contributor-workflow.md](./contributor-workflow.md)：从症状、owner、mock fixture 和异步测试，建立可审查的开发/贡献证据链。
- [observability-and-trace-timeline.md](./observability-and-trace-timeline.md)：从 tracing span、ACP/HTTP `traceparent`、unified JSONL、debug firehose 和 OTLP 脱敏，反向还原一次用户消息的完整时间线；包含 task context/关联键/默认拒绝实验。

后续适合放入本目录的主题包括 MCP dispatcher 等。
