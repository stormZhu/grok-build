# 16. Atomic 类型与 Ordering

> Atomic 适合独立的计数或状态位。锁、`RefCell` 和多字段不变量的选择见 [15. 内部可变性与锁](./15-interior-mutability-and-locks.md)。

## 16.1 为什么需要 Atomic

在异步/多线程环境中，多个任务可能同时读写同一个变量。`Atomic*` 类型提供**无锁**的原子操作，比 `Mutex` 轻量得多。

## 16.2 常用类型

| 类型 | 用途 |
|------|------|
| `AtomicBool` | 标志位（如 `is_flushing`） |
| `AtomicUsize` | 计数器（如 `last_flush_len`） |
| `AtomicU64` | 大数值计数器 |

## 16.3 核心操作

```rust
use std::sync::atomic::{AtomicUsize, Ordering};

let counter = AtomicUsize::new(0);

// 读取
let val = counter.load(Ordering::Relaxed);

// 写入
counter.store(42, Ordering::Relaxed);

// 读取-修改-写入（原子地加 1）
counter.fetch_add(1, Ordering::AcqRel);
```

## 16.4 Ordering 选择指南

| Ordering | 含义 | 适用场景 |
|----------|------|----------|
| `Relaxed` | 只保证原子性，不保证顺序 | 简单计数器、标志位，不需要同步其他数据 |
| `Acquire` | 读操作，保证后续读写不会被重排到此之前 | 读取共享数据 |
| `Release` | 写操作，保证之前的读写不会被重排到此之后 | 发布共享数据 |
| `AcqRel` | 同时具有 Acquire 和 Release 语义 | `fetch_add` 等读-改-写操作 |
| `SeqCst` | 最强保证，全局顺序一致 | 需要最强保证但很少必要 |

## 16.5 项目中的实际使用

[`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L312) 的 idle flush 分支只用 Atomic 表示独立计数和开关：

```rust
// 源码节选：是否正在 flush 只是一个独立状态位，使用 Relaxed 即可。
!session.memory.is_flushing.load(Ordering::Relaxed)

// 读取当前消息数后，记录“本次已尝试处理到哪里”。
let last_len = session.last_idle_flush_conversation_len
    .load(Ordering::Relaxed);
if current_len > last_len {
    session.last_idle_flush_conversation_len
        .store(current_len, Ordering::Relaxed);
}
```

这些操作不借 Atomic 向其他线程发布复合数据，因此没有选择 `Acquire`/`Release`。真正的会话状态一致性仍由 Actor 的串行事件循环维持。

### 项目关键代码：计数与控制流保持分离

[`search_bootstrap.rs`](../../crates/codegen/xai-grok-shell/src/session/storage/search_bootstrap.rs#L501) 的退出标志使用更强的 `Acquire`，因为它与其他任务发布的工作归属有关：

```rust
// 源码节选：另一个执行者接管索引时，当前任务停止继续写入。
if claim_lost.load(Ordering::Acquire) {
    return;
}

// 相反，纯进度计数不承载其他数据的可见性，因此使用 Relaxed。
progress.skipped.fetch_add(1, Ordering::Relaxed);
```

读到 `Ordering` 时要问“这个原子操作是否仅记录数字，还是在观察另一个线程已发布的状态？”这两个问题决定了这里的差异。

## 16.6 不要用 Atomic 拼装状态机

`Relaxed` 只保证单个操作的原子性。若一个标志的可见性依赖另一段数据已初始化，或多个字段必须一起变化，单独的多个 Atomic 往往不足以表达不变量。此时优先用 Actor 消息、锁，或在有明确证明和测试时采用 release/acquire 协议。

修改 `Ordering` 前必须能写出数据竞争的双方、发布数据的操作和观察数据的操作；没有这份说明时，保持现有内存序并先寻求更高层同步方案。

```rust
// 读取标志位 — 只是检查状态，不需要同步其他数据，用 Relaxed
session.memory.is_flushing.load(Ordering::Relaxed)

// 记录计数器 — 单独的计数值，用 Relaxed
session.last_idle_flush_conversation_len.load(Ordering::Relaxed)
session.last_idle_flush_conversation_len.store(current_len, Ordering::Relaxed)

// 原子递增 — 读-改-写操作，用 AcqRel
some_counter.fetch_add(1, Ordering::AcqRel)
```

> **经验法则**：项目中大多数标志位和计数器用 `Relaxed` 就够了。只有当这个 atomic 的值**与其它共享数据的可见性相关**时，才需要更强的 Ordering。
