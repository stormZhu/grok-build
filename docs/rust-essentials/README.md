# Grok Build 必备 Rust 知识

这组文档面向希望为 `grok-build` 阅读、审阅和完成改动的开发者。它既不是一本按语法顺序抄写的 Rust 手册，也不要求你先学完整本 Rust Book：每个专题都以仓库中的真实设计为落点，直接内嵌带学习注释的关键代码节选，并给出回到完整上下文时应检查的问题。

第一次进入本目录时，先做 [学习路线与能力自测](./00-learning-roadmap.md)。如果眼前代码“每个词都认识，整句却读不懂”，直接打开 [Rust 高密度语法解码](./20-syntax-decoder.md)；如果不知道一个名字来自哪里、应从哪个文件继续追，读 [模块、可见性与 API 导航](./21-modules-and-api-navigation.md)；遇到所有权、trait 或 `Send` 报错时使用 [编译器错误诊断地图](./23-compiler-error-atlas.md)。每学完一个语言主题，运行 [Rust 可运行实验](./labs/README.md) 获得编译器和运行时反馈；最后用 [仓库源码实验](./22-repository-reading-labs.md) 检验自己是否真的能独立阅读。

准备从 Rust 机制切换到整个项目时，使用 [项目学习地图](../14-project-learning-map.md) 选择一条用户旅程；遇到“能编译但行为不对”时按 [运行时调试手册](../15-runtime-debugging-playbook.md) 从症状定位状态 owner 和等待边。

## 贡献者主路径

按顺序阅读；读到异步部分后，可以一边读 [源码精读](../deep-dives/README.md) 中的 SessionActor 事件循环一边查阅专题。

| 阶段 | 必读专题 | 完成后应能做到 |
| --- | --- | --- |
| 定位起点 | [学习路线与能力自测](./00-learning-roadmap.md) | 找到当前短板，建立“预测、验证、解释、修改”的学习循环 |
| 语言与类型 | [所有权、借用与生命周期](./01-ownership-and-borrowing.md)、[类型建模与模式匹配](./02-type-modeling-and-patterns.md)、[trait、泛型与动态分发](./03-traits-generics-and-dyn.md)、[错误处理](./04-errors.md) | 解释一个值由谁拥有、一个错误如何传播、一个 trait 的边界为何存在 |
| 数据与工程 | [Serde 与线协议兼容性](./05-serde-and-wire-compat.md)、[Cargo workspace](./06-cargo-workspace.md)、[测试](./07-testing.md) | 安全修改配置、协议和 crate 依赖，并以最小范围验证 |
| 异步运行时 | [async、任务与 `Send`](./08-async-runtime-tasks.md)、[通道、取消与 Stream](./09-channels-cancellation-streams.md)、10--17 专题 | 跟踪 Actor 消息、取消、状态共享和任务边界 |
| 维护与进阶 | [宏与 RAII](./18-macros-and-raii.md)、[平台、`unsafe` 与性能](./19-platform-unsafe-performance.md) | 修改底层或平台相关代码时知道先验证哪些不变量 |
| 综合读码 | [语法解码](./20-syntax-decoder.md)、[API 导航](./21-modules-and-api-navigation.md)、[源码实验](./22-repository-reading-labs.md) | 从公开入口独立追踪类型、调用、任务和错误边界，并提出可验证的小改动 |

## 专题索引

### 语言与建模

| 文档 | 要点 |
| --- | --- |
| [01 所有权、借用与生命周期](./01-ownership-and-borrowing.md) | 移动、`Clone`、借用、字符串与路径类型、生命周期的阅读方法 |
| [02 类型建模与模式匹配](./02-type-modeling-and-patterns.md) | `struct`、`enum`、`Option`、`Result`、`match`、迭代器和闭包 |
| [03 trait、泛型与动态分发](./03-traits-generics-and-dyn.md) | trait bound、关联类型、`impl Trait`、`dyn Trait`、`Send + Sync` |
| [04 错误处理](./04-errors.md) | `?`、错误链、Actor/task/wire 失败层、重试与恢复边界 |
| [05 Serde 与线协议兼容性](./05-serde-and-wire-compat.md) | wire shape、缺失/null、兼容矩阵、自定义反序列化与协议不变量 |

