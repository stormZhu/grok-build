# 6. Arc / Mutex / RwLock 共享状态

> 本文是共享所有权的速查；锁选择、`RefCell` 与跨 `.await` 约束见 [15. 内部可变性与锁](./15-interior-mutability-and-locks.md)。

## 6.1 基本模式

在异步代码中，多个任务需要共享同一份数据：

```rust
use std::sync::{Arc, Mutex};

// Arc: 原子引用计数，允许多个所有者
// Mutex: 互斥锁，保证同时只有一个任务访问
let shared = Arc::new(Mutex::new(MyState::new()));

// 每个任务 clone 一个 Arc（增加引用计数，不复制数据）
let shared_clone = shared.clone();
tokio::spawn(async move {
    let mut state = shared_clone.lock().unwrap();
    state.update();
});
```

这里的同步 `Mutex` guard 只用于同步代码。**绝不要在 guard 仍存活时 `.await`**；异步锁或快照的正确用法见 15。

## 6.2 clone 模式

项目中最常见的模式：

```rust
// 将 session 的 Arc clone 一份传入 async 块
let session = session.clone();
tokio::task::spawn_local(async move {
    // 在 async 块中使用 session
    session.do_something().await;
});
```

## 6.3 RwLock 的使用场景

```rust
use std::sync::RwLock;

// RwLock: 多个读者可以同时读，但写者独占
// 适合读多写少的场景
let cache = Arc::new(RwLock::new(HashMap::new()));

// 读
let data = cache.read().unwrap().get(&key).cloned();

// 写
cache.write().unwrap().insert(key, value);
```

## 6.4 项目中的执行模型

`SessionActor` 在单线程 `LocalSet` 中运行时，可以用 `RefCell` 保存不跨线程的局部状态；跨 task 或跨线程共享才需要 `Arc` 加锁。看到 `Arc<dyn Trait>` 时，它解决的是共享所有权和可替换实现，trait 是否线程安全由 `Send + Sync` 边界决定。
