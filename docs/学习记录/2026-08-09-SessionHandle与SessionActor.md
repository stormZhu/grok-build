# SessionHandle 与 SessionActor 学习记录

- 日期：2026-08-09
- 主题：Rust 结构体可见性、SessionHandle、Actor 模式、SessionActor 的启动与运行机制
- 相关模块：`crates/codegen/xai-grok-shell/src/session`

## 1. `pub struct SessionHandle` 表示什么

相关代码：

- `crates/codegen/xai-grok-shell/src/session/handle.rs:43-48`

```rust
/// Handle for interacting with a session actor.
#[derive(Clone)]
pub struct SessionHandle {
    pub cmd_tx: mpsc::UnboundedSender<SessionCommand>,
```

`pub struct SessionHandle` 声明了一个公开的 Rust 结构体：

- `pub`：`SessionHandle` 类型可以被其他模块通过相应模块路径访问。
- `struct`：定义一个把多个相关字段组合在一起的结构体类型。
- `SessionHandle`：类型名称，遵循 Rust 类型的 UpperCamelCase 命名习惯。
- `{ ... }`：结构体字段列表。

结构体类型公开不代表它的所有字段都具有相同的可见性。字段仍然独立控制可见范围：

- `pub cmd_tx`：能够访问 `SessionHandle` 的外部代码也可以访问该字段。
- `pub(crate) persistence_tx`：仅当前 crate 内可访问。
- 没有 `pub` 的字段：只在定义它的模块及其子模块中可访问。

## 2. `#[derive(Clone)]` 的含义

相关代码：

- `crates/codegen/xai-grok-shell/src/session/handle.rs:46`

```rust
#[derive(Clone)]
```

它让编译器自动为 `SessionHandle` 实现 `Clone` trait，因此可以调用：

```rust
let another_handle = session_handle.clone();
```

这里复制的是操作会话的句柄，而不是创建一个新的会话。多个克隆出来的 `SessionHandle` 通常包含：

- 同一个 MPSC channel 的发送端克隆；
- 指向同一份共享状态的 `Arc`；
- 可克隆的子句柄；
- 某些会话配置快照。

因此，多处代码可以持有不同的 `SessionHandle`，但它们控制的仍是同一个 `SessionActor`。

## 3. 什么是会话 Actor

可以把 Actor 理解为一个拥有自身状态、长期在后台接收消息并执行操作的对象。

在这个项目中，各概念可以对应为：

| 概念 | 作用 | 类比 |
|---|---|---|
| `SessionActor` | 真正保存并管理会话状态的执行实体 | 厨房里的厨师 |
| `SessionHandle` | 外部操作会话的代理或遥控器 | 前台点餐器 |
| `SessionCommand` | Handle 发给 Actor 的操作指令 | 订单 |
| MPSC channel | 命令从 Handle 到 Actor 的传输通道 | 订单传送带 |
| oneshot channel | Actor 向某次调用返回一个结果 | 一次性取餐窗口 |

项目中的 `SessionActor` 定义在：

- `crates/codegen/xai-grok-shell/src/session/acp_session.rs:599`

它持有一个会话运行所需的真实状态和能力，例如：

- 会话信息；
- 聊天记录和 ChatState 句柄；
- 当前运行任务与待处理输入；
- 工具上下文；
- 模型、采样与认证配置；
- MCP 状态；
- 权限、通知、持久化和反馈能力。

所以 `SessionActor` 是会话在内存中的真正执行实体，而 `SessionHandle` 是可以跨模块、跨任务传递的操作入口。

## 4. SessionCommand 是 Actor 的消息协议

相关代码：

- `crates/codegen/xai-grok-shell/src/session/commands.rs:191`

`SessionCommand` 是一个枚举，每个枚举变体表示 Actor 支持的一类操作，例如：

```rust
pub enum SessionCommand {
    Initialize { /* ... */ },
    Prompt { /* ... */ },
    IsBusy { /* ... */ },
    SideQuestion { /* ... */ },
    // ...
}
```

其中：

- `Initialize`：初始化会话；
- `Prompt`：提交用户输入；
- `IsBusy`：查询会话是否有正在运行或排队的工作；
- `SideQuestion`：在不打断当前 Turn 的情况下处理旁路问题；
- 还有取消、关闭、MCP、模型切换等其他命令。

这组枚举变体共同构成了外部代码驱动 SessionActor 的消息协议。

## 5. Handle 与 Actor 如何连接

创建 SessionActor 时会建立一个无界 MPSC channel：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:331`

```rust
let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
```

两端的职责如下：

- `cmd_tx` 放入 `SessionHandle`，供外部发送 `SessionCommand`；
- `cmd_rx` 交给 `run_session`，供 SessionActor 主循环接收命令。

流程如下：

```text
外部调用方
    │
    ▼
