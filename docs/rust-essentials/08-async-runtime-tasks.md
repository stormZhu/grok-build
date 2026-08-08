# 8. async、任务与 `Send`

`async fn` 不会立即运行：调用它只构造 Future，`.await` 才把控制权交给运行时直到结果就绪。它适合等待 I/O，不会自动让 CPU 密集计算并行。

```rust
let handle = tokio::spawn(async move {
    fetch().await
});
let value = handle.await??; // JoinError，再是 fetch 自身的 Result
```

## task 边界

- `tokio::spawn`：任务可能在线程池任一线程运行，捕获值和 future 通常需 `Send + 'static`。
- `tokio::task::spawn_local`：只在 `LocalSet` / 单线程运行时执行，允许 `Rc`、`RefCell` 等 `!Send` 状态。
- `JoinHandle`：丢弃不会自动取消任务；需要谁拥有任务生命周期、何时 await 或显式取消的规则。
- `.await` 可让其他 task 运行。不要在它前后依赖未同步的共享状态，也不要在 `.await` 间持有锁 guard。

## 项目中的锚点

- [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 运行单线程会话 Actor，并使用本地 task。
- [`search_bootstrap.rs`](../../crates/codegen/xai-grok-shell/src/session/storage/search_bootstrap.rs) 使用 `JoinSet` 管理一组并发任务。
- [`xai-tracing::tokio`](../../crates/common/xai-tracing/src/tokio.rs) 说明 task 创建时如何传播 tracing 上下文。

### 仓库代码摘录：同一个 Actor 的两种调度边界

[`LocalTerminalBackend`](../../crates/codegen/xai-grok-tools/src/computer/local/terminal.rs) 根据调用环境选择 task：

```rust
if use_spawn_local {
    tokio::task::spawn_local(actor_fut);
} else {
    tokio::spawn(actor_fut);
}
```

这不是可随意互换的写法：前者允许 LocalSet 中的 `!Send` 状态，后者使 Actor 可在线程池调度。阅读配置来源后再决定新增状态能否跨线程。

## 阻塞工作

同步文件、CPU 或阻塞库调用不应直接占用 Tokio worker。优先使用异步 API；确有必要时评估 `spawn_blocking`，并保留取消、并发上限和错误处理。不要把所有函数改成 async，只有会等待异步资源的调用链才需要它。

## 阅读检查点

为一个 `spawn` 的闭包列出所有 `move` 捕获值，并判断其中哪一个若替换为 `Rc` 会导致 `Send` 编译错误；再说明为何本项目的 `spawn_local` 不受该限制。
