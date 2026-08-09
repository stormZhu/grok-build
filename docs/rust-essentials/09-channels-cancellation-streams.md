# 9. channel、取消、背压与 Stream

Channel 不只是“task 间传数据”：它编码谁拥有消息、能有几个发送者/接收者、队列满时怎么办、关闭如何传播。取消也不是一个 bool；它是从发出信号、停止接收新工作、让运行中 Future 收尾，到确认 task 已结束的一套协议。

## 先按语义选择 channel

| 工具 | 消息保留 | 接收者 | 容量/慢消费者 | 典型用途 |
| --- | --- | --- | --- | --- |
| `mpsc::channel(n)` | 每条消息由唯一 receiver 收一次 | 1 | 有界；`send().await` 形成背压 | Actor 命令、工作队列 |
| `mpsc::unbounded_channel()` | 同上 | 1 | 无界；发送不等待，可能增长内存 | 保证生产速率受其他机制约束的内部事件 |
| `oneshot::channel()` | 最多一个值 | 1 | 无队列 | 一次请求的专属回复/ack |
| `watch::channel(initial)` | 只保留最新值 | 多个 receiver clone | 慢接收者跳过中间值 | 配置、状态、shutdown flag |
| `broadcast::channel(n)` | 环形保留最近 n 条 | 每个 receiver 各看一份 | 慢接收者收到 Lagged | 订阅事件 |

类型选择前先写一句业务语义：“每条命令必须处理一次”“只需最终状态”“每位订阅者都应看到事件”。若说不清，就还不能选 channel。

## mpsc 的所有权与关闭

```rust
let (tx, mut rx) = tokio::sync::mpsc::channel::<Command>(64);

tx.send(command).await?;       // command move 进队列
while let Some(command) = rx.recv().await {
    handle(command).await;
}
```

关键语义：

- clone sender 只复制发送句柄；所有 sender（包括藏在 struct/task 中的 clone）drop 后，receiver 排空缓冲区，`recv()` 才返回 `None`。
- receiver drop 后，后续发送失败并把未发送消息放在 SendError 中返回。
- bounded `send().await` 可能等待容量；等待本身是取消点。
- unbounded send 不等待，生产者持续快于消费者会消耗无限内存。
- receiver 通常不能 clone，保证一条消息由唯一消费端取得。

“程序为何无法 shutdown”时，搜索所有 sender clone；一个遗留 clone 足以让 receiver 永远等不到 `None`。

## 背压是产品策略

有界队列满时，不同业务可能选择：

| 策略 | 合适场景 | 风险 |
| --- | --- | --- |
| `send().await` 等待 | 命令不能丢，调用者可自然减速 | 形成调用链等待或死锁环 |
| `try_send` 拒绝 | 延迟优先、调用者能处理 busy | 需要明确错误/重试 |
| 合并为最新值 | 状态同步 | 中间事件被有意丢弃，应使用 watch 或显式 coalesce |
| 丢弃旧低优先级事件 | telemetry/UI refresh | 需要指标证明丢弃可接受 |
| spill 到持久化队列 | 必须可靠且峰值大 | 复杂度、磁盘和恢复成本 |

容量不是随意常数。估算生产突发、单条内存、消费延迟和超时预算，并为队列深度/拒绝计数提供观测。

Actor 自己持有 sender clone 是常见关闭错误：receiver 等 sender 全 drop，而 sender 又等 actor 退出。设计 handle 与 actor state 时要看依赖环。

## oneshot 表达请求/回复

```rust
let (reply_tx, reply_rx) = oneshot::channel();
cmd_tx.send(Command::GetState { reply: reply_tx }).await?;
let state = reply_rx.await?;
```

reply sender 随 command move 给 actor，因此只有处理该命令的路径能回答这个调用者。

区分：

- command send 失败：命令未入队。
- reply receiver 失败：reply sender 未发送就 drop；命令可能已入队甚至产生部分副作用。
- reply value 是 `Result<T, E>`：channel 成功，业务操作失败。
- caller drop reply receiver：actor `reply.send(value)` 返回原值，说明无人再等待；actor 可忽略、记录或取消昂贵工作。

Tokio oneshot sender 的 `send` 是同步方法，不需要 `.await`。它只投递一个已计算好的值。

仓库 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 是大量 mpsc/oneshot 组合的入口；[04 错误处理](./04-errors.md) 解释各层错误。

## watch：版本化的最新状态

```rust
let (tx, mut rx) = watch::channel(initial);
tx.send(new_state)?;
rx.changed().await?;
let snapshot = rx.borrow_and_update().clone();
```