SessionHandle
    │ cmd_tx.send(SessionCommand)
    ▼
MPSC channel
    │ cmd_rx.recv()
    ▼
SessionActor / run_session
```

MPSC 是 Multiple Producer, Single Consumer：

- Multiple Producer：多个 `SessionHandle` 克隆可以发送消息；
- Single Consumer：该会话的 Actor 主循环是命令的单一接收方。

这种设计让不同模块和异步任务能够并发提交命令，同时由 Actor 统一协调会话状态。

## 6. 请求—响应如何通过 oneshot 完成

以查询会话是否繁忙为例：

- `crates/codegen/xai-grok-shell/src/session/handle.rs:249-265`

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

执行过程：

1. `oneshot::channel()` 创建一次性回复通道 `(tx, rx)`。
2. 调用方把 `tx` 放入 `SessionCommand::IsBusy`。
3. 命令通过 `cmd_tx` 发送给 Actor。
4. Actor 检查自己的状态，然后通过 `respond_to.send(busy)` 回复。
5. 调用方通过 `rx.await` 等待并取得结果。

Actor 端的处理位于：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs:1258-1266`

```rust
SessionCommand::IsBusy { respond_to } => {
    let busy = {
        let state = session.state.lock().await;
        state_is_busy(&state)
    };
    let _ = respond_to.send(busy);
}
```

如果命令发送失败，或者 Actor 在回复前退出，`is_busy()` 保守地返回 `true`。这样 leader 不会因为一次查询失败而错误卸载仍可能有工作的会话。

## 7. SessionActor 在什么时候启动

SessionActor 在创建或加载会话时启动，不是等到第一条 Prompt 到来才启动。

正常会话的主要调用位置：

- `crates/codegen/xai-grok-shell/src/agent/mvp_agent/agent_ops.rs:4656`

子 Agent 创建自己的会话时也会调用同一套启动机制：

- `crates/codegen/xai-grok-shell/src/agent/subagent/handle_request.rs:995`

整体启动链路：

```text
客户端创建或加载会话
        │
        ▼
MvpAgent 准备会话配置
        │
        ▼
spawn_session_on_thread()
        │
        ├─ 创建会话专用 OS 线程
        ├─ 创建 current-thread Tokio runtime
        ├─ 创建 LocalSet
        ├─ 调用 spawn_session_actor()
        ├─ 构造 SessionActor
        ├─ spawn_local(run_session(...))
        └─ 向外部返回 SessionHandle
```

### 7.1 创建会话专用线程

入口定义在：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:2168`

线程创建位置：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:2291-2297`

```rust
let sid = session_info.id.0.to_string();
let thread_name = format!("ses-{}", &sid[..sid.len().min(8)]);
const SESSION_THREAD_STACK_SIZE: usize = 8 * 1024 * 1024;
let join_handle = std::thread::Builder::new()
    .name(thread_name)
    .stack_size(SESSION_THREAD_STACK_SIZE)
    .spawn(move || {
        // ...
    });
```

因此，每个活跃会话拥有一个专用 OS 线程，线程名使用 `ses-` 加会话 ID 前八位。

### 7.2 在线程中创建 Tokio runtime 和 LocalSet

相关位置：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:2319-2331`

会话线程里创建一个 current-thread Tokio runtime，并通过 `LocalSet` 运行本地异步任务：

```text
会话专用 OS 线程
└── current-thread Tokio runtime
    └── LocalSet
        ├── SessionActor 主循环
        ├── 当前模型 Turn
        ├── sampler 事件处理任务
        ├── MCP 相关任务
        └── 其他会话局部异步任务
```

这样设计的重要原因是 `SessionActor` 包含 `RefCell` 等 `!Send` 数据。它在会话线程中构造，始终留在该线程上，不跨线程移动。

### 7.3 构造 SessionActor 并启动主循环

在线程内调用 `spawn_session_actor(...)`：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:2340-2342`

完成会话、工具、历史记录、采样器、MCP、权限等初始化后，通过以下代码启动主循环：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs:2073-2089`

```rust
tokio::task::spawn_local(async move {
    xai_grok_telemetry::session_ctx::with_session_ctx(
        telemetry_ctx,
        run_session(
            session,
            cmd_rx,
            chat_state_event_rx,
            event_rx,
            fs_notify_config,
            codebase_indexes,
            index_root_for_session,
            fs_watch_caps,
        ),
    )
    .await;
    let _ = session_done_tx.send(());
});
```

需要区分两个时间点：

- SessionActor 被创建：`session` 构造完成时；
- SessionActor 开始运行：`run_session(...)` 被 `spawn_local` 调度时。

Actor 启动后不会立刻调用模型，而是先等待 Prompt 或其他命令。

## 8. Actor 消息处理是否串行

是的，Actor 主循环对关键事件和命令的分派是串行协调的。

主循环位于：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs:308-310`

