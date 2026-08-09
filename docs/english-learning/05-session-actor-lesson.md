# 综合课：用英语解释 SessionHandle 与 SessionActor

这是本目录的第一节完整课程，主题与你当前的源码学习一致。建议用 45--60 分钟完成，先独立回答，再看后面的参考表达。

## 本课目标

技术目标：

- 区分 `SessionHandle`、`SessionActor` 和 `SessionCommand`；
- 跟踪 `SessionHandle::is_busy` 的 request/reply；
- 解释 MPSC、oneshot 和保守 fallback 的作用。

英语目标：

- 使用 `proxy`、`own`、`send through`、`reply through`；
- 用 `if`、`when` 和 `so that` 描述控制流与目的；
- 用简单英文准确解释一条异步消息链。

## 1. 先建立技术模型

```text
multiple callers
      │
      │ clone and hold
      ▼
SessionHandle
      │
      │ send SessionCommand (MPSC)
      ▼
SessionActor / run_session
      │
      │ send one reply (oneshot)
      ▼
requesting caller
```

关键区别：

| 概念 | 技术职责 | 推荐英文 |
| --- | --- | --- |
| `SessionActor` | 持有会话状态，处理消息，协调 turn 生命周期 | The actor owns and coordinates the session state. |
| `SessionHandle` | 可共享的外部操作入口 | The handle is a clonable proxy for the actor. |
| `SessionCommand` | Handle 与 Actor 之间的类型化消息协议 | The enum defines the commands that drive the actor. |
| MPSC channel | 多个 sender 向一个 receiver 发送 command | Multiple handles can send commands to one receiving loop. |
| oneshot channel | 携带某次 query 的单个 reply | A oneshot channel carries one reply back to the caller. |

`SessionHandle::clone()` 会复制句柄中可克隆的 sender 和共享引用，不会复制一份独立 `SessionActor` 状态。

## 2. 阅读第一段英文

源码：[handle.rs](../../crates/codegen/xai-grok-shell/src/session/handle.rs)

```text
`SessionHandle` — the `Clone + Send` proxy for interacting with a session actor.

Callers hold a `SessionHandle` and send `SessionCommand` messages via the
internal channel. Extracted from `acp_session.rs` to keep the actor
implementation focused on behaviour.
```

### 拆解

`the Clone + Send proxy` 的中心词是 `proxy`。`Clone + Send` 是 Rust trait 性质，说明它可以被克隆，并能够跨线程边界转移；它们不是两个业务动作。

`for interacting with a session actor` 修饰 `proxy`，说明用途。

第二句包含两个并列动作：

```text
Callers hold a SessionHandle.
Callers send SessionCommand messages via the internal channel.
```

第三句省略了主语，完整意思是：

```text
The handle code was extracted from `acp_session.rs` to keep the actor
implementation focused on behaviour.
```

`to keep` 表示重构目的：拆分文件让 actor 实现更聚焦于行为。

### 主动词组

```text
a proxy for interacting with ...
hold a handle
send messages via a channel
extract A from B
keep the implementation focused on ...
```

## 3. 阅读第二段英文

源码：[commands.rs](../../crates/codegen/xai-grok-shell/src/session/commands.rs)

```text
`SessionCommand` defines the message protocol used to drive a session actor.
```

句子主干：

```text
SessionCommand defines the protocol.
```

限定：

```text
什么 protocol：message protocol
用来做什么：used to drive a session actor
```

这里的 `protocol` 是进程内 Rust enum 构成的消息约定，不一定表示网络协议。`drive` 表示这些命令促使 Actor 改变或查询状态。

推荐复述：

```text
`SessionCommand` is an enum that describes the operations the session actor
can receive. Together, its variants form the actor's internal message protocol.
```

## 4. 跟踪 `is_busy`

### Step 1：状态谓词

源码：[acp_session.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session.rs)

```rust
pub(crate) fn state_is_busy(state: &State) -> bool {
    state.running_task.is_some() || !state.pending_inputs.is_empty()
}
```

英文解释：

```text
A session is busy when it has either a running task or at least one queued
input. The function only borrows the state and returns a Boolean value.
```

注意 `either A or B` 在这里是包含式条件：A 或 B 任一为真就返回 `true`，两者同时为真也返回 `true`。

### Step 2：reply 类型进入 command

源码：[commands.rs](../../crates/codegen/xai-grok-shell/src/session/commands.rs)

```rust
IsBusy {
    respond_to: oneshot::Sender<bool>,
},
```

`oneshot::Sender<bool>` 被放入 command。Actor 处理 command 后，可以通过这个 sender 发送一个布尔值。

推荐表达：

```text
The command embeds a oneshot sender for the Boolean reply.
```

`embed` 比 `has` 更清楚地表达 reply sender 随 command 一起移动。

### Step 3：Handle 发送并等待

源码：[handle.rs](../../crates/codegen/xai-grok-shell/src/session/handle.rs)