- receiver 创建时已有 initial，但它是否视为“未读变化”取决于创建/clone API 和后续读取方式。
- `borrow()` 读取当前值但不一定标记为已观察。
- `borrow_and_update()` 读取并更新 receiver 的 seen version，避免 `changed()` 立即因同一版本返回。
- 借用 guard 不应跨 await；先 clone/snapshot 所需数据。
- sender drop 后，`changed()` 最终返回错误；receiver 仍可读取最后值。
- 连续 send 多次，慢 receiver 只看到最新值，不保证逐项处理。

仓库持久化用 watch 传播 disk-full 状态，因为消费者关心当前是否磁盘满，而非每次重复变化。详见 [13 watch](./13-watch-channel.md)。

## broadcast：每位订阅者自己的游标

```rust
let (tx, mut rx) = broadcast::channel(128);
tx.send(event)?;
match rx.recv().await {
    Ok(event) => handle(event),
    Err(broadcast::error::RecvError::Lagged(skipped)) => recover(skipped),
    Err(broadcast::error::RecvError::Closed) => stop(),
}
```

Lagged 不是普通空队列：该接收者已经错过被环形缓冲覆盖的消息。恢复策略可能是重新拉取完整快照、记录并继续，或断开连接；不能无声忽略关键协议事件。

broadcast 不适合“工作只执行一次”，因为每位 receiver 都会获得一份。

## 取消是协作式协议

```rust
tokio::select! {
    _ = cancel.cancelled() => return Err(Error::Cancelled),
    result = operation() => return result,
}
```

`CancellationToken::cancel()` 只是让等待 `cancelled()` 的 Future 就绪。代码如果长时间计算、阻塞或从不检查 token，不会立即停止。

取消完整路径：

```text
发出 cancel
  -> 停止接收/创建新工作
  -> 运行中 task 在检查点观察
  -> drop/显式清理部分状态和资源
  -> 子任务、子进程传播取消
  -> flush/ack（若需要）
  -> join/确认退出
  -> 向调用者报告 Cancelled，而非伪装成功
```

取消延迟由检查点决定。CPU 循环需要周期性检查或移到可控 worker；外部阻塞调用可能无法被 Future drop 真正终止。

## clone token 与 child token

- `token.clone()` 是同一取消状态的另一个句柄，任一 clone cancel，全部观察到。
- `token.child_token()` 建立单向层级：父 cancel 会取消 child；child cancel 不反向取消父。

这适合 session → turn → tool 的取消树。若所有层都 clone 同一 token，取消一个工具可能意外取消整个 session；若层级完全断开，父 shutdown 又无法传播。

仓库 [`ToolCallContext::Cancellation`](../../crates/common/xai-tool-runtime/src/context.rs) 把 tool-call 取消令牌作为 typed extension 注入具体工具。

## drop Future 也是一种取消，但不保证回滚

`select!` 未胜出的分支 Future 通常在离开宏后被 drop，`timeout` 超时时也会 drop 被等待 Future。Rust 会 drop Future 状态机中已初始化的字段，但外部副作用可能已经发生：

- 已发送的网络请求不会自动撤回。
- 写了一半的普通文件需要临时文件+原子 rename 等协议。
- 已 spawn 的子 task 若 handle 被 drop，仍可能 detached 运行。
- 子进程需要显式 kill/wait。
- channel 消息一旦入队，drop 发送 Future 不会取回消息。

所以“Future 可被 drop”不等于操作具备事务性取消。

## cancellation safety

一个 Future 是 cancellation-safe，通常指：它在任意 Pending 点被 drop 后，重建并再次等待不会丢失/重复本应保留的进度，或 API 明确允许这种行为。

常见安全形状：

- `mpsc::Receiver::recv()`：取消等待不会取走一条未返回的消息。
- 只等待通知、状态不存于临时 Future 内的操作。

常见需审查形状：

- `read_exact`/分步解析：已读部分可能存于被 drop 的 Future。
- `send().await` 的队列公平位置：取消可能丢失排队资格。
- 内部先 mutation、后 await ack 的复合操作。
- `select!` 中每轮重新创建含局部进度的 Future。

准确保证以具体库版本文档为准。详见 [10 tokio::select!](./10-tokio-select.md)。

## Stream：随时间产生多项

Future 最终产生一个 Output；Stream 多次产生 `Option<Item>`：

```rust
while let Some(item) = stream.next().await {
    handle(item)?;
}
```

`None` 表示结束。除非 Stream 实现 `FusedStream` 或文档保证，结束后不要继续 poll。

仓库 [`ToolStream`](../../crates/common/xai-tool-runtime/src/tool.rs) 还有业务协议不变量：

```text
Progress* -> exactly one Terminal(Result<T, ToolError>) -> stream end
```

