# 11. `Pin`、`Unpin` 与可复用 Future

多数业务代码不需要手写 `Pin`。但读到 `tokio::pin!`、`Pin<&mut T>`、`Box::pin`、`Sleep::reset()` 或手写 `Future` 时，必须知道它限制的是移动，不是修改、线程或生命周期。任务模型见 [08 async、任务与 `Send`](./08-async-runtime-tasks.md)，多路等待见 [10 `tokio::select!`](./10-tokio-select.md)。

## 从 `Future::poll` 看问题

`Future` 的核心方法是：

```rust
trait Future {
    type Output;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}
```

编译器把 `async` 代码变成状态机。每次 `poll` 从上次暂停位置继续，返回：

- `Poll::Pending`：尚未完成，future 已安排在条件变化时通过 waker 再唤醒。
- `Poll::Ready(value)`：完成并产出结果，通常不能再 poll。

为什么接收者不是普通 `&mut Self`？某些状态机在开始 poll 后可能含有指向自身字段的引用，或者底层类型以地址稳定为安全前提。若此时把整个值按字节移动到新地址，内部引用就可能失效。`Pin` 让类型能够依赖“被 pin 后不再移动”这个承诺。

## `Pin<P>` 承诺了什么

`Pin<P>` pin 的是指针 `P` 所指向的值：通过这个 pinned pointer，不能再安全地取得允许移动该值的访问方式。

它不表示：

- 值不可修改。安全的字段和专门 API 仍可修改。
- 值永远不会 drop。drop 不等于先移动到别处。
- 值是 `Send`/`Sync`，或只能在一个线程使用。
- 引用变成 `'static`。
- 内存永远在 heap；stack 上也可以 pin。

Pin 的约束主要对 `!Unpin` 类型有意义。实现 `Unpin` 的类型即使被 `Pin<&mut T>` 包住，也允许安全取回 `&mut T` 并移动；大多数普通数据类型属于这一类。编译器生成的 async future 通常不能假定为 `Unpin`，因此组合器和 `select!` 常要求先 pin。

## stack pin 与 heap pin

局部等待通常使用 stack pin：

```rust
let sleep = tokio::time::sleep(Duration::from_secs(30));
tokio::pin!(sleep);

tokio::select! {
    _ = &mut sleep => { /* deadline 到达 */ }
}
```

`tokio::pin!` 在当前作用域内建立一个 pinned 局部绑定，不分配 heap。这个 pinned 引用不能逃出其所有者的生命周期。

需要把异构 future 放进集合、结构体或跨调用保存时，常见的是：

```rust
let task: Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> =
    Box::pin(run_operation());
```

`Box::pin` 在 heap 上拥有值并保证地址稳定。它仍不自动延长被 future 借用的数据；若 trait object 要求 `'static`，future 仍须拥有捕获值。

## 为什么循环外创建 Future

下面两种代码语义不同：

```rust
// 每轮创建一个全新的 sleep；其他分支获胜时，旧 sleep 被 drop。
loop {
    tokio::select! {
        _ = tokio::time::sleep(timeout) => on_timeout(),
        msg = rx.recv() => on_message(msg),
    }
}
```

```rust
// 同一个 Sleep 跨 select 轮次保留进度，并可显式重新设 deadline。
let sleep = tokio::time::sleep(timeout);
tokio::pin!(sleep);

loop {
    tokio::select! {
        _ = &mut sleep => {
            on_timeout();
            sleep.as_mut().reset(Instant::now() + timeout);
        }
        msg = rx.recv() => on_message(msg),
    }
}
```

第一种是否正确取决于意图：若“每收到一条消息就重新计算超时”，重建可能正合适；若要求从固定起点累计到 deadline，反复 drop 会把超时无限推迟。Pin 不是根本目的，保存同一个有状态 future 才是这里的行为契约。

## 仓库中的两个 pinned timer

