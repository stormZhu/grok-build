# 源码精读

这里收录针对单个函数、控制流或子系统的源码精读文章。它们不是新手学习主线中的连续章节，而是建立整体架构之后，按问题深入阅读的专题材料。

## 当前专题

- [run-session.md](./run-session.md)：`SessionActor` 的 `run_session` 事件循环、Prompt/Turn 链路与关闭流程。
- [message-flow.md](./message-flow.md)：用户从按下 Enter 到 UI/ACP 完成响应的完整消息流程，含队列、图片、skills、ChatState、sampling 和工具循环。
- [tool-call-pipeline.md](./tool-call-pipeline.md)：从模型工具调用追到权限、注册表、流式执行、结果回写与下一次采样。
- [permissions-and-sandbox.md](./permissions-and-sandbox.md)：`AccessKind`、权限策略、YOLO/auto、plan gate 与 OS sandbox 的安全边界。
- [pager-rendering.md](./pager-rendering.md)：ACP `SessionUpdate` 如何增量合并为 `RenderBlock`，再由 scrollback/layout 渲染到终端。

后续适合放入本目录的主题包括采样循环、MCP dispatcher 和上下文压缩等。
