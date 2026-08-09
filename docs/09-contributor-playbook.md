# 09 · 开发者实战手册：从问题到可验证改动

这篇文档的目标是让你能在这棵大型 Rust workspace 中完成一次可控改动：定位正确模块、理解边界、写最小测试、用和改动匹配的命令验证，并把问题描述给维护者。

> 公开仓库当前不接收外部 PR，权威说明见 [CONTRIBUTING.md](../CONTRIBUTING.md)。这里的“贡献”指在获授权的开发流程、下游 fork 或本地实验中产出可审查的改动；即便不能直接提交 PR，这套方法仍适用于提高质量 issue、复现报告和补丁建议。

架构总览见 [02-architecture.md](./02-architecture.md)，关键源码索引见 [07-module-map.md](./07-module-map.md)。如果你的改动涉及模型调用工具，先读 [Tool Call 精读](./deep-dives/tool-call-pipeline.md)。

---

## 1. 每次改动都先写下的五个问题

不要从搜索到的第一个函数直接修改。先在 issue、笔记或提交说明中回答：

| 问题 | 例子 | 用途 |
|---|---|---|
| 入口是什么？ | TUI 按键、CLI flag、ACP 请求、模型 tool call | 避免改到显示层却漏掉真正入口 |
| 权威状态在哪里？ | `ChatState`、`SessionActor`、workspace、tool resources | 避免产生第二份状态 |
| 外部契约是什么？ | CLI、JSON schema、ACP/MCP frame、持久化格式、prompt | 判断兼容性和测试边界 |
| 哪个组件拥有策略？ | Session 的权限/取消，Tool 的业务语义，Pager 的呈现 | 避免越层调用 |
| 最小可观察结果是什么？ | 单元测试、序列化断言、headless smoke、截图 | 让“改好了”可证伪 |

大多数返工来自前四项没有先说清。尤其在 Agent 系统中，代码路径通常同时受配置、prompt、模型输出和运行时权限影响，单看函数签名不足以判断行为。

---

## 2. 按症状定位：先到哪一个 crate

| 要改的行为 | 主要入口 | 紧邻的验证面 |
|---|---|---|
| CLI 参数、启动模式、顶层装配 | `xai-grok-pager-bin`、`xai-grok-pager` | 参数解析测试、headless/TUI smoke |
| 一次 prompt 的控制流、取消、回放 | `xai-grok-shell/session/` | session/turn 测试、tracing 日志 |
| 系统 prompt、Agent definition、AGENTS/skills | `xai-grok-agent` | prompt 渲染/definition 解析测试 |
| 模型请求、流式响应、重试 | `xai-grok-sampler`、`xai-grok-sampling-types`、shell `sampling/` | fixture、流解析、重试测试 |
| 历史、token 预算、压缩、恢复会话 | `xai-chat-state`、`xai-grok-compaction` | request builder/compaction 测试 |
| 内置工具、tool schema、工具输出 | `xai-grok-tools`、`xai-tool-runtime` | schema、registry、工具执行测试 |
| 本地文件/命令/Git/权限/worktree | `xai-grok-workspace`、`xai-grok-sandbox` | 临时目录集成测试、权限矩阵 |
| IDE 集成、ACP stdio | `xai-acp-lib`、shell `mvp_agent/`、`session/acp_*` | 协议转换和端到端 fixture |
| MCP server 集成 | `xai-grok-mcp`、shell session MCP 模块 | server fixture、动态注册、生命周期测试 |
| TUI 表现、markdown、键盘交互 | `xai-grok-pager`、`xai-grok-pager-render` | snapshot/渲染测试、人工终端 smoke |

这里的路径是职责入口，并不意味着只改一个 crate。跨 crate 改动要先沿依赖方向确认「谁定义类型/契约，谁实现，谁把它接到产品」；不要为了少改一个包把公共契约塞进上层。

---

## 3. 四种最常见改动的工作流

### 3.1 改一个内置工具

