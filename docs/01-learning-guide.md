# 01 · 新手学习指南：从运行到读懂一次请求

这是一篇面向**第一次接触 Rust 大型 workspace 或 Agent 运行时**的入门指南。目标不是一次读完仓库，而是在完成后能做到三件事：

1. 能在本地构建并启动程序，知道失败时先检查什么；
2. 能从用户输入一路追到模型调用和工具执行；
3. 能为一个小改动找到合适的 crate，并用最小范围的命令验证它。

深入的架构、Agent Loop、上下文与协议内容分别见 [02-architecture.md](./02-architecture.md)、[03-agent-loop.md](./03-agent-loop.md)、[04-context-management.md](./04-context-management.md) 与 [06-interfaces.md](./06-interfaces.md)。想专门理解用户消息如何穿过 TUI、ACP、SessionActor 和 ChatState，可读 [源码精读/message-flow.md](./deep-dives/message-flow.md)；想理解 `grok -p`、relay headless、stdio 和 Leader 的真实生命周期，可读[宿主模式与运行入口](./deep-dives/host-modes-and-entrypoints.md)；想理解取消、超时、工具进程和子代理如何收尾，可读[取消、超时与关闭](./deep-dives/cancellation-and-shutdown.md)；想按 workspace package 反查代码，读 [10-crate-catalog.md](./10-crate-catalog.md)；想动手练习，读 [11-guided-exercises.md](./11-guided-exercises.md)。外部资料和进一步阅读入口见 [参考资料.md](./参考资料.md)。

---

## 1. 先建立全局印象

### 1.1 项目是什么

**Grok Build** 是一个终端 AI 编程 Agent。发布时的命令一般叫 `grok`，从本仓库构建时的二进制产物叫 `xai-grok-pager`。

它有三种使用方式：

| 使用方式 | 典型命令或入口 | 适用场景 |
|---|---|---|
| 交互 TUI | `grok` | 在终端中聊天、查看工具进度、批准操作 |
| 单轮结果 | `grok -p "..."` | 脚本、CI、一次 prompt；输出 plain/JSON/流式 JSON |
| Relay Agent | `grok agent headless` | 长期在线的 Grok.com relay Agent；需要 session |
| 编辑器 Agent | `grok agent stdio` | 通过 ACP 嵌入 IDE；stdout 只能是协议 |
| 共享宿主 | `grok agent leader` | 多客户端共享 Agent/Session/Workspace；本机 Unix socket |

它不是“调用一次模型 API 的脚本”。一次用户请求可以反复执行以下循环：模型回复 → 请求工具 → 本地执行工具 → 把结果回传模型，直至模型不再请求工具、用户取消或达到限制。

```text
用户输入
  -> Pager/TUI 或 headless CLI
  -> Shell 的 SessionActor（会话编排）
  -> ChatState 组装上下文
  -> Sampler 请求模型并接收流式响应
  -> ToolBridge 执行工具或请求权限
  -> 工具结果写回 ChatState
  -> 继续采样，或结束本轮
```

阅读源码时，先记住这条链路；其余模块大多是在支持、扩展或展示它。

### 1.2 仓库为何看起来很大

本仓库是从内部 monorepo 周期性同步的 Rust 源码树，含约 75+ 个 workspace member。它把“产品功能”拆为多个 crate：

| 你关心的问题 | 优先看的 crate |
|---|---|
| 程序从哪里启动、如何处理 CLI 参数 | `xai-grok-pager-bin` |
| 终端 UI 怎么渲染和接收输入 | `xai-grok-pager` |
| 一次会话、权限、持久化如何编排 | `xai-grok-shell` |
| Agent、系统提示词与工具集如何组成 | `xai-grok-agent` |
| 读文件、编辑、命令、搜索等工具如何实现 | `xai-grok-tools` |
| 文件系统、Git、worktree 与权限 | `xai-grok-workspace` |
| 对话历史、请求裁剪与上下文压缩 | `xai-chat-state`、`xai-grok-compaction` |

