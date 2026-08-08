# 9. 通道、取消、背压与 Stream

Actor 的核心是“拥有状态的 task 接收命令”，通道决定消息的所有权、回复方式与背压策略。

| 工具 | 适用语义 | 项目例子 |
| --- | --- | --- |
| `mpsc` | 多个发送者、一位消费者的命令队列；有界容量提供背压 | [`LocalTerminalBackend`](../../crates/codegen/xai-grok-tools/src/computer/local/terminal.rs) |
| `oneshot` | 一次请求对应一次回复 | 同文件中的 `reply_tx` / `reply_rx` |
| `watch` | 只关心最新状态，慢接收者可跳过中间值 | [13 watch 通道](./13-watch-channel.md) |
| `broadcast` | 每个接收者都应看到事件；慢接收者要处理 lag | 适合订阅型事件，不适合命令 |

## 取消不是自动发生的

`CancellationToken` 是协作式取消信号：持有者调用 `cancel()`，执行中的 future 必须在等待点检查 `cancelled()` 或在 `select!` 中监听它。取消后仍要处理子任务、临时文件、回复通道关闭和部分完成状态。

```rust
tokio::select! {
    _ = cancel.cancelled() => return Err(Cancelled),
    result = operation() => result,
}
```

## Stream 与并发集合

Stream 是按时间产生多个值的异步序列。使用 `StreamExt::next()` 消费；`FuturesUnordered` / `JoinSet` 适合收集多个并行任务的完成顺序。并行不是免费：为网络、CPU 和外部服务设置容量，保留失败与取消的处理。

## 项目中的锚点

- [`terminal.rs`](../../crates/codegen/xai-grok-tools/src/computer/local/terminal.rs) 同时使用有界 `mpsc`、`oneshot` 和 `CancellationToken`。
- [`hooks.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session/hooks.rs) 使用 `FuturesUnordered` 消费并发 hook 的结果。
- [`leader/server.rs`](../../crates/codegen/xai-grok-shell/src/leader/server.rs) 有多处请求级取消 token，适合追踪取消的传递路径。

### 仓库代码摘录：命令与回复分成两条通道

终端后端创建有界命令队列和取消 token：

```rust
let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_CHANNEL_SIZE);
let cancel_token = CancellationToken::new();
```

每次 `run` 再创建专属 `oneshot`，将其随 `TerminalCommand::Run` 发送。这样队列只负责命令顺序，回复不会被别的请求取走；容量 `COMMAND_CHANNEL_SIZE` 则是背压策略的一部分。

## 阅读检查点

为一个 mpsc 命令定义写下：发送失败意味着什么？接收方退出后调用者如何获知？队列满时应等待、拒绝还是合并最新值？
