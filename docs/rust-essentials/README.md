# Grok Build 必备 Rust 知识

这组文档面向希望为 `grok-build` 阅读、审阅和完成中小改动的开发者。它不是从零开始的 Rust 教程：每个专题都以仓库中的真实设计为落点，并给出回到源码时应检查的问题。

## 贡献者主路径

按顺序阅读；读到异步部分后，可以一边读 [源码精读](../deep-dives/README.md) 中的 SessionActor 事件循环一边查阅专题。

| 阶段 | 必读专题 | 完成后应能做到 |
| --- | --- | --- |
| 语言与类型 | [所有权、借用与生命周期](./08-ownership-and-borrowing.md)、[类型建模与模式匹配](./09-type-modeling-and-patterns.md)、[trait、泛型与动态分发](./10-traits-generics-and-dyn.md)、[错误处理](./11-errors.md) | 解释一个值由谁拥有、一个错误如何传播、一个 trait 的边界为何存在 |
| 数据与工程 | [Serde 与线协议兼容性](./12-serde-and-wire-compat.md)、[Cargo workspace](./16-cargo-workspace.md)、[测试](./17-testing.md) | 安全修改配置、协议和 crate 依赖，并以最小范围验证 |
| 异步运行时 | [async、任务与 `Send`](./13-async-runtime-tasks.md)、[通道、取消与 Stream](./14-channels-cancellation-streams.md)、现有 01--07 专题 | 跟踪 Actor 消息、取消、状态共享和任务边界 |
| 维护与进阶 | [宏与 RAII](./18-macros-and-raii.md)、[平台、`unsafe` 与性能](./19-platform-unsafe-performance.md) | 修改底层或平台相关代码时知道先验证哪些不变量 |

## 专题索引

### 语言与建模

| 文档 | 要点 |
| --- | --- |
| [08 所有权、借用与生命周期](./08-ownership-and-borrowing.md) | 移动、`Clone`、借用、字符串与路径类型、生命周期的阅读方法 |
| [09 类型建模与模式匹配](./09-type-modeling-and-patterns.md) | `struct`、`enum`、`Option`、`Result`、`match`、迭代器和闭包 |
| [10 trait、泛型与动态分发](./10-traits-generics-and-dyn.md) | trait bound、关联类型、`impl Trait`、`dyn Trait`、`Send + Sync` |
| [11 错误处理](./11-errors.md) | `?`、错误上下文、恢复与 fail-closed 边界 |
| [12 Serde 与线协议兼容性](./12-serde-and-wire-compat.md) | derive、字段名、默认值、版本兼容与 JSON/TOML 边界 |

### Tokio 与并发

| 文档 | 要点 |
| --- | --- |
| [13 async、任务与 `Send`](./13-async-runtime-tasks.md) | Future、`.await`、`spawn`、`spawn_local`、任务生命周期 |
| [14 通道、取消与 Stream](./14-channels-cancellation-streams.md) | `mpsc`、`oneshot`、`watch`、`broadcast`、背压、`CancellationToken` |
| [01 tokio::select!](./01-tokio-select.md) | 多路等待、`biased;`、条件守卫、模式匹配 |
| [02 Pin 与 Future](./02-pin-and-future.md) | `tokio::pin!`、`Sleep::reset()`、可移动性 |
| [03 Atomic 与 Ordering](./03-atomic-ordering.md) | 原子状态与内存序 |
| [04 spawn_local](./04-spawn-local.md) | 单线程 `LocalSet` 与 `!Send` 状态 |
| [05 watch 通道](./05-watch-channel.md) | 最新值通知与 `borrow_and_update()` |
| [06 共享状态](./06-arc-mutex-rwlock.md) | `Arc`、锁、`RefCell` 与跨 `.await` 风险 |

### 工程实践与进阶

| 文档 | 要点 |
| --- | --- |
| [07 tracing](./07-tracing.md) | 结构化字段、span、任务上下文 |
| [16 Cargo workspace](./16-cargo-workspace.md) | package、feature、依赖边界与局部验证 |
| [17 测试](./17-testing.md) | 同步/异步测试、mock、临时目录、环境变量隔离 |
| [18 宏与 RAII](./18-macros-and-raii.md) | `derive`、声明式宏、`Drop` 清理 guard |
| [19 平台、unsafe 与性能](./19-platform-unsafe-performance.md) | 文件描述符、FFI、安全不变量、测量先行 |

## 每篇文章的使用方法

先读“什么时候使用”和最小示例，再打开项目锚点，回答：输入/输出是什么？谁拥有状态？错误或取消会到哪里？最后完成“阅读检查点”。遇到复杂生命周期或泛型时，先理解契约与边界，不要从类型细节开始逐字符推导。

本仓库使用 Rust 1.94、edition 2024。根 `Cargo.toml` 是自动生成的 workspace 根，日常改动应从目标 crate 的 manifest 和源码开始。
