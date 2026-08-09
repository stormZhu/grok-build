# 13. tokio::sync::watch 通道

> 通道选型、取消和背压见 [9. 通道、取消与 Stream](./09-channels-cancellation-streams.md)。

## 13.1 基本概念

`watch` 是一个**单生产者、多消费者**通道，用于广播状态变化。它始终保存最新值，新订阅者立即收到当前值。

## 13.2 核心 API

```rust
use tokio::sync::watch;

// 创建通道，初始值为 0
let (tx, mut rx) = watch::channel(0);

// 发送端：更新值（通知所有订阅者）
tx.send(42).unwrap();

// 接收端：等待值变化
rx.changed().await.unwrap();     // 等待直到值发生变化
let val = *rx.borrow_and_update(); // 读取当前值并标记为"已读"
```

## 13.3 项目中的使用

```rust
// 源码节选自 SessionActor run_loop。
// 订阅模型切换通知；rx 保存的是“最新 generation”，不是全部切换历史。
let mut model_switch_rx = session.models_manager.subscribe_model_switch();

// 先消费订阅时已有的值，避免第一次 changed() 读取陈旧 permit。
let _ = *model_switch_rx.borrow_and_update();

// 在 select! 中等待下一次变化；发送端全关闭时 changed 为 Err。
tokio::select! {
    changed = model_switch_rx.changed() => {
        if changed.is_ok() {
            // 同时读取最新值并标记已消费，下一轮只等更新后的 generation。
            let new_gen = *model_switch_rx.borrow_and_update();
            session.handle_model_switch_for_laziness(new_gen).await;
        }
    }
}
```

对应源码见 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L295)。这个用法说明 `watch` 传递的是状态快照：若两次模型切换发生在同一次处理前，接收者只需要处理最终 generation。

## 13.4 关键细节

## 13.5 何时不要用 watch

`watch` 只保证接收者最终能读取最新值，不能保证看见每一次中间更新。它适合配置、开关、当前状态；需要按顺序处理每个命令用 `mpsc`，需要一问一答用 `oneshot`，需要每个订阅者处理事件则评估 `broadcast`。发送者全部 drop 后，`changed()` 会返回错误，循环应明确处理关闭语义。

- `changed()` 只在值**实际变化**时返回，如果连续两次 `send(42)`，第二次不会触发 `changed()`
- `borrow_and_update()` 同时读取当前值并标记为"已读"，下次 `changed()` 会等待新变化
- 先调用 `borrow_and_update()` 消耗初始值，避免订阅后立即触发（代码注释中"no stored-permit hazard"指的就是这个）

### 项目关键代码：先清除初始观察值再进入主循环

```rust
// 源码节选，位于 run_loop 的 select! 之前。
let mut model_switch_rx = session.models_manager.subscribe_model_switch();

// watch receiver 创建时已经“看见”当前值；这里明确将它标为已读。
let _ = *model_switch_rx.borrow_and_update();

loop {
    tokio::select! {
        changed = model_switch_rx.changed() => {
            if changed.is_ok() {
                let new_gen = *model_switch_rx.borrow_and_update();
                session.handle_model_switch_for_laziness(new_gen).await;
            }
        }
        // 其他 SessionActor 事件分支 ...
    }
}
```

该模式的关键不是 `watch` API 本身，而是“订阅时已有状态不当成新事件”：这保证模型切换处理只对订阅后的 generation 运行。
