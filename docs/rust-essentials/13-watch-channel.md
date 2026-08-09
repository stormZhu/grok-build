# 13. `tokio::sync::watch`：最新状态与版本

`watch` 保存一个最新值，并让每个 receiver 等待“自上次确认后有新版本”。它不是事件队列：中间值可以被覆盖，慢消费者只保证能看到最新状态。通道选型、关闭和背压见 [09 通道、取消与 Stream](./09-channels-cancellation-streams.md)。

## 用版本号建立心智模型

可以把 channel 想成：

```text
共享槽位：(value, global_version)
每个 Receiver：last_seen_version
```

发送更新时，槽位替换并推进 version。receiver 的 `changed().await` 在 global version 比自己的 last seen 新时完成，并把当前版本标为已见；随后用 `borrow()` 读取值。

`borrow_and_update()` 则在同一个同步步骤里读取最新值并把当前 version 标为已见。这能避免 `changed()` 与 `borrow()` 之间又发生发送时造成的竞态。

## 最小正确循环

```rust
use tokio::sync::watch;

let (tx, mut rx) = watch::channel(Config::default());

let consumer = tokio::spawn(async move {
    while rx.changed().await.is_ok() {
        let snapshot = rx.borrow_and_update().clone();
        apply(snapshot).await;
    }
    // 所有 Sender 已 drop，且最后一个未见版本已消费。
});

tx.send_replace(new_config);
drop(tx);
consumer.await?;
```

不要让 `watch::Ref<'_, T>` 跨 `.await`：它持有对槽位的读 guard，可能阻塞 sender，且经常让 future 失去 `Send`。在 await 前 clone/copy 所需 snapshot。

## `borrow` 与 `borrow_and_update`

| API | 读取当前值 | 标记当前版本已见 | 常见用途 |
| --- | --- | --- | --- |
| `borrow()` | 是 | 否 | 同步快照，不改变下次 `changed()` 语义 |
| `borrow_and_update()` | 是 | 是 | `changed()` 完成后取出与通知一致的最新状态 |
| `has_changed()` | 否 | 否 | 同步查询是否有未见版本；channel 关闭时返回错误 |

一个常见 race：

```text
changed() 完成于 v1
发送者写入 v2
receiver borrow() 读到 v2，但只把 v1 视为已见
下一次 changed() 立刻再返回，重复处理 v2
```

`borrow_and_update()` 把“读到哪个版本”和“标记哪个版本”统一起来。若业务允许重复处理，`borrow()` 也可能可用，但必须是有意选择。

## 初始值是不是事件

新 `Receiver` 创建或通过 `Sender::subscribe()` 得到时，初始值被视为已经看过；`changed()` 等后续发送。你可以随时用 `borrow()` 读取当前 snapshot。

仓库 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 在进入循环前显式：

```rust
let mut model_switch_rx = session.models_manager.subscribe_model_switch();
let _ = *model_switch_rx.borrow_and_update();
```

这行表达设计意图：订阅时已有 generation 是基线，不应触发“用户刚切换模型”的处理。即使 Tokio 订阅语义已把它视为已见，显式消费也让维护者知道初始 snapshot 不是业务事件。

## 发送 API 的差异

`watch::Sender` 可以 clone，因此类型层面允许多个 producer；项目设计仍应尽量指定一个状态 owner，避免多个 writer 争夺“最新真相”。

| API | 行为 | 无 Receiver 时 |
| --- | --- | --- |
| `send(value)` | 替换值并通知，返回 `Result` | 返回 `SendError`，新值不会进入 channel |
| `send_replace(value)` | 始终替换并通知，返回旧值 | 仍保存新值，未来订阅者可见 |
| `send_modify(|v| ...)` | 原地修改并通知 | 仍修改保存值 |
| `send_if_modified(|v| -> bool)` | closure 返回 true 才通知 | 仍执行 closure，按返回值决定版本推进 |

普通 `send`/`send_replace` 不会用 `PartialEq` 判断值是否真的不同；重复发送相等值仍可产生新版本和通知。若必须只在语义变化时通知，发送端显式比较，或使用 `send_if_modified`。

仓库 [`ModelsManager`](../../crates/codegen/xai-grok-shell/src/agent/models.rs) 先在锁内比较 model id，只在确实变化时推进 generation：

```rust
fn set_current_model_id_internal(&self, id: ModelId) {
    let changed = {
        let mut current = self.inner.current_model_id.write();
        let changed = *current != id;
        *current = id;
        changed
    };

    if changed {
        self.inner
            .model_switch_watch
            .send_modify(|generation| *generation += 1);
    }
}
```

