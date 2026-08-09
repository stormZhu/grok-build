# 16. Atomic、Ordering 与单值协议

Atomic 保证某个原子对象上的操作不会发生 data race；Ordering 决定它如何与其他内存操作建立顺序。它们适合计数、flag、generation 和经过证明的小状态机，不适合用多个字段临时拼出复杂事务。

锁与状态 owner 见 [15 内部可变性](./15-interior-mutability-and-locks.md)。

## 原子性不等于完整不变量

```rust
let requests = AtomicU64::new(0);
requests.fetch_add(1, Ordering::Relaxed);
```

每次加一不会丢失，这是单个 counter 的原子性。但若业务不变量是：

```text
status == Ready 时，config 与 cache 必须都已完成初始化
```

一个 atomic status 并不会自动保护另外两个字段。需要锁、Actor，或一个明确证明的 publish/observe 协议。

先写出 atomic 表示的状态和参与者，再选 ordering。

## 常用 Atomic 类型与操作

| 类型 | 常见用途 |
| --- | --- |
| `AtomicBool` | 门闩、enabled、shutdown observed |
| `AtomicUsize/U64` | counter、generation、索引 |
| `AtomicPtr<T>` | 底层 lock-free pointer，回收最困难 |
| `Atomic*` + enum 编码 | 小状态机，但必须处理非法值与转换 |

基本操作：

```rust
let value = counter.load(Ordering::Relaxed);
counter.store(10, Ordering::Relaxed);
let previous = counter.fetch_add(1, Ordering::Relaxed);
let result = flag.compare_exchange(
    false,
    true,
    Ordering::Acquire,
    Ordering::Relaxed,
);
```

`fetch_add` 是原子 read-modify-write；选择 AcqRel 与否取决于同步语义，不是因为“读改写天然需要更强 ordering”。

## 从编译器顺序到 happens-before

多核 CPU 和编译器可在不改变单线程可观察结果的前提下重排内存操作。跨线程看到另一个 atomic 值，并不默认意味着同时看见对方此前写入的其他数据。

关键关系是：

```text
线程 A：写普通数据 -> Release 写 atomic
                              |
                         synchronizes-with
                              |
线程 B：Acquire 读到该值 -> 读取普通数据
```

当 B 的 Acquire 实际观察到 A 的 Release（或相应 release sequence）时，A 在 Release 前的操作 happens-before B 在 Acquire 后的操作。

Acquire/Release 不是给任意普通 data race 发许可证。被发布数据仍必须有合法的 ownership/aliasing 设计，例如发布后只读，或本身受其他同步保护。

## 五种 Ordering

| Ordering | 提供的主要约束 | 典型意图 |
| --- | --- | --- |
| `Relaxed` | 该 atomic 本身的原子性与 modification order | 独立 counter、无需发布其他数据的 generation/flag |
| `Release` | 此操作前的读写不能跨到其后，用于发布 | writer 宣布某状态/数据已准备好 |
| `Acquire` | 此操作后的读写不能跨到其前，用于观察 | reader 观察到对应 Release 后使用已发布数据 |
| `AcqRel` | 对同一 read-modify-write 同时 Acquire + Release | 既观察旧状态又发布新状态的转换 |
| `SeqCst` | Acquire/Release 之外还有所有 SeqCst 操作的单一全序 | 需要全局顺序推理的小协议 |

`SeqCst` 更容易形成直观全序，但不会修复错误的 ownership、多个原子间缺失的事务或 use-after-free。先证明协议，再决定是否可以用较弱序；不要从 Relaxed 逐个“升级直到测试不 flaky”，调度测试不能可靠证明内存模型。

## Relaxed counter 为什么正确

若 counter 只用于 telemetry：

```rust
processed.fetch_add(1, Ordering::Relaxed);
```

需求只有：

- 每次增量不丢。
- 最终读取获得该 atomic modification order 中的某个值。
- 不借此 counter 发布某个 request payload。

那么 Relaxed 已满足。并发读取不保证看到“实时最新”的墙钟意义，但任何 ordering 也不提供分布式瞬时 snapshot。

多个 Relaxed counter 分别正确，不代表一次读取能得到彼此一致的同一时刻快照。若 `total == ok + error` 必须始终成立，使用一个锁/单值编码或容忍采集时短暂不一致。

## 仓库的 flush 门闩