[`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 为 idle flush 和 dream check 各创建一个 `Sleep`：

```rust
let idle_flush_sleep = match session.idle_flush_timeout {
    Some(timeout) => tokio::time::sleep(timeout),
    None => tokio::time::sleep(Duration::MAX),
};
tokio::pin!(idle_flush_sleep);

loop {
    tokio::select! {
        _ = &mut idle_flush_sleep,
            if session.idle_flush_timeout.is_some() =>
        {
            // 启动本轮工作后重新设定同一个 timer。
            if let Some(timeout) = session.idle_flush_timeout {
                idle_flush_sleep
                    .as_mut()
                    .reset(tokio::time::Instant::now() + timeout);
            }
        }
        // 其他事件分支
    }
}
```

从类型角度读这段代码：

1. `sleep(...)` 先创建拥有状态的 `Sleep`。
2. `tokio::pin!` 让后续通过 `Pin<&mut Sleep>` 访问它。
3. `&mut idle_flush_sleep` 交给 `select!` poll，而不转移所有权。
4. `as_mut()` 只重新借用 pinned pointer，避免消费它。
5. `reset()` 是 `Sleep` 提供的 pin-aware 修改 API。
6. `if` guard 在功能禁用时不 poll 分支；`Duration::MAX` 同时提供不会正常到期的初始 deadline。

这里的两个机制分工不同：`if` 才是“禁用本次等待”的开关，条件为 `false` 时该分支不能获胜；`Duration::MAX` 则是 `None` 时的占位 `Sleep`。即使功能关闭，下面的分支在编译时仍必须能引用一个类型固定的 `idle_flush_sleep`，才能在 loop 外 pin 并在同一份 `select!` 中复用：

```rust
_ = &mut idle_flush_sleep, if session.idle_flush_timeout.is_some() => { ... }
```

也可以保存 `Option<Sleep>`，但那样需要在 `select!` 内把 `None` 转为一个永远 `Pending` 的 Future，或复制分支逻辑。远未来的 `Sleep` 使这一处状态和类型保持简单；它不是第二次条件判断。

## Pin 与取消安全不是同一件事

把 future pin 住，不代表它可以被随时 drop 后无损重建。`select!` 中其他分支获胜时，未获胜分支本轮建立的 future 会被 drop；是否丢数据由该操作的 cancellation safety 决定。

例如，保存 `Sleep` 的进度靠循环外 pin；但一个协议读操作若在收到半帧后被 drop，重建能否继续取决于 API 契约。遇到 `select!` 循环要分别问：

- future 是否跨轮复用？
- 未完成时被 drop 是否会丢状态、队列位置或部分输入？
- 完成后会不会再次 poll 同一个不可重用 future？
- 是否需要 `Fuse`、`Option::take`、重新构造或显式状态 enum？

## 什么时候会手写 Pin 代码

常见情形只有几类：

- 实现 `Future`/`Stream`，poll 内部字段。
- 持有 `Pin<Box<dyn Future<...>>>` 的类型擦除任务。
- 使用要求 `Pin<&mut T>` 的 timer、stream 或 I/O API。
- 构造自引用或地址敏感类型。

手写投影到结构体字段很容易破坏 pin 不变量。项目若采用 `pin-project` 一类库，应跟随其生成的安全投影；不要用 `unsafe { Pin::new_unchecked(...) }` 只为消除类型错误。使用 unsafe 前必须证明值从此不会再通过任何别名被移动。

## 诊断常见错误

| 症状 | 先检查 |
| --- | --- |
| `cannot be unpinned` / `Unpin` bound 不满足 | API 是否要求可移动 future；能否在调用点 `pin!`/`Box::pin` |
| `select!` 循环中借用或 move 报错 | future 是否应在循环外创建，并每轮传 `&mut` |
| timer 永远不到 | 是否每轮重建并重置了等待起点；branch guard 是否始终为 false |
| 完成后再次 poll panic | 是否需要完成后退出、替换 future 或保存 done 状态 |
| 想返回 `Pin<&mut T>` 但生命周期不够 | pinned owner 是否只是当前 stack 局部；应否由调用者拥有或改用 `Pin<Box<T>>` |

## 阅读练习

1. 运行 [`timer_reset`](./labs/async-demos/src/bin/timer_reset.rs)，逐行标出 pinned owner、每次 `.as_mut()` 重借用和三次 deadline；再尝试解释如果每轮新建 `Sleep` 会改变什么。
2. 打开 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)，画出 idle 和 dream 两个 timer 在“分支触发、其他分支获胜、功能禁用”三种情况下的状态。
3. 暂时把 timer 构造移进 loop 的 `select!` 分支，只做纸面推演：连续收到消息时 deadline 会如何变化？不要修改仓库源码。
4. 写一个接收 `Pin<&mut tokio::time::Sleep>` 的 helper，解释调用时为什么需要 `.as_mut()`。
5. 对一个 `select!` 中的非 timer future 查其 cancellation-safety 文档，说明 pin 是否解决了相同问题。

完成标准：看到 `Pin<&mut T>` 时，能指出 pinned owner 在哪里、为何要保持地址或进度、何时会 drop，以及 `Unpin`、`Send`、生命周期和取消安全分别是不是相关约束。