1. 找到 `xai-grok-tools/src/implementations/` 中最相近的工具；
2. 明确入参 schema、模型可见说明、读写能力、输出上限和取消语义；
3. 实现或调整具体 `Tool`；
4. 在 `ToolRegistryBuilder` 的注册表确认它能进入当前 session；
5. 检查 Agent 工具 allowlist、`disallowedTools`、命名覆盖和 plan mode；
6. 为成功、无效参数、业务失败和需要权限的路径加测试；
7. 用一个实际 session 验证 definition 可见、调用可达、结果会回到模型。

不要直接从模型回复字符串中手写解析或自己把结果拼回对话。工具执行的权限、并发和结果回写应经 session/runtime 主路径，细节见 [Tool Call 精读](./deep-dives/tool-call-pipeline.md)。

### 3.2 改 Agent 或系统提示

1. 区分要改的是主模板、某个 Agent definition、AGENTS.md 注入、skill 描述还是运行时 reminder；
2. 在 `xai-grok-agent/src/prompt/` 跟到 `PromptContext`，确认值从哪里产生；
3. 若改模板变量，确认 `TemplateRenderer` 及所有 prompt audience（primary/subagent）的渲染路径；
4. 评估 token 预算、敏感信息和 subagent 是否应继承该段内容；
5. 测试渲染出的最终文本/结构，不只测模板文件存在；
6. 用代表性配置验证 agent allowlist、skill 发现和模板覆盖仍正确。

提示词也是接口。一个字段名、工具名或角色指令的变化会影响持久化 session、plugins 和模型已学到的调用行为；用兼容层或版本化迁移，而不是静默改名。

### 3.3 改 Session / Agent Loop

1. 先确认这是外层 `run_session`、一次 `handle_prompt`，还是内层 `process_conversation_turn` 的问题；
2. 标出该状态属于 `SessionActor`、`ChatState`、Tool registry 还是后台 task；
3. 判断失败应结束 turn、给模型一个可恢复的 tool/error result，还是触发重试；
4. 明确取消、interjection、权限拒绝和 shutdown 在此处应发生什么；
5. 在最小层建立 deterministic 测试，避免把每个分支都写成真人 TUI 手测；
6. 需要跨 actor 时，给消息/事件加关联 ID 和 tracing 字段，便于复现时追踪。

不要把耗时 I/O 放进持有 actor 状态锁的同步区。Rust 编译器不能替你证明业务层没有把一个 session 卡住；审查 `.await` 前后拥有的借用、锁和取消路径。

### 3.4 改协议或持久化格式

这类改动风险最高，先画出 producer/consumer 列表：

```text
写入端 -> serde wire/storage -> 旧版本/外部 client -> 读取端 -> UI/Agent 行为
```

执行时遵守：

- 优先新增可选字段、`#[serde(default)]` 或明确版本，而不是删/改已有字段；
- 为旧 payload 和新 payload 都写反序列化测试；
- ACP、Tool Protocol、MCP 的变更分别检查对应 crate 和 adapter，不能把它们视为同一协议；
- session 持久化改动要考虑 resume、replay 和降级工具；
- 记录兼容窗口和回滚方式。

Rust 的类型安全只覆盖当前二进制内的调用，无法覆盖磁盘上旧 JSON、已安装插件和远程 client。

---

## 4. 测试策略：让覆盖范围匹配风险

### 4.1 从内向外建证据

| 层 | 适合验证什么 | 示例 |
|---|---|---|
| 纯函数/类型 | 边界、转换、schema、错误映射 | serde round-trip、参数校验、token 选择 |
| crate 内组件 | actor command、registry、stream/retry | mock backend、测试 channel、临时文件 |
| crate 间集成 | 调用者是否正确接线 | Agent build + ToolBridge、ACP conversion |
| 进程级 smoke | CLI 配置、真正入口、产物可启动 | `cargo run -p ... -- ...` 或 headless fixture |
| 人工交互 | TUI 布局、终端兼容、浏览器认证、真实权限体验 | 有限且带记录的手测 |

优先把核心判断放到前两层。完整 TUI/模型网络测试适合证明接线，但不应是唯一证据，因为它们更慢、更不稳定，也难覆盖失败分支。

### 4.2 常用命令和它们证明的范围

在仓库根目录执行，`<crate>` 用实际 package 名替换：

