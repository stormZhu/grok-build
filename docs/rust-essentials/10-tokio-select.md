# 10. tokio::select! 宏

> 在阅读本篇前，先了解 [8. async、任务与 `Send`](./08-async-runtime-tasks.md)；通道与取消语义见 [9. 通道、取消与 Stream](./09-channels-cancellation-streams.md)。

`select!` 同时等待多个异步操作，**哪个先完成就执行哪个**，其余的被取消。

---

## 通俗理解

### 把 `select!` 的一个分支想象成一句话

```rust
_ = &mut dream_check_sleep, if 条件A && 条件B => { 做事 }
```

把它拆成三块，**逗号是分隔符**：

| 部分           | 代码                         | 含义               |
| -------------- | ---------------------------- | ------------------ |
| ① 等什么       | `_ = &mut dream_check_sleep` | 等待这个定时器到期 |
| ② 前提条件     | `, if 条件A && 条件B`        | 但只有满足条件才等 |
| ③ 到期后做什么 | `=> { 做事 }`                | 到期后执行这段代码 |

---

### 用日常语言类比

就像说：

> **"如果** 今天下雨 **且** 我带了伞，**就** 等公交车来，车来了 **就** 上车。"

翻译成 select! 语法：

```rust
_ = 等公交车(), if 今天下雨 && 我带了伞 => { 上车 }
```

- 不下雨？那这整行直接跳过，根本不去等公交。
- 下雨但没带伞？也跳过。
- 下雨且带了伞？那就等公交，车来了就上车。

---

### 回到项目代码

```rust
_ = &mut dream_check_sleep, if session.dream_check_timeout.is_some()
    && session.memory.is_enabled() => {
    // dream_check 定时器到期了，执行 dream consolidation
}
```

翻译成人话：

> **如果** 用户配置了 dream_check 超时 **且** memory 功能开启了，**就** 等这个定时器到期，到期了 **就** 执行 dream consolidation。

两个条件缺一个，这整段就不参与——定时器不会被 poll，相当于这行不存在。

---

### 对比：没有 `if` 的分支

```rust
changed = model_switch_rx.changed() => { ... }
```

这里没有 `, if ...`，所以**无条件**每次都参与 poll，等 `model_switch_rx` 发生变化。

**关键**：`, if` 是一个**可选过滤器**，逗号前面是"等什么"，逗号后面是"什么情况下才等"。

---

## 10.1 基本语法

```rust
tokio::select! {
    // 格式: 模式 = future表达式, if 可选条件守卫 => { 处理代码 }
    result = async_operation_1() => {
        // 处理 operation_1 的结果
    }
    result = async_operation_2() => {
        // 处理 operation_2 的结果
    }
}
```

## 10.2 完整分支结构

一个分支由三部分组成，用**逗号**分隔：

```
模式 = future表达式  ,  if 条件守卫  =>  { 处理代码 }
 ①         ②            ③               ④
```

| 部分                    | 含义                            | 是否必须 |
| ----------------------- | ------------------------------- | -------- |
| ① `模式 = future表达式` | 等待什么 future，如何接收返回值 | ✅ 必须  |
| ② `, if 条件`           | 满足条件才参与 poll             | ❌ 可选  |
| ③ `=>`                  | 分隔符                          | ✅ 必须  |
| ④ `{ 处理代码 }`        | future 就绪后执行               | ✅ 必须  |

## 10.3 实际项目代码

来自 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L309)：

> 下面保留源码原有注释，并额外用“学习注释”标出与本项目会话 Actor 相关的读法；省略的分支不改变 `select!` 的调度规则。

```rust
loop {
    tokio::select! {
        biased;  // ① 模式修饰符，不是分支

        // ② 带 if 条件守卫的分支：条件不满足时，整个分支不参与 poll。
        // 学习注释：SessionActor 因此不会在 memory 功能关闭时处理 idle flush。
        _ = &mut idle_flush_sleep, if session.idle_flush_timeout.is_some()
            && session.memory.is_enabled()
            && !session.memory.is_flushing.load(Ordering::Relaxed) => {
            // 定时器到期，执行 flush 逻辑...
        }

        // ③ 另一个带条件守卫的分支
        _ = &mut dream_check_sleep, if session.dream_check_timeout.is_some()
            && session.memory.is_enabled() => {
            // 定时器到期，执行 dream consolidation...
        }

        // ④ 无条件守卫的分支（每次都参与 poll）
        changed = model_switch_rx.changed() => {
            // 模型切换时触发...
        }

        // ⑤ 无条件守卫 + 接收 channel 消息
        event = chat_state_event_rx.recv() => {
            // 处理状态事件...
        }
    }
}
```

