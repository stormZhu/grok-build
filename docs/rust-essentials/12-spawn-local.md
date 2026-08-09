# 12. tokio::spawn_local 与单线程运行时

> `spawn_local` 解决 `!Send` 状态的调度约束；任务所有权、JoinHandle 与阻塞工作见 [8. async、任务与 `Send`](./08-async-runtime-tasks.md)。

## 12.1 spawn vs spawn_local

```rust
// spawn: 可以在多线程运行时中使用，future 必须实现 Send
tokio::spawn(async { /* ... */ });

// spawn_local: 只能在单线程运行时中使用，future 不需要 Send
tokio::task::spawn_local(async { /* ... */ });
```

## 12.2 为什么这个项目用 spawn_local

这个项目为每个会话创建**独立的单线程 tokio 运行时**（`current_thread`），因此：

- 每个会话有自己的事件循环，互不干扰
- 使用 `spawn_local` 可以避免 `Send` 约束，`Rc`、`RefCell` 等非 `Send` 类型也能在 async 代码中使用
- 性能更好，没有跨线程调度的开销

来自 [`build_session_runtime`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs#L44)：

```rust
// 源码节选：每个 session 使用 current_thread runtime，
// 所以 SessionActor 内的 RefCell/Rc 不需要满足 Send。
pub(crate) fn build_session_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
}
```

## 12.3 spawn_local 的实际使用

## 12.4 生命周期边界

`spawn_local` 仍要求任务在有效的 `LocalSet` 或 local runtime 上运行；从普通 `tokio::spawn` task 中随意调用会 panic。它也不让借用局部变量跨 task 存活：task 往往仍需要 `move` 捕获拥有值或 `Rc`/`Arc`。单线程只消除了跨线程共享，不消除重入、取消和跨 `.await` 的状态一致性问题。

```rust
// 源码节选：clone Arc 后 move 进 task；不借用当前 select! 分支的局部变量。
tokio::task::spawn_local({
    let session = session.clone();
    async move {
        // 后台 flush 失败只记录，主循环继续处理其他事件。
        if !session.run_memory_flush("interval", None).await {
            tracing::info!("MEMORY_IDLE_FLUSH: skipped");
        }
    }
});
```

这个片段出自 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L324)。`session.clone()` 是 `Arc` 引用计数增加，不会复制整个会话；`async move` 则让 task 拥有这个 `Arc`，满足 task 不借用当前事件循环栈帧的要求。