```sh
# 只检查当前 crate 及其依赖闭包，适合每次编辑后快速运行
cargo check -p <crate>

# 目标 crate 的测试；可加测试名称过滤
cargo test -p <crate>
cargo test -p <crate> <test_name_fragment>

# 目标 crate 的 lint；不会替代测试
cargo clippy -p <crate>

# 全 workspace 格式化
cargo fmt --all

# 确认 Cargo 识别到的包/feature 关系，不启动完整构建
cargo metadata --no-deps --format-version 1
```

`cargo check -p` 证明类型、名称和目标 crate 的依赖闭包能通过；它**不**证明测试、CLI 行为、序列化兼容或 prompt/tool 的模型效果。不要把一次 `check` 写成“功能已验证”。全 workspace 构建代价高，只有修改 workspace 范围配置、共享底层契约或发布前才应扩大验证范围。

根 `Cargo.toml` 是上游生成物，参见根 README；常规功能改动应修改具体 crate 的 manifest。不要手改 `Cargo.lock`，让 Cargo 解析后更新。

---

## 5. 调试 Agent 系统时收集什么证据

当行为依赖模型、工具或异步任务，最有价值的 bug 报告不是“它没工作”，而是一个可重放的时间线：

```text
输入 / 配置 / 工作区状态
  -> 选中的 Agent + prompt context（注意脱敏）
  -> model response / tool definition 名称
  -> permission / dispatch decision
  -> tool progress + terminal result
  -> ChatState 更新 / 下一轮行为
```

收集时遵守以下边界：

- 不记录 API key、OAuth token、私有源码、完整用户 prompt 或未经脱敏的工具输出；
- 用 session ID、tool call ID、trace/span 字段关联事件，而不是在日志中复制敏感正文；
- 对 streaming 问题记录事件顺序和终态是否存在；
- 对并发问题记录两个调用是否实际并发、访问的资源/路径和取消时点；
- 对 prompt 问题记录最终渲染结构和启用的工具列表，而不是只记录模板源文件。

本项目已广泛使用 `tracing` span；修改长期流程时，尽量沿用既有 span 名称和关联字段。Rust/异步追踪的背景可看 [rust-essentials/17-tracing.md](./rust-essentials/17-tracing.md)。

---

## 6. 代码审查清单

提交前，按改动类型做一次短审查：

- **所有改动**：是否改在状态/策略所有者处？错误能否被调用者处理？是否有与行为对应的测试？
- **async/actor**：是否在锁或可变借用跨 `.await` 时阻塞了其它工作？取消后是否留下任务或资源？
- **工具**：schema/description/能力声明是否匹配真实行为？拒绝和失败是否能让模型采取下一步？输出是否受限？
- **prompt/Agent**：primary 和 subagent 都正确吗？是否泄露宿主/隐私信息？是否打破工具名或模板变量兼容性？
- **持久化/协议**：旧数据和旧 client 怎么读？有没有默认值/版本/迁移测试？
- **workspace/权限**：路径、worktree、sandbox 和 permission mode 是否覆盖到？是否意外扩大可写范围？
- **测试**：每条命令实际覆盖什么？失败时最早的本地错误在哪？

这张清单刻意不把“跑全量测试”当作唯一答案。好的验证是针对改动风险建立多层证据，而不是执行一条范围不明的大命令。

---

## 7. 一个可重复的 90 分钟阅读/修改练习

选一个小而明确的行为，例如修改只读工具的错误文案或为既有 Agent definition 增加可选配置。按下面节奏练习：

1. 用 `rg` 搜到用户可见文本或 public 类型，画出入口到所有者的 3-5 个节点；
2. 阅读相邻测试，写下一个当前失败、改后应通过的断言；
3. 只改所有者 crate，除非类型契约确实需要向下移动；
4. 先运行测试名过滤，再运行 `cargo test -p <crate>` 和 `cargo clippy -p <crate>`；
5. 看 `git diff`，确认没有无关格式化和生成物；
6. 用一句话说明“输入是什么、状态在哪里改变、输出由谁观察”。

能稳定完成这类小改动后，再进入工具注册、Actor loop、协议兼容或跨 crate 重构。大仓库的熟练度来自不断验证边界，而不是记住所有目录。
