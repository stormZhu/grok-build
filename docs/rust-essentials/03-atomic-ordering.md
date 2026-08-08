# 3. Atomic 类型与 Ordering

## 3.1 为什么需要 Atomic

在异步/多线程环境中，多个任务可能同时读写同一个变量。`Atomic*` 类型提供**无锁**的原子操作，比 `Mutex` 轻量得多。

## 3.2 常用类型

| 类型 | 用途 |
|------|------|
| `AtomicBool` | 标志位（如 `is_flushing`） |
| `AtomicUsize` | 计数器（如 `last_flush_len`） |
| `AtomicU64` | 大数值计数器 |

## 3.3 核心操作

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

## 3.4 Ordering 选择指南

| Ordering | 含义 | 适用场景 |
|----------|------|----------|
| `Relaxed` | 只保证原子性，不保证顺序 | 简单计数器、标志位，不需要同步其他数据 |
| `Acquire` | 读操作，保证后续读写不会被重排到此之前 | 读取共享数据 |
| `Release` | 写操作，保证之前的读写不会被重排到此之后 | 发布共享数据 |
| `AcqRel` | 同时具有 Acquire 和 Release 语义 | `fetch_add` 等读-改-写操作 |
| `SeqCst` | 最强保证，全局顺序一致 | 需要最强保证但很少必要 |

## 3.5 项目中的实际使用

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