Stream 自然结束与收到 Terminal 不同。默认 [`ToolDispatch::call_terminal`](../../crates/common/xai-tool-runtime/src/dispatch.rs) 若流在 Terminal 前结束，会返回 `stream_no_terminal` 协议错误。

消费 Stream 时回答：

- item 的顺序是否重要？
- 慢消费者如何反压 producer？
- 错误在 `Item = Result<T,E>` 中，还是 Stream 自身通过结束表达？
- 取消后最后的 Terminal 是否仍保证？
- 部分 Progress 能否安全展示/持久化？

## FuturesUnordered 与 JoinSet 不是 channel

`FuturesUnordered` 是一组被当前 task poll 的 Future；`JoinSet` 管理独立 spawned task。它们按完成顺序产出，常与 mpsc 配合，但不会自动提供多生产者队列或长期 actor mailbox。

仓库工具分发先用 `FuturesUnordered` 并发执行，再由 drainer 发进 mpsc，使 actor 可以同时 select 其他事件。阅读 [`tool_calls.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/tool_calls.rs) 时分别标出：工具 Future 集合、drainer task、dispatch channel、actor consumer。

## shutdown 应有阶段和预算

可靠关闭通常不是 `cancel(); return`：

```text
1. close ingress：拒绝新命令/关闭公开 sender
2. cancel running：向当前操作和子任务传播
3. drain：处理已接收但必须完成的消息
4. flush：持久化并等待 ack
5. join：回收 task/进程
6. deadline：预算耗尽后升级 abort/kill
```

顺序取决于业务。例如先关闭 persistence receiver 再 flush 会永远等不到 ack。每个等待应有超时，但 timeout 后要说明数据可能处于什么状态。

仓库 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 的关闭辅助函数展示 cancel feedback loop、flush persistence、关闭 workflow 和最终清理等多个阶段。

## Actor 请求/回复实例

```rust
enum TerminalCommand {
    Run {
        request: TerminalRunRequest,
        reply: oneshot::Sender<Result<Output, ComputerError>>,
    },
}

let (cmd_tx, cmd_rx) = mpsc::channel(COMMAND_CHANNEL_SIZE);
let cancel = CancellationToken::new();
```

这段设计编码：

- actor 独占 `cmd_rx` 和终端状态。
- handle clone `cmd_tx` 发送 owned command。
- 每个 Run 自带独立 reply 路径。
- 有界 mailbox 对调用者施加背压。
- cancel 是 actor 生命周期信号，不替代每条 command reply。

调用者不直接共享/锁住子进程状态，所有状态变化通过 actor 串行化。

## 读 channel 的固定检查表

遇到一个 channel，写下：

1. 消息类型和所有权由谁转移？
2. sender/receiver 各有几个，clone 藏在哪里？
3. 容量是多少，满时等待、拒绝、丢弃还是合并？
4. receiver drop 后 sender 如何获知？
5. 所有 sender drop 后 receiver 如何退出？
6. 每条命令是否需 reply/ack，回复丢失意味着副作用是否未知？
7. 取消是在等待队列、执行操作还是等待回复阶段发生？
8. shutdown 谁 close、drain、flush、join？
9. 队列深度、lag、send failure 是否可观测？

## 动手练习

1. 运行 Katas 第 13 关，注释 reply send，观察 caller 失败：[`labs/katas.rs`](./labs/katas.rs)。
2. 运行 [`actor_request_reply`](./labs/async-demos/src/bin/actor_request_reply.rs) 和 [`watch_latest`](./labs/async-demos/src/bin/watch_latest.rs)，对比“每条命令必须到达”和“只保留最新状态”两种契约。
3. 从 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 选三个方法，分别画出 command sender、oneshot sender/receiver 和默认/错误策略。
4. 阅读 [`feedback_manager.rs`](../../crates/codegen/xai-grok-shell/src/session/feedback_manager.rs) 的 sync loop，列出 interval、cancel 和鉴权错误三类退出/继续条件。
5. 阅读 [`ToolStream`](../../crates/common/xai-tool-runtime/src/tool.rs) 及测试：

```sh
cargo test -p xai-tool-runtime --test tool_streaming --test trait_object_safety
```

6. 结合 [10 select](./10-tokio-select.md)、[13 watch](./13-watch-channel.md) 和 [取消/关闭源码精读](../deep-dives/cancellation-and-shutdown.md)，为一次 session shutdown 写出阶段、超时和残留风险。

完成标准：看到 `mpsc + oneshot + CancellationToken` 时，能解释命令、回复、生命周期三条独立通道，而不是笼统地说“actor 异步通信”。