### Tokio 与并发

| 文档 | 要点 |
| --- | --- |
| [08 async、任务与 `Send`](./08-async-runtime-tasks.md) | Future/poll、`.await`、并发、spawn、JoinSet 与任务生命周期 |
| [09 通道、取消与 Stream](./09-channels-cancellation-streams.md) | channel 关闭、背压、协作式取消、Stream 不变量与 shutdown |
| [10 tokio::select!](./10-tokio-select.md) | 多路等待、`biased;`、条件守卫、模式匹配 |
| [11 Pin 与 Future](./11-pin-and-future.md) | `Future::poll`、`Unpin`、stack/heap pin、timer 复用与取消安全 |
| [12 spawn_local](./12-spawn-local.md) | 专用 session thread、current-thread runtime、`LocalSet` 与 local task 生命周期 |
| [13 watch 通道](./13-watch-channel.md) | 最新状态、版本、同值通知、关闭语义与 `borrow_and_update()` 竞态 |
| [14 共享状态](./14-arc-mutex-rwlock.md) | `Arc`、`Mutex`、`RwLock` 与仓库共享状态形状速查 |
| [15 内部可变性与锁](./15-interior-mutability-and-locks.md) | 状态 owner、`RefCell`、同步/异步锁、临界区与死锁证明 |
| [16 Atomic 与 Ordering](./16-atomic-ordering.md) | happens-before、Release/Acquire、CAS、Relaxed 门闩与 ABA |

### 工程实践与进阶

| 文档 | 要点 |
| --- | --- |
| [06 Cargo workspace](./06-cargo-workspace.md) | package/target/crate、feature/cfg、build script 与验证矩阵 |
| [07 Rust 测试](./07-testing.md) | 测试层级、替身、异步确定性、过滤器陷阱与覆盖证据 |
| [17 tracing](./17-tracing.md) | event/span/subscriber、结构化字段、`#[instrument]`、task 上下文与脱敏 |
| [18 宏与 RAII](./18-macros-and-raii.md) | 宏展开与 hygiene、derive/属性约束、`Drop`、取消恢复与显式 shutdown |
| [19 平台、unsafe 与性能](./19-platform-unsafe-performance.md) | unsafe 证明、FFI/fd 所有权、`cfg`、profile 与 benchmark 证据链 |

### 学习系统与综合实践

| 文档 | 要点 |
| --- | --- |
| [00 学习路线与能力自测](./00-learning-roadmap.md) | 分级自测、四阶段路线、每周节奏、卡住时的诊断方法 |
| [20 Rust 高密度语法解码](./20-syntax-decoder.md) | `Self`、`impl Trait`、HRTB、turbofish、模式、闭包、转换和常见组合类型 |
| [21 模块、可见性与 API 导航](./21-modules-and-api-navigation.md) | crate/module 边界、`mod`/`use`/`pub use`、源码跳转和依赖方向 |
| [22 仓库源码实验](./22-repository-reading-labs.md) | 由浅入深的真实源码任务、产出模板、自检答案和毕业标准 |
| [23 编译器错误诊断地图](./23-compiler-error-atlas.md) | 所有权、借用、类型推断、trait、dyn 与异步 `Send` 错误的系统排查方法 |
| [Rust 可运行实验](./labs/README.md) | 13 个标准库 Katas、6 个 compile-fail、30 个 Tokio async demos、六条 `study.sh` 路径、生产精读映射与学习检查表 |

## 每篇文章的使用方法

先读“什么时候使用”和最小示例，再打开项目锚点，回答：输入/输出是什么？谁拥有状态？错误或取消会到哪里？最后完成“阅读检查点”。遇到复杂生命周期或泛型时，先理解契约与边界，不要从类型细节开始逐字符推导。

不要把“看完”当作掌握。每篇至少做一次闭卷复述或源码实验：先遮住实现预测行为，再用类型、测试和调用方校验。能向别人解释一个设计的约束，并做出一个通过 focused test 的小改动，才算真正掌握。

本仓库使用 Rust 1.94、edition 2024。根 `Cargo.toml` 是自动生成的 workspace 根，日常改动应从目标 crate 的 manifest 和源码开始。