generation 而不是 model id 本身充当通知值，说明消费者关心“发生过一次切换”以及当前序号。多个快速切换仍可能合并成一次处理，但 receiver 读到的是最终 generation。

## 状态通道与事件通道

选择 watch 前先问“漏掉中间值是否正确”：

| 需求 | 常用原语 |
| --- | --- |
| 当前配置、ready flag、连接状态、最新 generation | `watch` |
| 每条命令都必须按序处理 | `mpsc` |
| 每个订阅者都应尝试看到每个事件，允许 lag 错误 | `broadcast` |
| 一次请求对应一次回复 | `oneshot` |
| 只需协作式取消 | `CancellationToken` |
| 只需唤醒，不携带状态 | `Notify`，但要分析 permit 语义 |

若“启动 -> 进度 50% -> 完成”三个阶段都必须触发副作用，watch 不合适；慢 receiver 可能直接看到完成。若 UI 只需显示当前进度，覆盖中间值通常正是优势。

## 关闭语义

所有 sender drop 后：

- receiver 仍能借用最后保存值。
- 若还有未见版本，`changed()` 可先成功一次。
- 当前版本已见且 channel 关闭后，`changed()` 返回 `RecvError`。
- `Sender::closed().await` 可等待所有 receiver drop，适合 producer 停止昂贵后台工作。

不要写成忽略错误的忙循环：

```rust
loop {
    let _ = rx.changed().await; // channel 关闭后立即反复返回 Err
    process(rx.borrow().clone()).await;
}
```

应在错误时 break，或明确切换到 shutdown 分支。

## `select!` 与取消安全

`Receiver::changed()` 可安全放进 `select!` 循环：其他分支获胜而取消这次等待时，不会把一个尚未观察的版本错误标为已见。真正读取值时仍要采用一致的 `borrow_and_update` 模式。

```rust
loop {
    tokio::select! {
        result = rx.changed() => {
            if result.is_err() {
                break;
            }
            let state = rx.borrow_and_update().clone();
            handle(state).await;
        }
        _ = shutdown.cancelled() => break,
    }
}
```

若 handler 很慢，期间多个版本合并是 watch 契约，不是 channel bug。需要逐条处理就换原语。

## 仓库中的两种状态形状

1. [`ModelsManager`](../../crates/codegen/xai-grok-shell/src/agent/models.rs) 用 `watch<u64>` 传播模型切换 generation；session 只需按最新 generation 重置策略。
2. [`PersistenceHandle`](../../crates/codegen/xai-grok-shell/src/session/persistence.rs) 持有 `watch::Receiver<bool>` 表示当前 disk-full 状态；新订阅者需要立即读到现状，而不是回放每次磁盘状态变化。

前者的值表示单调版本，后者表示 level-triggered 状态。两者都允许合并中间更新，但消费逻辑不同。

## 测试矩阵

watch 相关测试至少覆盖：

1. 订阅者能读初始 snapshot，且它是否应触发业务处理。
2. 一次发送后 `changed` 完成并读到新值。
3. 快速多次发送后，慢 receiver 读到最终值，测试不错误要求中间值全到达。
4. 相等值是否应通知；若不应，验证发送端比较或 `send_if_modified`。
5. 所有 sender drop 后循环退出，不 busy-spin。
6. handler 跨 await 时没有持有 `watch::Ref`。
7. 无 receiver 时选用的 send API 是否仍需保存值。

异步测试不要用 sleep 猜 `changed()` 是否已注册；通过发送、oneshot barrier 和 timeout 建立确定顺序。见 [07 测试](./07-testing.md)。

## 阅读练习

1. 运行 [`watch_latest`](./labs/async-demos/src/bin/watch_latest.rs)，分别解释覆盖中间值、同值新版本和 sender 全部 drop 三个断言。
2. 跟踪 [`ModelsManager::set_current_model_id_internal`](../../crates/codegen/xai-grok-shell/src/agent/models.rs) 到 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)，写出 producer、保存值、receiver 和副作用。
3. 假设 model id 在 receiver 处理前切换三次，预测 generation、`changed()` 次数的允许范围和最终处理值。
4. 找到 persistence 的 disk-full sender，判断它使用 `send`、`send_replace` 还是其他 API；解释无 receiver 时应不应该保留新状态。
5. 将一个 `mpsc` 使用点套入“漏中间值是否正确”的问题，说明为什么不能机械换成 watch。

完成标准：能准确说明 watch 保存的是状态而非事件，写出无竞态的接收循环，并对同值发送、多 producer、关闭、慢消费者和无 receiver 发送行为做出正确预测。
