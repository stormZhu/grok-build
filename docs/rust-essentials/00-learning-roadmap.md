# 0. 学习路线与能力自测

目标不是记住 Rust 的全部语法，而是逐步获得四种能力：看懂类型、追踪数据、解释并发、可靠修改。对 `grok-build` 来说，“掌握 Rust”的可观察标准是：拿到一个陌生模块后，能在 30 分钟内说清公开入口、状态所有者、错误/取消路径和最小验证命令。

## 先做 15 分钟自测

不要查资料，给下面每项打分：`0` 表示不会，`1` 表示看提示能答，`2` 表示能结合仓库源码解释。

| 问题 | 对应材料 |
| --- | --- |
| `String`、`&str`、`PathBuf`、`&Path` 分别通常用于什么边界？ | [01 所有权与借用](./01-ownership-and-borrowing.md) |
| 为什么 `match value` 可能移动字段，而 `match &value` 不会？ | [02 类型与模式](./02-type-modeling-and-patterns.md) |
| `Tool` 为什么有 `type Args`，`ToolDispatch` 为什么改用 JSON 值？ | [03 trait](./03-traits-generics-and-dyn.md)、[20 语法解码](./20-syntax-decoder.md) |
| `?` 会把错误送到哪里，`From` 在其中做了什么？ | [04 错误](./04-errors.md) |
| 给线协议 enum 新增 variant 可能破坏什么？ | [05 Serde](./05-serde-and-wire-compat.md) |
| `cargo check -p xai-tool-runtime` 实际检查哪些代码？ | [06 Cargo](./06-cargo-workspace.md) |
| `tokio::spawn` 为什么常要求 `Send + 'static`？ | [08 async](./08-async-runtime-tasks.md) |
| `mpsc`、`oneshot`、`watch` 分别表达什么通信语义？ | [09 通道](./09-channels-cancellation-streams.md) |
| `select!` 丢弃未完成分支时，哪些操作可能不具备取消安全性？ | [10 select](./10-tokio-select.md) |
| `spawn_local` 为什么仍要求 `'static`，由谁保证 task 不跨线程？ | [12 spawn_local](./12-spawn-local.md) |
| `watch` 重复发送相等值会不会通知，慢 receiver 会不会看见每个中间值？ | [13 watch](./13-watch-channel.md) |
| 为什么 `MutexGuard` 通常不应跨 `.await`？ | [14 共享状态](./14-arc-mutex-rwlock.md) |
| 一个 `AtomicBool` 使用 `Relaxed` 前需要证明什么？ | [16 Atomic](./16-atomic-ordering.md) |
| unsafe block 的有效性、所有权、线程和错误不变量分别是什么？ | [19 unsafe](./19-platform-unsafe-performance.md) |
| `pub use` 与普通 `use` 对调用者有何不同？ | [21 API 导航](./21-modules-and-api-navigation.md) |
| 看到 `Pin<Box<dyn Stream<Item = T> + Send>>`，能从外向内解释吗？ | [20 语法解码](./20-syntax-decoder.md) |

总分不是考试成绩，只用来选入口：

- `0--11`：按 01 → 09 顺序读，暂时跳过 10--19 的细节。
- `12--23`：按薄弱项查阅，然后开始 [源码实验](./22-repository-reading-labs.md) 的 1--4 关。
- `24--32`：直接做 5--8 关；不会解释的地方再反查专题。

## 四阶段路线

### 阶段 A：读懂单个函数

完成 01--05。每读一个函数，只写五行笔记：

```text
输入：拥有值还是借用值？
输出：值、Option、Result 还是 Future？
状态：哪些局部变量会变化？
分支：哪些 enum variant / 错误会提前返回？
副作用：文件、网络、日志、channel 中的哪一种？
```

不要一开始推导所有生命周期。先把 `'a` 读作“这些引用之间存在同一段有效期约束”，再从函数返回值反推它约束了谁。

### 阶段 B：读懂 crate 的公共契约

完成 06--07、20--21。选择一个较小 crate，例如 [`xai-tool-runtime`](../../crates/common/xai-tool-runtime/src/lib.rs)：

1. 从 `Cargo.toml` 看它依赖谁。
2. 从 `src/lib.rs` 看公开模块和 re-export。
3. 只读公开 trait、struct、enum 和类型别名，暂时跳过实现。
4. 找一个测试，观察调用者真正依赖哪些行为。
5. 用一句话描述它在依赖图中的责任。

这一阶段的目标是学会把实现细节和稳定契约分开。

### 阶段 C：读懂异步调用链

完成 08--17。从 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 向 [`SessionCommand`](../../crates/codegen/xai-grok-shell/src/session/commands.rs) 追，再到 [`run_session`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)。沿途为每个边界标注：