```rust
pub async fn is_busy(&self) -> bool {
    let (tx, rx) = oneshot::channel();
    if self
        .cmd_tx
        .send(SessionCommand::IsBusy { respond_to: tx })
        .is_err()
    {
        return true;
    }
    rx.await.unwrap_or(true)
}
```

动作顺序：

1. `create` a oneshot channel;
2. `embed` `tx` in the command;
3. `send` the command through `cmd_tx`;
4. `await` the reply on `rx`;
5. `fall back to true` if either communication step fails.

### Step 4：Actor 计算并回复

源码：[run_loop.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)

```rust
SessionCommand::IsBusy { respond_to } => {
    let busy = {
        let state = session.state.lock().await;
        state_is_busy(&state)
    };
    let _ = respond_to.send(busy);
}
```

英文解释：

```text
When the actor receives `IsBusy`, it locks the session state and evaluates
`state_is_busy`. It releases the lock after leaving the inner block, then
sends the Boolean result through the oneshot sender.
```

不要写 `The actor returns the value from the function`。这个 branch 通过 channel 回复，边界是 `send`，不是普通函数 `return`。

## 5. 为什么 fallback 是 `true`

`is_busy` 服务于 leader 的 idle-unload decision。无法联系 Actor 时存在两种策略：

```text
false -> assume idle -> session may be unloaded
true  -> assume busy -> keep session resident
```

选择 `true` 会多保留一个可能已经空闲的 session，但避免错误卸载仍有工作的 session，因此是 conservative fallback。

推荐表达：

```text
If the command channel is closed or the actor drops the reply sender, the
method returns `true`. This conservative fallback may keep an idle session
resident, but it avoids unloading a session that could still have work in
flight.
```

## 6. 完整参考解释

先自己写，再对照：

```text
`SessionHandle` is a clonable proxy for interacting with a session actor. The
actor owns the mutable session state, while callers use handles to send typed
`SessionCommand` messages through an MPSC channel. For a query such as
`is_busy`, the handle creates a oneshot channel and embeds the reply sender in
the command. The actor checks whether a turn is running or an input is queued,
then sends the result back through the oneshot sender. If either communication
path fails, the handle falls back to `true` so that the leader does not unload
a potentially busy session.
```

这段答案约 90 词，已经足以做 code review 或学习复述。没有必要为了显得高级而使用更长的句子。

## 7. 分层练习

### A. 词组填空

从以下词组中选择：

```text
owns the state
send a command
reply through
work in flight
falls back to
```

1. The actor ______, while the handle provides access to it.
2. Callers ______ through the MPSC channel.
3. The actor can ______ a oneshot sender.
4. A running turn counts as ______.
5. The method ______ `true` when the actor is unreachable.

### B. 判断并改错

1. `Cloning SessionHandle creates an independent session actor.`
2. `The handle directly reads the actor's running_task field.`
3. `The actor returns the is_busy result through a normal function call.`
4. `Returning true on channel failure is a conservative choice.`

前三句需要改写；第四句补充原因。

### C. 回答问题

每题最多 3 句：

1. Why is `SessionHandle` clonable?
2. What is the difference between the MPSC channel and the oneshot channel here?
3. What does a failed MPSC `send` imply?
4. Why does `is_busy` return `true` when the reply is dropped?

### D. 闭卷复述

合上本页，根据下图口头讲 60--90 秒：

```text
caller -> handle -> MPSC -> actor -> state check -> oneshot -> caller
```

必须使用以下五个词组：

```text
clonable proxy
own the state
send through
reply through
conservative fallback
```

## 8. 参考答案

### A

1. `owns the state`
2. `send a command`
3. `reply through`
4. `work in flight`
5. `falls back to`

### B

```text
1. Cloning `SessionHandle` creates another handle to the same session actor.
2. The handle asks the actor by sending `SessionCommand::IsBusy`.
3. The actor sends the result through the oneshot reply channel.
4. Returning `true` is conservative because it prevents a potentially busy
   session from being unloaded.
```

### C 要点

1. 多个调用方或 task 需要共享操作入口；clone sender 不复制 Actor 状态。
2. MPSC 携带发给 Actor 的 commands；oneshot 携带某次 query 的单个 reply。
3. receiving side 已关闭，通常表示 Actor loop 已退出或不可达。
4. `true` 让 leader 保留 session，避免 false negative 导致误卸载。

## 9. 本课完成标准

- 不看材料，能画出 request/reply 图；
- 能用英文区分 handle 与 actor；
- 能解释 `send`、`await` 和普通函数 `return` 的边界差异；
- 能用 80--120 词复述 `is_busy`；
- 第二天仍能闭卷使用本课五个核心词组。

完成后进入 [03-guided-practice.md](./03-guided-practice.md) 的 Level 3，继续学习 Tool stream invariant 和 cancellation safety。
