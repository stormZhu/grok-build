# Grok Build 源码研究文档

本目录是对 **Grok Build**（`grok` CLI / TUI）开源树的工程级分析，面向：

- 想从源码**学习编译、Rust 工程结构与核心概念**的开发者
- 想**研究设计架构、Agent Loop、上下文管理、提示词工程**的研究者
- 想对接 **ACP / Tool Protocol / Sampling API** 等接口的集成方

> 仓库说明：本树是 SpaceXAI monorepo 的周期性同步产物，首方代码 Apache-2.0。
> 官方产品文档见 [docs.x.ai/build](https://docs.x.ai/build/overview)；用户手册在
> `crates/codegen/xai-grok-pager/docs/user-guide/`。

---

## 文档索引

| 文档 | 内容 |
|------|------|
| [参考资料.md](./参考资料.md) | 外部学习资料与仓库内部阅读入口 |
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
| [源码精读](./deep-dives/README.md) | 针对具体函数、控制流和子系统的专题阅读；包含完整消息流、`run_session`、工具调用、权限沙箱与 Pager 渲染 |

---

## 一句话定位

**Grok Build = 终端 AI 编程 Agent 运行时**：全屏 TUI（Pager）+ 无头/CI 模式 + ACP（Agent Client Protocol）嵌入 IDE；底层用 Rust Actor 模型驱动「采样 → 工具调用 → 写回会话 → 再采样」的多轮 Agent Loop，并配有完整的上下文压缩、权限沙箱与插件扩展体系。

---

## 建议阅读顺序

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
11. **源码精读 / message-flow** — 跟踪用户发送一条消息后的完整控制流

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
