# 4. tokio::spawn_local 与单线程运行时

## 4.1 spawn vs spawn_local

```rust
// spawn: 可以在多线程运行时中使用，future 必须实现 Send
tokio::spawn(async { /* ... */ });

// spawn_local: 只能在单线程运行时中使用，future 不需要 Send
tokio::task::spawn_local(async { /* ... */ });
```

## 4.2 为什么这个项目用 spawn_local

这个项目为每个会话创建**独立的单线程 tokio 运行时**（`current_thread`），因此：

- 每个会话有自己的事件循环，互不干扰
- 使用 `spawn_local` 可以避免 `Send` 约束，`Rc`、`RefCell` 等非 `Send` 类型也能在 async 代码中使用
- 性能更好，没有跨线程调度的开销

来自 [spawn.rs](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs#L44-L48)：

```rust
pub(crate) fn build_session_runtime() -> std::io::Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()  // 单线程运行时
        .enable_all()
        .build()
}
```

## 4.3 spawn_local 的实际使用

```rust
// 在 select! 分支中 spawn 后台任务，不阻塞事件循环
tokio::task::spawn_local({
    let session = session.clone();
    async move {
        if !session.run_memory_flush("interval", None).await {
            tracing::info!("MEMORY_IDLE_FLUSH: skipped");
        }
    }
});
```