新手不需要读完所有 crate，更不需要从 `third_party/` 开始。先顺着一个可见行为找到它的入口，再沿依赖和调用跳转。

---

## 2. 读代码前需要知道什么

### 2.1 最小 Rust 前置知识

不必先完整学完 Rust，但以下概念会频繁出现。不了解时先查概念，再回到当前文件，不要卡在一处。

| 概念 | 在本项目里意味着什么 |
|---|---|
| `Result<T, E>` 与 `?` | 错误沿调用链上抛；先看调用方如何处理错误 |
| `struct` / `enum` / `match` | 状态和消息类型；Actor 命令通常是枚举 |
| trait | 接口约定，例如工具的统一 `Tool` 接口 |
| `async fn` / `.await` | I/O 不阻塞线程；模型流、文件和网络调用都常见 |
| Tokio task / channel | 后台任务和组件通信；常见 `mpsc`、`oneshot`、`broadcast` |
| `Arc` | 多个异步任务共享只读或同步访问的数据 |
| feature | 编译期开关；阅读 `Cargo.toml` 时注意 `default` 与可选依赖 |

推荐顺序：先通过 [Rust 必备知识](./rust-essentials/README.md) 的能力自测找到短板，掌握 ownership、`Result`、trait、async 基础，再阅读本仓库的 Actor 代码。遇到生命周期或泛型很复杂的实现时，先理解它的输入、输出和职责，不必立刻逐字符推导类型。

### 2.2 先认识几个词

| 名词 | 这份代码中的含义 |
|---|---|
| crate | 一个 Rust 包，通常对应一个目录和一个 `Cargo.toml` |
| workspace | 将很多 crate 放在一起统一构建、锁定依赖版本的仓库 |
| TUI | Terminal UI，终端内的全屏交互界面 |
| Session | 一段可持续的会话，持有对话、配置、工具、权限等状态 |
| Turn | 一次用户输入触发的处理单元；一个 Turn 可以包含多次模型调用 |
| Actor / Handle | Actor 独占状态并处理消息；Handle 是其他模块与它通信的句柄 |
| Sampling | 向 LLM 后端发送请求并消费流式响应 |
| MCP | Model Context Protocol，外部工具服务器接入协议 |
| ACP | Agent Client Protocol，编辑器与 Agent 间的会话协议 |
| Compaction | 上下文将超限时，用摘要替换较早历史，而不是直接丢弃 |

---

## 3. 先建立与改动匹配的反馈

### 3.1 只改文档时不要构建

`.md`、`docs/` 索引、源码阅读笔记和链接修正不会改变 Rust 编译输入。此类改动的反馈应是：

```text
编辑 Markdown
  -> git diff --check
  -> 相对链接存在性检查
  -> fenced code block / 表格格式检查
  -> 人工通读引用的源码路径
```

不要因为文档中出现 Rust 代码片段就运行 `cargo build`；代码围栏是解释材料，不会被 Cargo 编译。只有当本次改动涉及 `.rs`、`Cargo.toml`、`Cargo.lock`、build script、生成输入或会改变运行时行为的资源时，才选择对应的 `cargo check`、focused test 或 build。构建产物、网络下载和测试临时目录也会增加无关噪声，不能代替 Markdown 静态校验。

### 3.2 代码改动时再准备第一次构建

### 3.3 环境要求

仓库的 [rust-toolchain.toml](../rust-toolchain.toml) 固定 Rust 版本；当前为 `1.94.0`。建议用 `rustup` 管理，而不是手动安装一个不受管理的 `rustc`。

还需要 DotSlash，`bin/protoc` 等 hermetic 工具通过它解析。首次构建可能下载 Rust 组件和依赖，请确保网络可用。

```sh
# 安装/确认 Rust 工具链；若已安装 rustup，可跳过安装步骤
rustup --version
rustc --version
cargo --version

# 安装并验证 DotSlash（只需一次）
cargo install dotslash
dotslash --help
```

支持的主要构建主机是 macOS 和 Linux；Windows 是 best-effort。完整要求与官方构建命令以根 [README.md](../README.md) 为准。

