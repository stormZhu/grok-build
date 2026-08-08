# 5. tokio::sync::watch 通道

## 5.1 基本概念

`watch` 是一个**单生产者、多消费者**通道，用于广播状态变化。它始终保存最新值，新订阅者立即收到当前值。

## 5.2 核心 API

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

## 5.3 项目中的使用

```rust
// 订阅模型切换通知
let mut model_switch_rx = session.models_manager.subscribe_model_switch();

// 先 consume 当前值，避免刚订阅就触发 "changed"
let _ = *model_switch_rx.borrow_and_update();

// 在 select! 中等待变化
tokio::select! {
    changed = model_switch_rx.changed() => {
        if changed.is_ok() {
            let new_gen = *model_switch_rx.borrow_and_update();
            session.handle_model_switch_for_laziness(new_gen).await;
        }
    }
}
```

## 5.4 关键细节

- `changed()` 只在值**实际变化**时返回，如果连续两次 `send(42)`，第二次不会触发 `changed()`
- `borrow_and_update()` 同时读取当前值并标记为"已读"，下次 `changed()` 会等待新变化
- 先调用 `borrow_and_update()` 消耗初始值，避免订阅后立即触发（代码注释中"no stored-permit hazard"指的就是这个）