## 10.4 `biased;` 模式修饰符

```rust
tokio::select! {
    biased;  // 这行不是分支，没有 => 是正常的
    // ...
}
```

- **默认**：多个 future 同时就绪时，**随机**选择一个（公平轮询）
- **`biased;`**：多个 future 同时就绪时，**按书写顺序**优先选择排在前面的

## 10.5 `if` 条件守卫详解

```rust
_ = &mut idle_flush_sleep, if 条件A && 条件B => { ... }
```

- `_ = &mut idle_flush_sleep` — 等什么：poll 这个定时器 future，忽略返回值（`_`）
- `, if 条件A && 条件B` — 前提条件：条件为 `false` 时，定时器**根本不会被 poll**
- `=> { ... }` — 到期后执行什么

**为什么用 `if` 守卫？** 当用户没有配置超时（`timeout` 为 `None`）时，对应的 `Sleep` 被设为 `Duration::MAX`（永不触发）。用 `if` 守卫直接跳过，避免无意义地 poll 一个永远不会到期的 future，零开销。

## 10.6 模式匹配接收返回值

`模式 = future表达式` 中的“模式”不是普通赋值，而是 Rust 的**模式匹配**：future 完成后，把它的返回值交给左边的模式。如果匹配成功，就进入 `=>` 后面的处理块。

```rust
// 忽略返回值
_ = some_future() => { ... }

// 绑定返回值
result = some_future() => {
    println!("got: {result}");
}

// 解构返回值
Ok(data) = fallible_future() => { ... }
Err(e) = fallible_future() => { ... }
```

### 变量什么时候可以使用？

模式中的变量只有在 future 完成、模式匹配成功并进入处理块后才会绑定。因此，它从 `=> {` 开始可用，到这个处理块结束为止：

```rust
tokio::select! {
    message = receiver.recv() => {
        println!("收到: {message}"); // 可以使用 message
    }

    _ = cancel.cancelled() => {
        // 这里不能使用 message：这是另一个分支
    }
}
```

变量不能在 `if` 守卫中使用，因为守卫是在等待 future **之前**判断的，此时还没有返回值：

```rust
tokio::select! {
    // 错误示例：message 尚未产生
    message = receiver.recv(), if message.is_some() => { ... }
}
```

变量也不会自动带出 `select!`。如果后面的代码需要它，让 `select!` 的分支返回这个值：

```rust
let message = tokio::select! {
    message = receiver.recv() => message,
    _ = cancel.cancelled() => return,
};

println!("收到: {message}"); // 现在可以在 select! 外使用
```

常见模式的含义：

```rust
_ = sleep(...) => { ... }              // 匹配任何值，但忽略返回值
result = some_future() => { ... }      // 绑定完整返回值
Ok(data) = read_file() => { ... }      // 只匹配 Ok，并取出 data
Some(message) = receiver.recv() => { ... } // 只匹配 Some，并取出 message
```

如果模式不匹配，该分支在本次 `select!` 调用中会被跳过，继续等待其他分支。例如 `Some(message)` 不会匹配 channel 关闭时的 `None`。

## 10.7 取消安全与公平性

未选中的分支 future 会被丢弃，因此每个分支都应是取消安全的，或在下次进入循环后能正确恢复。读取一次消息、推进一次流或修改外部状态的 future 若在 poll 中途被丢弃，可能造成丢事件或重复操作；优先把状态保存在 loop 外，或使用明确记录进度的 API。

`biased;` 不是性能开关，而是调度策略。高频且排在前面的就绪分支可能饿死后面的分支；只有存在明确优先级时使用，并将关闭/取消等必须及时响应的分支放在合适位置。