构建时出现 `dotslash` 或 `protoc` 缺失错误，可直接按 [08-build-troubleshooting.md](./08-build-troubleshooting.md) 的实际案例修复。

### 3.4 推荐的第一组命令

在仓库根目录运行。先用 `cargo check`，它会做类型检查但不链接最终可执行文件，反馈通常比 `build` 更快。

```sh
# 验证最接近最终产品的入口 crate
cargo check -p xai-grok-pager-bin

# 真正构建并启动 TUI；首次启动通常会打开浏览器完成认证
cargo run -p xai-grok-pager-bin

# 发行构建；产物位于 target/release/xai-grok-pager
cargo build -p xai-grok-pager-bin --release
```

认证、模型可用性或网络问题不妨碍你阅读和编译大多数本地代码；如果目的只是学习源码，先以 `cargo check` 和局部测试为主。

### 3.5 常用开发命令

把 `<crate>` 替换成正在修改或阅读的 crate 名，例如 `xai-grok-agent`。

```sh
# 快速类型检查
cargo check -p <crate>

# 单元测试；可再加测试名过滤，例如 cargo test -p xai-grok-agent parser
cargo test -p <crate>

# 运行 linter
cargo clippy -p <crate>

# 格式化整个 workspace；提交前再运行
cargo fmt --all

# 查看 Cargo 识别到的 workspace 与包，不执行编译
cargo metadata --no-deps --format-version 1
```

### 3.6 常见的第一次失败

| 现象 | 先检查什么 |
|---|---|
| `dotslash` / `protoc` 找不到 | `dotslash --help` 是否可用；再检查根 README 的 DotSlash 安装说明 |
| Rust 版本不匹配 | 在根目录执行 `rustup show`，确认已读取 `rust-toolchain.toml` |
| 依赖下载失败 | 网络、代理或企业证书配置；不要用手工改版本来绕过 |
| 链接失败或磁盘不足 | `target/` 很大；检查磁盘空间和系统 C/C++ 工具链 |
| 启动后无法使用模型 | 这是认证、网络或账号配置问题，先看 pager user guide 的 authentication / troubleshooting |
| 只改了一个 crate 却报另一个 crate 的错 | 确认当前目标的依赖闭包；从报错中第一个本地 crate 开始处理 |

---

## 4. Cargo workspace：怎样改、怎样验证

### 4.1 根 `Cargo.toml` 为什么不要日常手改

根 [Cargo.toml](../Cargo.toml) 明确标注为自动生成的 workspace root。它汇总：

- workspace 成员列表；
- `[workspace.dependencies]` 中的统一依赖版本；
- 公共 package 配置、lint 和 profile。

这个公开同步树没有包含生成器。它在上游 monorepo 重新生成并同步时，根文件的手工修改可能被覆盖。因此，**日常功能改动应修改目标 crate 目录中的 `Cargo.toml`**，而不是把根文件作为常规入口。

### 4.2 crate 的依赖应如何声明

```toml
[dependencies]
# 已由 workspace 统一版本管理的第三方依赖
serde = { workspace = true }

# 本仓库中的另一个 crate
xai-grok-tools = { path = "../xai-grok-tools" }

# 需要和全局版本/feature 隔离的例外依赖
some-library = { version = "1", features = ["feature-a"] }
```

优先复用已有的 workspace 依赖，避免同一个库被随意引入多个版本或不同默认 feature。新增 workspace member、统一版本或全局 lint 的变更，需要在上游的生成源维护；在这个同步树中不要猜测生成脚本的位置。

`Cargo.lock` 是 Cargo 根据 manifest 解析出来的锁文件。让 Cargo 更新它，不要手工编辑。

### 4.3 为什么日常命令建议使用 `-p <crate>`

`-p` 是 `--package` 的简写，用来指定本次命令的**顶层目标包**。

```sh
cargo check -p xai-grok-agent
```

这条命令不会只看一个目录：Cargo 仍会检查 `xai-grok-agent` 所需的全部依赖；若你改了它依赖的底层 crate，Cargo 也会重新检查该依赖。它省下的是与当前目标无关的其他 workspace 包，尤其是反向依赖者。

