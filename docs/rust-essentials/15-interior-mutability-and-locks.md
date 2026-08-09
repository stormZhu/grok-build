# 15. 内部可变性、锁与状态所有权

Rust 的普通可变访问由 `&mut T` 在编译期保证唯一。`Cell`、`RefCell`、锁和 atomic 把不同部分的规则移到运行期；选型依据不是“哪个更快”，而是谁拥有状态、访问是否跨线程、临界区是否跨 `.await`，以及要保护什么不变量。

共享所有权见 [14 `Arc` / `Mutex` / `RwLock`](./14-arc-mutex-rwlock.md)，原子协议见 [16 Atomic 与 Ordering](./16-atomic-ordering.md)。

## 所有权与可变性是两个维度

`Rc<T>`/`Arc<T>` 只回答“有几个 owner”，不自动允许修改；`RefCell<T>`/`Mutex<T>` 才定义访问规则：

| 形状 | owner | 可变访问规则 |
| --- | --- | --- |
| `T` / `&mut T` | 单一或借用 | 编译期唯一可变借用 |
| `Rc<RefCell<T>>` | 单线程多个 owner | 运行时 borrow 检查 |
| `Arc<std::sync::Mutex<T>>` | 多线程多个 owner | 阻塞线程取得互斥 guard |
| `Arc<tokio::sync::Mutex<T>>` | 多 task/线程 owner | 异步等待互斥 guard |
| Actor + channel | 状态只有 actor owner | 消息串行修改，调用者不直接借用 |

看到 `Arc<Mutex<T>>` 不要只翻译成“线程安全共享变量”，而要找出：谁 clone Arc、谁 lock、锁保护的完整不变量、guard 在哪里 drop。

## `Cell` 与 `RefCell`

`Cell<T>` 通过 copy/replace 整体值修改，不返回内部引用，适合单线程的小型 `Copy` 标志或计数：

```rust
let mode = Cell::new(Mode::Idle);
mode.set(Mode::Busy);
let current = mode.get();
```

`RefCell<T>` 提供 `borrow()` 和 `borrow_mut()`。规则与普通借用相同，但在运行期检查：

```rust
let state = RefCell::new(vec![1]);

let read = state.borrow();
let second_read = state.borrow(); // 多个共享 borrow 合法
drop((read, second_read));

let mut write = state.borrow_mut(); // 唯一可变 borrow
write.push(2);
```

重叠的 `borrow_mut` 或“共享 borrow 尚存时可变 borrow”会 panic，不是返回编译错误。边界输入或重入可能冲突时可用 `try_borrow`/`try_borrow_mut` 返回 `Result`，但这不应掩盖错误的状态设计。

`RefCell` 不是线程同步原语，不实现 `Sync`。它适合本仓库固定在 LocalSet 的 session actor，见 [12 `spawn_local`](./12-spawn-local.md)。

## 借用 guard 的真实生命周期

Non-lexical lifetimes 常能在最后一次使用后缩短普通引用，但 `Ref`、`MutexGuard` 等拥有 Drop 的 guard 最清楚的写法仍是显式小作用域：

```rust
let storage = {
    let borrowed = self.storage.borrow();
    borrowed.clone()
}; // Ref 在这里 drop

storage.write(data).await?;
```

