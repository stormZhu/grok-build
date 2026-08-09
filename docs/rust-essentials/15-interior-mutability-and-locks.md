# 15. 内部可变性与锁选择

Rust 的普通借用规则要求可变访问持有 `&mut T`。内部可变性类型把运行时访问规则放进容器中，适合状态所有者无法取得 `&mut self` 的情况，但要匹配执行模型。

| 场景 | 首选 | 原因 |
| --- | --- | --- |
| 单线程 Actor 内的短暂可变状态 | `RefCell<T>` | 运行时借用检查；不可跨线程 |
| 多线程、无 `.await` 的短临界区 | `std::sync::Mutex<T>` / `RwLock<T>` | 锁 API 同步，guard 不应跨 await |
| 异步任务必须等待锁 | `tokio::sync::Mutex<T>` / `RwLock<T>` | lock 本身可 await；仍应缩短 guard 生命周期 |
| 简单独立计数/标志 | `Atomic*` | 无锁单值更新，不替代复合不变量 |

```rust
let snapshot = {
    let state = state.lock().await;
    state.snapshot()
}; // guard 在 await 前释放
send(snapshot).await;
```

## 项目中的锚点

- [`SessionMemory`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs#L11) 明确说明其 `RefCell` 只在 `LocalSet` 的 SessionActor 内使用。
- [`TerminalEntry`](../../crates/codegen/xai-grok-shell/src/terminal/streaming_local_terminal.rs#L142) 展示 `Arc<Mutex<...>>` 管理跨 task 的终端状态。
- [`SharedResources`](../../crates/codegen/xai-grok-tools/src/types/resources.rs#L181) 展示共享资源与 trait 对象包装。

### 仓库代码摘录：局部借用与原子状态分工

[`SessionMemory`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs#L34) 把两类状态明确分开：

```rust
// 源码节选：SessionActor 是 LocalSet 单线程，因此 RefCell 合法。
// 它保存需要从 &self 修改的局部内容，不能被跨线程 task 共享。
pub last_flush_content: RefCell<Option<String>>,

// AtomicBool 只表示一个独立的开关；它不保护 last_flush_content。
pub is_flushing: AtomicBool,
```

前者只在单线程 Actor 内借用，后者只表达独立的“是否正在 flush”标志。不要据此推断多个字段具有原子一致性；跨字段不变量仍应由 Actor 顺序或更高层同步维护。

### 项目关键代码：原子 compare-and-set 充当轻量门闩

[`SessionMemory::try_acquire_flush_lock`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs#L71) 用一个 AtomicBool 防止同一会话并发 flush：

```rust
pub(crate) fn try_acquire_flush_lock(&self) -> bool {
    self.is_flushing
        .compare_exchange(
            false, // 只有尚未 flush 时才能取得门闩。
            true,
            Ordering::Relaxed,
            Ordering::Relaxed,
        )
        .is_ok()
}

pub(crate) fn release_flush_lock(&self) {
    self.is_flushing.store(false, Ordering::Relaxed);
}
```

它只保护“是否已有 flush 在运行”这个单值不变量；实际 memory 内容仍由 SessionActor 的顺序控制，不能把这段代码当作通用互斥锁替代品。

## 必须避免

- 不要跨 `.await` 持有任何同步锁，可能阻塞运行时线程或造成死锁。
- 不要因“读多写少”机械改用 `RwLock`；先测量争用与临界区。
- 不要用多个 Atomic 拼装需要原子一致性的状态机；改用单个锁、channel 或明确的原子协议。

## 阅读检查点

看到 `Arc<Mutex<T>>` 时，确认锁 guard 的作用域没有跨 await，并写出这个锁保护的完整不变量，而不只是字段名。