相反，在这个 virtual workspace 根目录直接运行 `cargo build` 或 `cargo check`，默认目标覆盖 workspace 成员，Cargo 需要处理更大的依赖图、更多 fingerprint 检查和更多可能受影响的上层 crate。即使增量构建会复用未变产物，全量命令也更慢、报错范围也更大。

日常循环建议是：

```text
改 xai-grok-agent 的代码
  -> cargo check -p xai-grok-agent
  -> cargo test -p xai-grok-agent
  -> 必要时 cargo check -p xai-grok-shell（验证直接使用者）
```

以下场景再运行约定的全 workspace 检查：修改共享类型或宏、改 workspace 依赖/feature、准备发布、或 CI 要求。`cargo fmt --all` 是例外，它本来就应该覆盖整个 workspace。

---

## 5. 先跑一次，再开始读代码

### 5.1 从入口看运行模式

打开 [crates/codegen/xai-grok-pager-bin/src/main.rs](../crates/codegen/xai-grok-pager-bin/src/main.rs)，先不要读完整文件。搜索以下函数名：

```text
fn main
run_headless
run_stdio_agent
run_leader
```

你会看到二进制 crate 主要负责解析命令、初始化运行环境并根据模式分发：

```text
xai-grok-pager-bin/src/main.rs
  ├─ TUI 交互       -> xai-grok-pager
  ├─ grok -p        -> xai-grok-pager::headless::run_single_turn
  ├─ agent headless -> xai-grok-shell::agent::app::run_headless
  ├─ agent stdio    -> xai-grok-shell::agent::app::run_stdio_agent
  └─ agent leader   -> xai-grok-shell::agent::app::run_leader
```

**检查点**：能解释为什么 `pager-bin` 是 composition root：它把各库拼成产品，不应承载大部分业务逻辑。

### 5.2 不要从 UI 细节进入核心逻辑

TUI 的布局、键盘事件和绘制文件很多。刚开始应把它看成“用户输入和会话事件的客户端”，先转向 `xai-grok-shell`，那里才是一次请求的编排中心。

可以这样搜索，而不是用文件树盲目翻找：

```sh
# 定位会话主循环和一次请求的处理函数
rg -n "run_session|handle_prompt|process_conversation_turn" \
  crates/codegen/xai-grok-shell/src/session

# 定位某个类型、方法或错误消息的定义和调用点
rg -n "SessionActor" crates/codegen/xai-grok-shell
```

---

## 6. 用一条请求链路阅读源码

这一节是核心练习。每次只读一个阶段，写下“输入是什么、输出是什么、状态在哪里变化”，再进入下一阶段。

### 第 1 步：Session 接收请求

从 `xai-grok-shell/src/session/acp_session.rs` 开始，它组织 `SessionActor` 的模块。接着看 `acp_session_impl/run_loop.rs`。

关注点：

- 外部如何通过命令和事件与 SessionActor 通信；
- 为什么使用 `tokio::select!` 同时等候用户输入、取消、后台事件和定时器；
- 哪些状态只由 SessionActor 自己修改。

**产出**：画出“客户端 -> SessionActor -> 客户端更新”的箭头即可，不必先理解每种命令。

### 第 2 步：Turn 如何开始

打开 `acp_session_impl/turn.rs`，找到 `handle_prompt`。这是一次用户 prompt 的中层编排。

先识别这几类工作：解析 slash command、加载 skill 或规则、准备用户消息、选择模型/Agent、检查权限或计划模式、开始采样。看到不懂的 helper 时，先记下名字，等主控制流读完再展开。

**产出**：能区分 Session（长期存在）和 Turn（一次输入的处理）。“一个 Turn 可能调用模型多次”是理解 Agent 的关键。

### 第 3 步：模型和工具的内层循环

继续读 `acp_session_impl/sampler_turn.rs` 与 `acp_session_impl/tool_calls.rs`。此处发生：

