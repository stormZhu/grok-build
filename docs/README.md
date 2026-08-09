# Grok Build 源码研究文档

本目录是对 **Grok Build**（`grok` CLI / TUI）开源树的工程级分析，面向：

- 想从源码**学习编译、Rust 工程结构与核心概念**的开发者
- 想**研究设计架构、Agent Loop、上下文管理、提示词工程**的研究者
- 想对接 **ACP / Tool Protocol / Sampling API** 等接口的集成方

> 仓库说明：本树是 SpaceXAI monorepo 的周期性同步产物，首方代码 Apache-2.0。
> 官方产品文档见 [docs.x.ai/build](https://docs.x.ai/build/overview)；用户手册在
> `crates/codegen/xai-grok-pager/docs/user-guide/`。

验证原则：只修改 `docs/` 下的 Markdown 时，不需要运行 Cargo 构建或测试；做链接、围栏、空白和人工内容审查即可。只有 Rust、Cargo manifest、生成输入或运行时资源发生变化时，才按 [09-contributor-playbook.md](./09-contributor-playbook.md) 选择最小的代码验证范围。

---

## 文档索引

| 文档 | 内容 |
|------|------|
| [参考资料.md](./参考资料.md) | 外部学习资料与仓库内部阅读入口 |
| [Rust 必备知识](./rust-essentials/README.md) | 面向本仓库的 Rust 课程：能力自测、语言专题、异步并发、语法解码、API 导航与 8 关源码实验 |
| [01-learning-guide.md](./01-learning-guide.md) | 学习路径：编译、语言栈、核心概念、推荐阅读顺序 |
| [02-architecture.md](./02-architecture.md) | 整体架构、分层、crate 地图、进程/入口模型 |
| [03-agent-loop.md](./03-agent-loop.md) | Agent Loop：SessionActor、Turn、工具执行、生命周期钩子 |
| [04-context-management.md](./04-context-management.md) | 上下文管理：ChatState、Compaction、Pruning、Memory |
| [05-prompt-engineering.md](./05-prompt-engineering.md) | 提示词工程：模板、AGENTS.md、Skills、Subagent、加密与渲染 |
| [06-interfaces.md](./06-interfaces.md) | 对外/对内接口：ACP、Tool Protocol、Sampling、Leader、MCP |
| [07-module-map.md](./07-module-map.md) | 模块地图：按职责索引关键源码路径 |
| [08-build-troubleshooting.md](./08-build-troubleshooting.md) | 构建环境、DotSlash/protoc、Cargo 网络与首次编译排障 |
| [09-contributor-playbook.md](./09-contributor-playbook.md) | 开发实战：按改动类型定位、测试、调试、兼容性与代码审查 |
| [10-crate-catalog.md](./10-crate-catalog.md) | 依据 workspace metadata 的全 crate 目录、功能反查与贡献者导航 |
| [11-guided-exercises.md](./11-guided-exercises.md) | Rust 与 Agent 结合的源码实验：从 Actor、Prompt、Tool 到真实 turn 测试 |
| [12-glossary.md](./12-glossary.md) | Rust、Agent、Prompt、协议、安全与项目 owner 的统一术语索引 |
| [13-contribution-projects.md](./13-contribution-projects.md) | 从低风险文档/纯函数到 Tool、Session、协议、认证和扩展的阶段化贡献项目 |
| [源码精读](./deep-dives/README.md) | 针对具体函数、控制流和子系统的专题阅读；包含完整消息流、采样、认证与模型选择、配置分层/运行时解析、工具调用、MCP 生命周期/dispatcher、Leader、子代理/Workflow、权限沙箱、Workspace rewind/worktree、扩展生命周期、Pager 渲染、持久化重放和可观测性时间线 |

---

## 一句话定位

**Grok Build = 终端 AI 编程 Agent 运行时**：全屏 TUI（Pager）+ 无头/CI 模式 + ACP（Agent Client Protocol）嵌入 IDE；底层用 Rust Actor 模型驱动「采样 → 工具调用 → 写回会话 → 再采样」的多轮 Agent Loop，并配有完整的上下文压缩、权限沙箱与插件扩展体系。

---

## 建议阅读顺序

Rust 基础薄弱或尚不能独立解释仓库中的组合类型、trait 和异步任务边界时，先读第 1 项，再按 [Rust 必备知识](./rust-essentials/README.md) 的自测结果穿插学习；不需要读完全部专题才回到架构主线。

1. **01 学习指南** — 先能编译、能定位入口
2. **02 架构** — 建立分层心智模型
3. **03 Agent Loop** — 理解一次用户输入如何变成一串工具调用
4. **04 上下文** — 理解长会话如何不爆 context window
5. **05 提示词** — 理解行为如何被模板与规则塑造
6. **06 接口** — 对接或二次开发时查阅
7. **07 模块地图** — 按问题跳转到具体文件
8. **09 开发者实战手册** — 以一个可验证的小改动开始真正动手
9. **10 Crate 全目录** — 从 workspace package 反查职责、入口和验证命令
10. **11 源码实验手册** — 把 Rust 概念和 Agent 运行时练习连起来
11. **12 术语索引** — 遇到陌生词时先确认 Rust、Agent 和项目语义的对应关系
12. **13 阶段化贡献项目** — 按风险递进完成从文档、纯函数到协议和跨 session 改动
13. **源码精读 / message-flow** — 跟踪用户发送一条消息后的完整控制流
14. **源码精读 / persistence-and-replay** — 理解 session 落盘、恢复、重放、rewind 和 fork
15. **源码精读 / sampling-lifecycle** — 理解请求构建、流事件、重试与采样后如何接回工具循环
16. **源码精读 / prompt-assembly** — 理解 Agent 定义、规则、skills 如何进入模型上下文
17. **源码精读 / extensions-and-lifecycle** — 区分 hooks、plugins、skills、memory 与进程内 lifecycle 扩展的控制边界
18. **源码精读 / leader-control-plane** — 理解多客户端如何共享 Agent Host，以及 IPC/版本/重连的边界
19. **源码精读 / authentication-and-model-resolution** — 理解模型目录、凭据来源和 401 恢复为什么分层
20. **源码精读 / subagents-and-workflows** — 理解 definition、fork/resume、隔离 worktree、Rhai journal 和 Workflow 状态机
21. **源码精读 / mcp-dispatcher** — 理解 50ms 事件合并、client identity、防 stale close、ACP status 和自动恢复
22. **源码精读 / configuration-and-runtime-resolution** — 理解配置层级、深度合并、requirements/MDM、campaign、远端 settings 和热刷新
23. **源码精读 / workspace-state-and-worktree-lifecycle** — 理解文件快照、FS/git/hunk rewind、checkpoint durability 和 worktree 隔离
24. **源码精读 / observability-and-trace-timeline** — 学会用关联 ID、span、unified log、firehose 和 OTLP 还原一次消息
25. **源码精读 / contributor-workflow** — 学会按 owner 选测试、收集异步/协议证据并写出可审查改动
26. **源码精读 / resources-and-capability-injection** — 理解工具依赖、session 能力、取消和持久化资源如何注入与重建
27. **源码精读 / host-modes-and-entrypoints** — 区分 `grok -p`、relay headless、stdio 与 Leader 的真实入口和生命周期
28. **源码精读 / cancellation-and-shutdown** — 理解取消、超时、replay flush、工具进程和子代理关闭的跨层不变量

---

## 关键 crate 速查

| Crate | 角色 |
|-------|------|
| `xai-grok-pager-bin` | 二进制 composition root（`xai-grok-pager` / 发行名 `grok`） |
| `xai-grok-pager` | TUI：滚动区、输入框、模态、渲染 |
| `xai-grok-shell` | Agent 运行时：SessionActor、Leader、stdio/headless |
| `xai-grok-agent` | Agent 定义、System Prompt 组装、插件发现 |
| `xai-grok-tools` | 内置工具实现（读改文件、终端、搜索、任务…） |
| `xai-grok-sampler` | HTTP 流式采样 + 重试 Actor |
| `xai-chat-state` | 会话消息状态 Actor（conversation 权威源） |
| `xai-grok-compaction` | 与传输无关的压缩引擎 |
| `xai-tool-runtime` / `xai-tool-protocol` | 工具统一运行时契约与线协议 |
| `xai-grok-workspace` | 主机 FS / VCS / 权限 / checkpoint |

---

*文档基于本仓库源码静态分析生成，随内部 monorepo 同步可能略有滞后；以源码为准。*