```text
调用者 --mpsc::send--> Actor
调用者 <--oneshot----- 单次结果
Actor   --spawn_local-> 后台任务
Actor   --select!-----> 当前最先就绪的事件
```

异步代码不要只沿函数调用读。任务、channel 和取消令牌同样是控制流边。

### 阶段 D：做出可靠改动

完成 18--19 和源码实验。改动前写下不变量，改动后用最小范围验证：

```text
行为不变量：Terminal 必须恰好出现一次。
类型不变量：线协议仍能反序列化旧数据。
并发不变量：跨 await 的 guard 有明确必要性与等待图，关闭 channel 能退出任务。
验证：cargo test -p <crate> <focused-test>
```

先做纯函数、错误消息、测试和小型类型变更，再做 Actor 事件循环、协议或 `unsafe`。

## 每次学习的 45 分钟循环

| 时间 | 动作 | 产出 |
| --- | --- | --- |
| 5 分钟 | 选一个问题，不选“读完整个文件” | 一句具体问题 |
| 10 分钟 | 只看签名、类型和调用方，预测实现 | 一张数据流草图 |
| 15 分钟 | 阅读实现并用专题文档解码 | 3--5 条带证据结论 |
| 10 分钟 | 看测试或运行 focused test | 验证结果 |
| 5 分钟 | 闭卷复述，并记录仍不确定之处 | 一段自己的解释 |

推荐问题示例：“`ToolDispatch::call_terminal` 如何保证只返回终态？”比“学习 trait”更有效。

## 编译器是练习伙伴

对源码做实验时，优先在现有测试中添加最小用例；纯语法可以在临时文件中验证：

```sh
rustc --edition 2024 /tmp/rust_scratch.rs
```

本目录还提供不依赖 Cargo workspace 的 [Rust 阅读 Katas](./labs/README.md)：正常案例用于预测所有权和运行结果，compile-fail 案例覆盖 move、借用冲突、局部引用、临时值与 `Rc: !Send`。完成 01--04 时至少运行前 10 关，进入异步并发前再完成线程和 channel 关。

常用的只读导航命令：

```sh
# 找定义和实现
rg -n '^(pub )?(struct|enum|trait) Tool|^impl.*Tool' crates

# 找调用点；先限定 crate，结果太少再扩大
rg -n 'call_terminal\(' crates/common/xai-tool-runtime crates/codegen

# 看包名、target 和 feature，不编译
cargo metadata --no-deps --format-version 1

# 让编译器解释诊断码
rustc --explain E0382
```

不要用 `cargo expand`、`cargo tree` 等命令作为理所当然的前置条件；它们可能未安装。先用 rust-analyzer、`rg` 和 Cargo 自带命令，确有需要再引入工具。

## 卡住时按症状诊断

| 症状 | 不要继续硬读 | 下一步 |
| --- | --- | --- |
| 不知道符号在表达什么 | 猜整行含义 | 查 [20 语法解码](./20-syntax-decoder.md)，从类型最外层向内拆 |
| 不知道名字来自哪里 | 顺着文件从头读 | 查 `use`、`pub use`、`mod`，再看 [21 API 导航](./21-modules-and-api-navigation.md) |
| 所有权报错 | 到处加 `.clone()` | 画出所有者和借用区间，查 [错误诊断地图](./23-compiler-error-atlas.md) 的 E0382/E0502 |
| Future 不是 `Send` | 给所有类型加 `Arc<Mutex<_>>` | 找跨 `.await` 存活的引用或 guard，再查 [错误诊断地图](./23-compiler-error-atlas.md) |
| Actor 代码分支太多 | 逐行记忆 | 先列输入 channel、输出 channel、状态与退出条件 |
| 宏看不懂 | 立即研究展开后的全部 token | 先看宏调用生成了哪类 item，再找测试和实现目标 |
| 修改后不知测什么 | 跑整个 workspace | 从目标 crate 的现有测试和直接调用者开始 |

## 阶段验收

达到下面标准，才进入下一阶段：

- 能不查资料解释 `Option<Result<T, E>>` 与 `Result<Option<T>, E>` 的行为差异。
- 能从 `lib.rs` 找到一个 re-export 的真实定义。
- 能画出一次 `mpsc + oneshot` 请求/回复的所有权转移。
- 能指出一个 `.await` 前后仍然存活的值。
- 能为改动选择 focused test，并解释它覆盖和没有覆盖什么。
- 能完成 [源码实验](./22-repository-reading-labs.md) 中至少一项“改码关”。

最后一项最重要：Rust 是通过预测、编译、解释错误和修正模型掌握的，不是通过累计阅读页数掌握的。