```text
构建 ConversationRequest
  -> Sampler 流式返回文字、tool call、usage 或 stop reason
  -> 结果写进 conversation
  -> 有 tool call：执行并写回 ToolResult，然后继续循环
  -> 没有 tool call：结束本次 Turn
```

不要把“模型生成文字”与“工具调用”分成两套系统。两者都是一次 sampling 响应的一部分；不同之处在于工具调用会让 Session 决定继续下一轮。

**产出**：能回答“工具结果为什么必须写回 conversation？”答案是：下一次模型请求需要看到该结果，才能继续推理。

### 第 4 步：上下文从哪里来

看 `xai-chat-state/src/lib.rs` 和 `xai-chat-state/src/actor/request_builder.rs`。

ChatState 是 conversation 的权威状态，而不是散落在所有 UI 或工具模块里的 `Vec<Message>`。在真正发请求前，它会构造 ConversationRequest，并进行图像预算、工具结果裁剪、memory 注入等处理。

当历史接近模型上下文限制时，再转到 `xai-grok-compaction` 和 shell 的 `session/compaction.rs`。Compaction 是长期历史的摘要替换；请求级 pruning 则是本次请求的临时裁剪，两者不要混淆。

**产出**：能区分“保存的会话历史”“发给模型的请求”“压缩后的历史”。

### 第 5 步：工具和权限落到哪里

按下面顺序进入工具层：

1. `xai-grok-tools/src/bridge.rs`：工具如何被登记、选择和调用；
2. `xai-tool-runtime`：统一工具 trait 和调度契约；
3. `xai-grok-tools/src/implementations/`：具体读文件、编辑、终端等实现；
4. `xai-grok-workspace/src/permission/`：权限决策与工作区边界。

**产出**：能回答“模型不会直接写磁盘，为什么？”模型只产生结构化 tool call；宿主经过工具注册、参数校验和权限流程后才执行本地操作。

---

## 7. 核心概念速查

| 概念 | 简明解释 | 深入阅读 |
|---|---|---|
| Session | 长期会话容器，编排输入、采样、工具、权限和持久化 | [03-agent-loop.md](./03-agent-loop.md) |
| Turn | 一次 prompt 的处理；内部可有多轮采样/工具 | [03-agent-loop.md](./03-agent-loop.md) |
| Agent | prompt、工具、模型和策略的可移植组合 | `xai-grok-agent/README.md` |
| ToolBridge | 产品层的工具注册和调用桥 | [06-interfaces.md](./06-interfaces.md) |
| Sampler | 与模型后端通信、消费流和重试的运行时 | [06-interfaces.md](./06-interfaces.md) |
| ChatState | conversation 的单一权威源 | [04-context-management.md](./04-context-management.md) |
| Compaction | 用摘要更新长期历史，释放上下文预算 | [04-context-management.md](./04-context-management.md) |
| ACP | 编辑器客户端与 Agent 的会话协议 | [06-interfaces.md](./06-interfaces.md) |
| Leader | 本机常驻控制面，支持多客户端/会话协作 | [02-architecture.md](./02-architecture.md) |
| MCP | 外部工具服务器的接入方式 | [06-interfaces.md](./06-interfaces.md) |

Skills、Plugins、Hooks 与 `AGENTS.md` 都会影响 Agent 行为，但角色不同：

| 扩展点 | 解决的问题 |
|---|---|
| `AGENTS.md` / rules | 给当前项目注入长期指令 |
| Skill | 可复用的任务知识、提示和可选工具策略 |
| Plugin | 打包并分发技能、MCP、扩展能力 |
| Hook | 在工具或会话生命周期节点运行脚本 |

它们如何进入 prompt，见 [05-prompt-engineering.md](./05-prompt-engineering.md)。

---

## 8. 两条可执行学习路线

### 路线 A：只想理解设计（约 2 天）

1. 完成第 3 节的一次 `cargo check`；
2. 阅读 [02-architecture.md](./02-architecture.md) 的总览和分层；
3. 按第 6 节读完一条请求链路；
4. 阅读 [03-agent-loop.md](./03-agent-loop.md) 的控制流伪代码；
5. 对照 [07-module-map.md](./07-module-map.md) 回答每层各自负责什么。

