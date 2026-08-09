# 源码精读

这里收录针对单个函数、控制流或子系统的源码精读文章。它们不是新手学习主线中的连续章节，而是建立整体架构之后，按问题深入阅读的专题材料。

## 当前专题

- [run-session.md](./run-session.md)：`SessionActor` 的 `run_session` 事件循环、Prompt/Turn 链路与关闭流程。
- [message-flow.md](./message-flow.md)：用户从按下 Enter 到 UI/ACP 完成响应的完整消息流程，含队列、图片、skills、ChatState、sampling 和工具循环。
- [tool-call-pipeline.md](./tool-call-pipeline.md)：从模型工具调用追到权限、注册表、流式执行、结果回写与下一次采样。
- [permissions-and-sandbox.md](./permissions-and-sandbox.md)：`AccessKind`、权限策略、YOLO/auto、plan gate 与 OS sandbox 的安全边界。
- [pager-rendering.md](./pager-rendering.md)：ACP `SessionUpdate` 如何增量合并为 `RenderBlock`，再由 scrollback/layout 渲染到终端。
- [mcp-lifecycle.md](./mcp-lifecycle.md)：MCP 配置、连接、工具发现、snapshot/reminder、调用、OAuth 和断线恢复。
- [persistence-and-replay.md](./persistence-and-replay.md)：`updates.jsonl`、`chat_history.jsonl`、`ReplayBuffer` 的分层，以及恢复、重放、compaction、rewind 和 fork。
- [sampling-lifecycle.md](./sampling-lifecycle.md)：从 `ConversationRequest`、Sampler Actor 和 SSE transform，追到流事件、重试、认证/compact recovery 与 tool-loop 接续。
- [prompt-assembly.md](./prompt-assembly.md)：从 Agent Definition、ToolBridge 和 `PromptContext` 追到 system prompt、首轮 preamble、AGENTS/rules、skills 与动态 reminder。
- [contributor-workflow.md](./contributor-workflow.md)：从症状、owner、mock fixture 和异步测试，建立可审查的开发/贡献证据链。

后续适合放入本目录的主题包括 MCP dispatcher 等。