```rust
loop {
    tokio::select! {
        biased;
        // 多个事件分支
    }
}
```

它会同时等待多个事件源：

- `cmd_rx`：`SessionHandle` 发来的 `SessionCommand`；
- `completion_rx`：当前模型 Turn 的完成结果；
- `event_rx`：会话通知事件；
- `chat_state_event_rx`：ChatStateActor 事件；
- 内存刷新、dream check 等定时器。

`tokio::select!` 每次选择一个已经就绪的分支执行。主循环在某一时刻只执行一个分支，所以 Actor 对关键状态的协调是串行的。

这里的准确说法是：

> 状态协调和事件分派是串行的，但所有耗时业务工作并不都在主循环中串行跑完。

## 9. Actor 正在处理模型任务时为什么仍能响应 IsBusy

如果 Actor 在 `SessionCommand::Prompt` 分支里直接 `.await` 整个模型 Turn，那么主循环确实无法响应 `IsBusy`、`Cancel` 或新 Prompt。

这个项目没有这样做。收到 Prompt 后，主循环主要完成：

1. Prompt 准入检查；
2. 将 Prompt 放进 `pending_inputs`；
3. 如果当前空闲，将队首 Prompt 提升为 `running_task`；
4. 将真正的模型 Turn 启动为独立本地异步任务；
5. 主循环返回 `tokio::select!`，继续等待其他事件。

Prompt 命令分支位于：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs:597-690`

其中先执行 `queue_input(...)`，再调用：

```rust
SessionActor::maybe_start_running_task(
    session.clone(),
    completion_tx.clone(),
).await;
```

`maybe_start_running_task()` 将队首输入提升为正在运行的任务：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/notification_drain.rs:115-123`
- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/notification_drain.rs:304-320`

核心状态更新是：

```rust
state.running_task = Some(AgentTask::new_prompt(/* ... */));
```

`AgentTask::new_prompt()` 使用 `spawn_local` 运行真正的 Turn：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/tasks_cancel.rs:88-106`

```rust
handle: tokio::task::spawn_local(async move {
    run_task(
        session.clone(),
        // ...
    )
    .await
})
.abort_handle(),
```

因此同一个 LocalSet 中至少存在两个相互独立调度的异步任务：

```text
LocalSet
├── run_session()：Actor 主循环，继续接收命令
└── run_task()：执行模型调用和工具调用
```

模型 Turn 运行期间，主循环仍然可以接收并处理 `IsBusy`、`Cancel`、新 Prompt、Interject 和 Shutdown 等命令。

## 10. 单线程为什么可以同时处理主循环和模型任务

这里需要区分：

1. OS 线程；
2. Tokio 异步任务；
3. Actor 消息处理。

会话使用一个专用 OS 线程，但该线程上的 Tokio runtime 可以管理多个异步任务。它们不是 CPU 级并行，而是协作式并发。

当 `run_task()` 执行到异步等待点，例如：

```rust
network_request.await;
channel.recv().await;
sleep.await;
```

当前任务会暂停并把执行权还给 Tokio。Tokio 随后可以轮询 `run_session()`，让 Actor 处理新命令。

典型时间线：

```text
主循环：收到 Prompt
主循环：spawn_local(run_task)
主循环：回到 select! 等待事件

run_task：发起模型网络请求
run_task：在 await 处让出执行权

主循环：收到 IsBusy
主循环：检查 running_task
主循环：通过 oneshot 返回 true

run_task：模型数据就绪后继续执行
```

所以：

> 单线程不代表只能有一个异步任务处于存活状态；它表示这些任务不会在这个线程上同时执行 CPU 指令。

## 11. 模型任务完成后如何通知 Actor

模型任务结束后，通过 `completion_tx` 将 `(prompt_id, result)` 发送给 Actor 主循环。主循环监听 `completion_rx`：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs:444-502`

收到完成事件后，主循环会：

1. 刷新缓冲的 Turn 增量；
2. 调用 `handle_completion` 处理结果；
3. 更新 Goal 等会话状态；
4. 清理本轮运行状态；
5. 检查 `pending_inputs`；
6. 如果队列中还有 Prompt，则启动下一轮；
7. 如果没有工作，则发布会话空闲事件。

完整闭环：

```text
Actor 主循环
    │
    ├─ 收到 Prompt
    ├─ 设置 running_task
    ├─ spawn_local(run_task)
    │               │
    │               ├─ 调用模型
    │               ├─ 执行工具
    │               └─ completion_tx.send(result)
    │
    ├─ 期间继续处理 IsBusy、Cancel、新 Prompt
    │
    └─ 从 completion_rx 收到完成结果并收尾