完成标准：能在不看文档的情况下画出“UI、Shell、Sampler、Tool、Workspace、ChatState”的关系图。

### 路线 B：准备做第一个改动（约 3 至 5 天）

1. 完成路线 A；
2. 选择一个边界清晰的目标，例如文案、一个 slash command、一个 Agent definition，或单个工具的参数校验；
3. 从 `rg` 找到定义、调用点和现有测试；
4. 先阅读同目录中最接近的测试，再修改最小范围代码；
5. 按 crate 执行 `cargo fmt --all`、`cargo test -p <crate>` 和 `cargo clippy -p <crate>`；
6. 若改动跨越 crate 边界，再检查直接使用者。

不要把第一次练习选成 Session 主循环、权限策略、持久化迁移或 workspace 根依赖变更。这些改动影响面大，且需要完整的回归验证。

### 可选练习

| 练习 | 你会学到什么 | 建议验证 |
|---|---|---|
| 为一个已有 Agent definition 增加说明文字 | YAML frontmatter、prompt 组装 | `cargo test -p xai-grok-agent` |
| 调整一个局部 TUI 文案 | pager 的视图与事件更新 | `cargo check -p xai-grok-pager` |
| 给纯函数增加测试 | Rust 单测、模块可测试性 | `cargo test -p <crate> <test-name>` |
| 跟踪一个工具名 | 注册、模型工具定义、实现和权限边界 | `rg -n "<tool_name>" crates/codegen` |

---

## 9. 读大型 Rust 代码的实用方法

### 9.1 每次只回答一个问题

好的问题：

- 用户按回车后，第一个发送消息的函数在哪里？
- `ToolResult` 在哪里写入 conversation？
- 某个权限弹窗由哪个事件触发？
- `AgentBuilder` 最终产出了哪些字段？

不好的起点：“我要读懂 `xai-grok-shell`。”后者没有明确终点，容易在细节中迷失。

### 9.2 先看边界，再看实现

打开一个模块时按以下顺序看：模块注释和 `pub use`、输入输出类型、调用点、测试、最后才是内部 helper。对于 Actor，先找命令枚举和 `Handle` 方法；对于 trait，先找实现者；对于大 `impl`，先找异步入口函数。

### 9.3 借助编辑器和搜索，而不是记忆文件树

```sh
# 找定义或引用
rg -n "struct SessionActor|enum SessionCommand|fn handle_prompt" \
  crates/codegen/xai-grok-shell/src

# 找一个 crate 的测试和模块入口
rg --files crates/codegen/xai-grok-agent | rg '(^|/)(lib|mod)\.rs$|tests|README\.md$'

# 查看一个目标包依赖哪些本地 crate
cargo tree -p xai-grok-agent
```

每读完一个阶段，写三行笔记：入口文件、关键状态、输出事件。这样的笔记比复制大量函数代码更有用。

---

## 10. 最终自检

能用自己的话回答以下问题，就已经建立了可继续深入的基础：

1. `xai-grok-pager-bin`、`xai-grok-pager` 和 `xai-grok-shell` 分别负责什么？
2. Session 和 Turn 有什么区别？为什么一个 Turn 会有多次模型调用？
3. 模型为什么不能直接执行本地命令或修改文件？
4. ChatState、请求级 pruning 与 Compaction 分别解决什么问题？
5. 为什么通常执行 `cargo check -p <crate>`，而不是在根目录无参数运行？
6. 为什么不应日常手改根 `Cargo.toml`？
7. 想调试某个工具的调用路径时，你会从哪些模块开始找？

接下来按兴趣选择深入方向：想理解控制流看 [03-agent-loop.md](./03-agent-loop.md)；想理解长会话看 [04-context-management.md](./04-context-management.md)；想改 prompt 或扩展看 [05-prompt-engineering.md](./05-prompt-engineering.md)；想查具体文件看 [07-module-map.md](./07-module-map.md)。