[`SessionMemory::try_acquire_flush_lock`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 使用：

```rust
self.is_flushing
    .compare_exchange(
        false,
        true,
        Ordering::Relaxed,
        Ordering::Relaxed,
    )
    .is_ok()
```

它表达单值协议：

```text
false --成功 CAS--> true：本调用取得执行资格
true  --CAS 失败----> true：已有 flush，当前调用跳过
```

实际 memory 内容不是通过这个 flag 发布给另一线程，session 的主要一致性由 LocalSet actor 流程维护，因此这里选择 Relaxed。名字叫 “lock” 不代表它具有 Mutex 的数据发布语义。

release 也用 `store(false, Relaxed)`。若未来另一个线程在观察 false 后直接读取 flush 产生的非原子数据，原有证明将失效，必须重构同步边界而不是只凭名字保留 Relaxed。

## compare-exchange 的两个 Ordering

`compare_exchange(current, new, success, failure)` 有成功与失败两条路径：

- success ordering 同时约束读取旧值和写入新值。
- failure 只发生读取，不能是 `Release` 或 `AcqRel`。
- failure 常用 `Relaxed`，除非失败时读到的值也用于观察某个发布协议。
- weak 版本可 spuriously fail，通常放在循环中；strong 版本不因该原因失败。

```rust
let mut current = state.load(Ordering::Relaxed);
loop {
    let next = transition(current)?;
    match state.compare_exchange_weak(
        current,
        next,
        Ordering::AcqRel,
        Ordering::Acquire,
    ) {
        Ok(previous) => break previous,
        Err(observed) => current = observed,
    }
}
```

这是形状示例，不意味着所有状态机都需要 AcqRel/Acquire。每个 ordering 必须对应“观察谁发布的哪些数据”这句话。

## generation 与 notification

generation 常用 `fetch_add(1, Relaxed)` 表示“发生过变化”，但要区分：

- atomic generation 自身就是全部状态，receiver 只比较数字。
- generation 发布另一个普通对象的新 snapshot。
- generation 只用于 cache invalidation，真实对象由锁/ArcSwap 管理。

第一种通常 Relaxed 足够；后两种需要依赖对象自己的同步或成对 release/acquire。单独让 generation 变强不能弥补无同步访问普通可变数据。

仓库 model switch 使用 `watch<u64>` 而非 Atomic generation，channel 自己承担通知与同步；见 [13 watch](./13-watch-channel.md)。

## Acquire load 的仓库语境

[`search_bootstrap.rs`](../../crates/codegen/xai-grok-shell/src/session/storage/search_bootstrap.rs) 的 `claim_lost` 用 Acquire 读取，因为它参与跨执行者的工作归属/停止协议；同一区域的纯进度 `skipped.fetch_add(1, Relaxed)` 不发布其他状态。

不能只从这一行推导 Acquire 正确性。完整审查必须找到所有 store/CAS writer，回答：

1. 哪个操作发布 claim lost。
2. writer 在 Release 前完成了什么。
3. reader 读到哪个值后依赖哪些数据。
4. 没读到新值时继续工作是否安全。
5. state 回绕或复用会不会产生 ABA。

Ordering 是协议两端的关系，不是单行属性。

## Atomic 状态机与 ABA

用整数编码 enum：

```rust
const IDLE: u8 = 0;
const RUNNING: u8 = 1;
const CLOSED: u8 = 2;
```

需要列出所有合法 transition 和失败行为。多个 Atomic 分别 CAS 无法形成跨字段事务。

ABA 问题：

```text
reader 看到 A
其他线程 A -> B -> A
reader CAS(A -> C) 成功，却不知道中间发生过 B
```

若中间变化有意义，可加入单调 generation/tag，或使用不会复用 identity 的 ownership/reclamation 方案。pointer CAS 还涉及对象何时可释放；Arc raw pointer、hazard pointer、epoch reclamation 都不能凭直觉手写。

## Atomic、锁还是 Actor

| 需求 | 优先考虑 |
| --- | --- |
| 独立计数/布尔门闩 | Atomic |
| 多字段必须一致变化 | Mutex 或 Actor |
| 操作需等待 I/O 且状态 owner 单一 | Actor command |
| 读多写少且需一致 snapshot | RwLock/ArcSwap，按语义与测量 |
| lock-free 数据结构 | 先用成熟 crate；需要严格 proof 和回收模型 |

Atomic 可能减少 lock 开销，也可能因 cache-line bouncing 在高争用下更慢。`SeqCst` vs Relaxed 的性能差异依平台/操作而异；正确性证明优先，性能用 benchmark/profile。

## false sharing 与缓存行

两个逻辑无关的 hot atomic 若落在同一 cache line，不同核心更新时会互相使 cache 失效，称 false sharing。症状是 CPU/吞吐恶化而锁等待不明显。

不要未测量就给每个 counter padding；这会增加内存和 cache footprint。用 profile/hardware counters/代表性 benchmark 确认，再考虑分片计数、批量聚合或 cache padding 类型。

## 测试不能证明弱内存序正确

单元测试即使重复百万次也可能从未触发目标架构的重排。验证层次：

- 先写纸面/注释中的 happens-before proof。
- 小并发模型可用 Loom（若项目采用）探索 interleaving。
- Miri 检查部分 UB，不是硬件内存序证明器。
- ThreadSanitizer 可发现 data race，但不证明高层协议。
- 在支持的目标架构运行压力/集成测试，作为补充。
- 保留状态转换的纯函数/表驱动测试。

没有 proof 时优先回到锁、channel 或 Actor，降低推理成本。

## 修改 Ordering 的审查清单

1. atomic 表示的完整状态是什么？
2. 所有 reader/writer 在哪里？
3. 仅需 atomicity，还是发布/观察其他数据？
4. Release 与哪个 Acquire 配对，后者保证看见什么？
5. CAS success/failure order 分别为何足够？
6. 多个字段是否被误当成同一 snapshot？
7. 是否有回绕、ABA、对象回收或 shutdown 竞态？
8. 当前 actor/lock/channel 是否已经提供同步，使 atomic 只需 Relaxed？
9. 测试覆盖状态转换，但 proof 是否独立存在？
10. 性能变化是否有实际证据？

## 阅读练习

1. 在 [`memory_state.rs`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 将 Atomic 分成 telemetry counter、门闩和状态 flag，分别写出为何不发布其他数据。
2. 跟踪 [`search_bootstrap.rs`](../../crates/codegen/xai-grok-shell/src/session/storage/search_bootstrap.rs) 的 `claim_lost` 所有 writer；仅看 Acquire reader 不算完成。
3. 找一个 `compare_exchange`，画出成功、失败、重试和取消路径，解释两个 ordering。
4. 找两个需要一致读取的数值字段，说明为何分别 Atomic 不能给出事务 snapshot。
5. 将一个 Atomic 方案改写成 actor message 的纸面设计，比较吞吐、错误处理和证明复杂度。

完成标准：选择 Ordering 时能说出具体 publish/observe 或“只需原子性”的证明，并能识别多字段事务、ABA 和回收问题何时应回到更高层同步。
