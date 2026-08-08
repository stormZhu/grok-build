# 11. Pin 与异步 Future

> Pin 是 Future 实现细节的一部分。普通业务代码通常只需在 `select!` 中复用 Future 或调用需要 `Pin<&mut T>` 的 API 时理解它；任务模型见 [8. async、任务与 `Send`](./08-async-runtime-tasks.md)。

## 通俗理解

### 先理解问题：为什么需要 Pin？

Rust 的 `Future` 本质上是一个状态机，poll 一次走一步，poll 多次走完。问题出在：有些 future 内部会**自己指自己**（自引用）。

想象一个带定时器的 future：

```
┌─────────────────────────┐
│  Sleep {                │
│    deadline: 12:00:00   │
│    timer: ──────┐       │  ← timer 指向自己内部的 deadline 字段
│                 │       │
└─────────────────┼───────┘
                  │
                  └── 内部指针
```

如果这个 future 被**移动**到另一个内存位置，那个内部指针就成了**野指针**——指向旧地址，新地址已经空了。

> **就像你搬家了，但快递地址还是写的旧地址——快递永远送不到。**

`Pin` 就是一把"钉子"，把 future **钉在原地**，保证它永远不会被移动。

---

### Pin 怎么用？

```rust
// 创建一个 Sleep future
let idle_flush_sleep = tokio::time::sleep(Duration::from_secs(60));

// 把它钉住
tokio::pin!(idle_flush_sleep);

// 之后只能通过 &mut 引用去 poll 它，不能移动
tokio::select! {
    _ = &mut idle_flush_sleep => { /* 定时器到期 */ }
}
```

**关键**：`pin!` 之后，你不再直接持有 `idle_flush_sleep`，而是持有 `Pin<&mut Sleep>`——一个"钉住的引用"。你可以 poll 它，但不能把它移走。

---

### 通俗类比

| 操作          | 类比                   |
| ------------- | ---------------------- |
| 创建 future   | 装好一个定时炸弹       |
| `tokio::pin!` | 把它焊在地上           |
| `&mut` poll   | 按按钮让它倒计时走一步 |
| 移动 future   | ❌ 焊死了，搬不动      |
| `.reset()`    | 重新设定倒计时时间     |

---

## Sleep::reset() 方法

定时器到期后，不需要拆了重装一颗炸弹——直接**重新设定倒计时**就行：

```rust
// 重置定时器，等待下一轮
if let Some(timeout) = session.idle_flush_timeout {
    idle_flush_sleep.as_mut().reset(tokio::time::Instant::now() + timeout);
}
```

`.as_mut()` 获取 `Pin<&mut Sleep>`，然后 `.reset()` 改到期时间，无需重建。

---

## 永不触发的定时器技巧

## 常见边界

- `Pin` 不等于线程安全，也不等于不可修改；它限制的是值在内存中的移动方式。
- `tokio::pin!` 固定局部变量的存放位置，适合当前 async 作用域；把 future 放进结构体或跨 API 保存时，要重新确认其类型与生命周期。
- 在 `select!` 中反复构造 future 会丢掉先前进度；需要等待同一个 `Sleep`、stream 或操作时，应在循环外创建并按 API 要求 pin/reset。

项目里有个巧妙用法：功能没开启时，设一个**永不到期**的定时器，配合 `select!` 的 `if` 守卫，零开销跳过：

```rust
let idle_flush_sleep = match session.idle_flush_timeout {
    Some(timeout) => tokio::time::sleep(timeout),
    None => tokio::time::sleep(std::time::Duration::MAX),  // ≈ 2.8 亿年，永不到期
};
tokio::pin!(idle_flush_sleep);
```

> 把定时器设成 2.8 亿年后到期，反正等不到，就当它不存在。`if` 守卫再补一刀——条件不满足时根本不 poll，双重保险。