```

## 12. 为什么查询 IsBusy 也选择发消息

`is_busy()` 没有直接读取一个公开的原子布尔值，而是向 Actor 发消息，主要原因是 Actor 才是以下状态的权威协调者：

```text
running_task.is_some() || !pending_inputs.is_empty()
```

通过 Actor 消息查询有几个特点：

### 12.1 保持命令顺序语义

如果同一个发送端先发送 Prompt，随后发送 IsBusy：

```text
Prompt → IsBusy
```

MPSC channel 保持该发送端的发送顺序。Actor 会先登记 Prompt，再处理 IsBusy，因此查询能观察到前一条命令造成的状态变化。

如果外部直接读取一个独立原子布尔值，就必须额外保证该布尔值与 `running_task`、`pending_inputs` 及命令队列始终同步。

### 12.2 避免复制一套状态

繁忙状态由 `running_task` 和 `pending_inputs` 共同决定。通过消息进入 Actor 读取，可以避免再维护一份可能失真的忙碌标记。

### 12.3 查询失败时采用保守策略

如果 Actor 不可达，`is_busy()` 返回 `true`。其目标不是提供严格实时监控，而是服务于 leader 的空闲卸载判断：查询失败时宁可保留会话，也不错误卸载它。

## 13. IsBusy 是否一定立即返回

不一定。

Actor 的主循环虽然不会等待整个模型 Turn，但每个命令分支本身仍可能包含 `.await`。在某个分支完成并回到 `tokio::select!` 之前，新到达的 `IsBusy` 只能在 `cmd_rx` 中排队。

例如 Prompt 分支中包含：

- `ensure_prefix_ready().await`；
- `queue_input(...).await`；
- 状态锁获取；
- 某些取消和入队处理。

设计原则通常是：

- 较短的状态协调操作：在 Actor 主循环中直接执行或等待；
- 模型调用、长期等待用户审批等开放式长任务：通过 `spawn_local` 分离出去；
- 后台任务结束：通过 channel 把结果送回主循环。

恢复 Plan 审批就是一个明确例子：

- `crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs:573-589`

代码使用 `spawn_local` 等待用户决策，避免长时间阻塞 Actor 命令循环。

因此 `IsBusy` 的语义是异步查询，而不是无等待、硬实时读取。

## 14. 协作式调度的限制

Tokio current-thread runtime 使用协作式调度。一个异步任务只有在以下情况下，其他任务才有机会运行：

- 到达未就绪的 `.await`；
- 主动让出执行权；
- 当前任务完成。

如果某个本地任务执行长时间纯 CPU 循环且没有 `.await`，例如：

```rust
loop {
    expensive_cpu_work();
}
```

它会霸占会话线程。即使模型 Turn 已经通过 `spawn_local` 与主循环分开，Actor 主循环此时仍不能及时响应。

因此这种架构要求：

- 网络和 I/O 使用异步 API；
- 长异步任务要自然到达 `.await`；
- CPU 密集型工作应交给专用线程池或阻塞任务机制；
- 避免在 LocalSet 上执行长时间不让权的同步代码。

## 15. 最终心智模型

可以用下面的结构理解整个会话：

```text
MvpAgent / 外部模块
    │
    ├─ SessionHandle clone A
    ├─ SessionHandle clone B
    └─ SessionHandle clone C
             │
             │ SessionCommand
             ▼
        cmd_tx / cmd_rx
             │
             ▼
会话专用 OS 线程
└── Tokio current-thread runtime + LocalSet
    ├── run_session()
    │   ├─ 串行协调命令和关键状态
    │   ├─ 处理 IsBusy、Cancel、Prompt 等命令
    │   └─ 监听 completion_rx 和其他事件源
    │
    └── run_task()
        ├─ 执行一轮模型调用
        ├─ 执行工具调用
        └─ 完成后通过 completion_tx 通知主循环
```

最简结论：

- `SessionActor`：真正拥有和协调会话状态的后台执行实体；
- `SessionHandle`：可克隆、可传递的会话操作代理；
- `SessionCommand`：Handle 与 Actor 之间的命令协议；
- `cmd_tx/cmd_rx`：多发送者、单接收者的命令通道；
- Actor 在创建或加载会话时启动；
- Actor 的关键状态协调是串行的；
- 模型 Turn 被拆成独立的本地异步任务，不会持续占住命令循环；
- `IsBusy` 通过消息查询权威状态，可能短暂排队，但模型生成期间通常仍能得到响应；
- 整体是单线程上的协作式并发，不是多线程并行。