仓库 [`SessionMemory::storage`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 正是这个模式：

```rust
pub(crate) fn storage(&self) -> Option<MemoryStorage> {
    self.storage.borrow().clone()
}
```

返回 owned clone 后，调用者可跨 await 使用它，而不会把 `RefCell` borrow 留在 actor 上。读到“clone before await”时先判断 clone 的是 Arc/handle 还是整份大数据，再评估成本。

## 三类常见锁

### `std::sync::Mutex` / `RwLock`

取得锁时阻塞当前 OS thread。适合无 await、临界区很短、竞争低的同步数据。标准库锁会 poison：持锁线程 panic 后，后续 `lock()` 返回 `PoisonError`；调用者可传播、恢复 inner，或按不变量 fail closed。

`std::sync::RwLock` 允许多个 reader，但公平性和 writer starvation 细节依赖平台。读多不自动等于更快；缓存行、锁管理和更长读 guard 都可能抵消收益。

### `parking_lot` 锁

API 同步、体积与性能特征不同，且通常不 poison。仍会阻塞线程，仍不能把“没有 PoisonError”理解成 panic 后状态一定有效。仓库已有使用时遵循局部惯例，不为去掉一个 unwrap 随意替换锁实现。

### `tokio::sync::Mutex` / `RwLock`

`lock().await` 不阻塞 runtime thread，等待者 task 会被挂起。适合临界区本身确实需要跨 async 操作，或竞争时不能阻塞执行器。

异步锁更贵，也不意味着 guard 可以无限跨 await。跨 await 会扩大临界区，可能造成 convoy、循环等待和 shutdown 卡住。

## 锁能否跨 `.await`

“同步锁 guard 不跨 await”应作为强默认：

- 它可能让 future 失去 `Send`。
- await 期间 task 停止，但 OS thread 上其他工作仍可能需要同一锁。
- 在 current-thread runtime 上尤其容易卡住所有 task。
- 取消发生时虽会 drop guard，但外部操作可能只完成了一半。

Tokio 锁在类型上允许跨 await，且有时这是保持事务不变量的正确做法：

```rust
let mut connection = connection.lock().await;
connection.write_all(frame).await?;
connection.flush().await?;
```

若同一连接的 frame 必须不可交错，整个 async write 可能就是临界区。此时要明确最大等待时间、调用图是否会重入同一锁、取消后连接状态是否仍合法。

若 await 不需要受保护状态，先取 snapshot：

```rust
let request = {
    let state = state.lock().await;
    state.build_request()
};

let response = client.send(request).await;

{
    let mut state = state.lock().await;
    state.apply(response)?;
}
```

但释放再重取会引入 TOCTOU：response 返回时 state 可能已变化。需要 generation/version 检查、显式状态机，或把完整操作留给单一 actor。

## 仓库中的 LocalSet 状态

[`SessionMemory`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 把不同语义拆开：

```rust
pub storage: RefCell<Option<MemoryStorage>>,
pub last_flush_content: RefCell<Option<String>>,
pub is_flushing: AtomicBool,
pub flush_count: AtomicU64,
```

- `storage` 和 `last_flush_content` 属于 session thread，可从 `&self` 在短作用域内修改。
- `is_flushing` 是可被并发观察/竞争取得的单值门闩。
- `flush_count` 是独立 telemetry 计数。
- 这些字段并不共同组成一个自动原子的 snapshot；跨字段不变量仍由 session actor 流程维护。

`try_acquire_flush_lock` 使用 compare-exchange：

```rust
self.is_flushing
    .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
    .is_ok()
```

它只保证一个调用者把 false 改成 true。它不是通用 mutex，也不保护 `last_flush_content`。如果未来让真正的多线程数据通过这个 flag 发布，Relaxed 是否足够必须重新证明。

## 仓库中的异步 registry 锁

[`streaming_local_terminal.rs`](../../crates/codegen/xai-grok-shell/src/terminal/streaming_local_terminal.rs) 的全局 registry 保存 `Arc<TerminalEntry>`：

```rust
async fn get_entry(session_id: &str, terminal_id: &str) -> Option<Arc<TerminalEntry>> {
    let key = (session_id.to_string(), terminal_id.to_string());
    registry().lock().await.get(&key).cloned()
}
```

临时 guard 在表达式结束后释放，只把 entry 的 Arc clone 返回。随后对终端做 I/O 不继续占用全局 map 锁，其他 session 仍可注册或查询。

同一文件的批量清理则需要在一个临界区内选择并移除一组 entry，避免“先列 ID、再逐个删除”期间有新终端插入。这里说明临界区大小由不变量决定，不能机械追求每次只锁一行。

## 死锁与锁顺序

Rust 防止 data race，不防止逻辑死锁。常见环：

```text
task A：持有 sessions，等待 terminal
task B：持有 terminal，等待 sessions
```

维护多把锁时：

1. 写出全局获取顺序，例如 `registry -> entry`。
2. 不在持锁时调用未知回调、trait 方法或发送可能同步回入的事件。
3. 不把 guard 隐藏在长表达式或 iterator 中。
4. 对“锁内 await”审查被 await 的完整调用图。
5. shutdown 路径也遵循相同顺序。
6. timeout 可防止永久挂住，但不能修复被破坏的不变量。

若锁关系持续复杂，考虑让一个 actor 拥有状态，通过 command + oneshot reply 序列化操作。

## `RwLock`、分片和 ArcSwap 不是默认升级

读多写少时可能考虑 `RwLock`；key 之间独立且争用可测时可考虑 sharded map；读路径需要 lock-free snapshot 时可考虑 `ArcSwap`。这些结构都会增加一致性模型：

- 多个 shard 之间没有原子 snapshot。
- ArcSwap reader 可能继续看到旧 Arc。
- RwLock 的 upgrade 不是普通 read guard 原地变 write。
- lock-free 不等于 wait-free，也不等于总体更快。

先用 profile/metrics 证明锁是瓶颈，再改变同步模型；见 [19 性能边界](./19-platform-unsafe-performance.md)。

## 取消、panic 与 RAII

guard drop 会释放锁，所以普通 panic unwind 或 future 取消通常不会永久占锁；但业务状态可能已做一半修改。设计临界区时选择：

- 先在局部构造完整新值，再一次替换。
- 用状态 enum 显式表示 `Updating`/`Failed`。
- 对外部副作用使用幂等操作或恢复日志。
- 关键清理用 RAII guard，异步收尾另有显式 shutdown。

RAII 详见 [18 宏与 RAII](./18-macros-and-raii.md)。

## 测试与诊断

锁相关测试不要依赖 sleep 构造顺序。使用 barrier/oneshot 让 task 停在“已持锁”“已释放”“等待外部结果”等阶段，并断言：

- 第二个操作应等待还是并行。
- 取消 waiter/holder 后 guard 是否释放。
- panic/错误后状态是否仍满足不变量。
- generation 变化时旧 response 是否被拒绝。
- shutdown 是否能在 deadline 内完成。

遇到挂住时记录 task 正在等待哪把锁、当前 holder 在 await 什么；只看最后一条业务日志通常不够。

## 阅读练习

1. 运行 [`mutex_snapshot`](./labs/async-demos/src/bin/mutex_snapshot.rs)，用 oneshot 同步点证明 reader 等待期间已经释放 guard，并区分 snapshot 与当前共享状态。
2. 在 [`SessionMemory`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 给每个 `RefCell`/Atomic 字段标出访问线程和不变量，判断是否存在必须一起读取的字段。
3. 跟踪 [`streaming_local_terminal.rs`](../../crates/codegen/xai-grok-shell/src/terminal/streaming_local_terminal.rs) 的 registry 到 `kill_and_release_all_for_session`，解释哪些工作必须在 map lock 内、哪些应在释放后执行。
4. 找一个 `lock().await` 后还有另一个 `.await` 的调用点，画出可能等待它的 task；判断跨 await 是协议要求还是可缩短。
5. 找一个 `RefCell::borrow_mut`，确认借用是否可能跨 callback 或 await，并写出发生重入时的失败方式。

完成标准：面对共享状态时，能先写出 owner、线程/task 边界和不变量，再选择 RefCell、同步锁、异步锁、Atomic 或 Actor，并解释 guard 的精确释放点。
