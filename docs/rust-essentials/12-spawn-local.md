# 12. `spawn_local`、`LocalSet` 与单线程 Actor

`spawn_local` 允许调度 `!Send` future，但它不是“较轻量的 spawn”。它要求 task 始终由一个正在运行的 `LocalSet` 驱动，并把线程归属变成架构契约。理解普通 task 的所有权、取消和 JoinHandle 后再读本章；见 [08 async、任务与 `Send`](./08-async-runtime-tasks.md)。

## `Send` 约束从哪里来

Future 每次 `.await` 后可能在另一条 runtime worker 线程继续 poll。`tokio::spawn` 因此要求：

```rust
pub fn spawn<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static;
```

`Send` 不是说 task 会同时在两条线程执行，而是允许 runtime 在暂停点之间移动整个 future。future 在 `.await` 时仍保存的每个捕获值和局部值都会影响它是否 `Send`。

`Rc<T>`、`RefCell<T>` 和许多线程绑定句柄不是 `Send`。若它们确实只属于一条线程，可用：

```rust
tokio::task::spawn_local(async move {
    use_non_send_state().await;
});
```

`spawn_local` 的 future 不要求 `Send`，但仍要求 `'static`：spawn 出去的 task 可能比当前函数活得久，不能借用当前 stack 局部。通常要用 `async move` 捕获 owned 值、`Rc` 或 `Arc` clone。

## runtime、LocalSet 与 task 的三层

```text
OS thread
  -> current-thread Tokio Runtime：I/O driver、timer、普通 runtime context
       -> LocalSet：保证 local task 只在当前线程 poll
            -> spawn_local task：可以持有 Rc/RefCell 等 !Send 状态
```

仅使用 `Builder::new_current_thread()` 不足以让任意位置的 `spawn_local` 工作；必须在 `LocalSet::run_until`/`block_on` 或 Tokio 的 local runtime context 内调用。否则 `spawn_local` 会 panic。

`LocalSet` 也必须持续被驱动。只创建它、向其中 spawn task，却从未 `run_until`/`await`，task 不会自行执行。

## 本仓库的会话线程边界

[`spawn_session_on_thread`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs) 为每个 session 建立专用 OS thread：

```rust
let join_handle = std::thread::Builder::new()
    .name(thread_name)
    .spawn(move || {
        let rt = match build_session_runtime() {
            Ok(rt) => rt,
            Err(error) => {
                let _ = init_tx.send(Err(AgentBuildError::RuntimeBuild(error)));
                return;
            }
        };

        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            // SessionActor 在这条线程内构造、运行并最终销毁。
            let result = spawn_session_actor(/* owned Send inputs */).await;
            // 只把 Send 的 handle/metadata 通过 channel 送回调用线程。
        });
    })?;
```

关键边界不是“整个应用是单线程”，而是：

- 一个 session actor 固定在自己的一条线程和 LocalSet 上。
- `SessionActor` 可包含 `RefCell` 等 `!Sync`/`!Send` 局部状态，因为它本身不跨线程。
- 外部代码通过 `SessionHandle` 和 channel 交互，而不是拿到 actor 引用。
- session 内仍可显式 `tokio::spawn` 一个 `Send` task；这与 `spawn_local` 是不同边界。
- runtime 构建会因 fd/线程资源耗尽而失败，所以 `build_session_runtime()` 返回 `io::Result`，不能 unwrap 成全进程 panic。

这是一种 thread confinement：把复杂可变状态留在 owner 线程，把跨线程表面收缩为消息和可发送 handle。

## 仓库中的 local task

[`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 在 idle flush 分支 clone session 后启动 local task：

```rust
tokio::task::spawn_local({
    let session = session.clone();
    async move {
        if !session.run_memory_flush("interval", None).await {
            tracing::info!("MEMORY_IDLE_FLUSH: skipped");
        }
    }
});
```

这里要分别读懂：

1. `session.clone()` 增加 `Arc` 引用计数，不复制 actor 数据。
2. `async move` 把这个 clone 移进 future，使 task 不借用当前 select 分支的栈帧。
3. `spawn_local` 保证 future 不跨 session 线程，允许它最终触达 actor 的 local-only 状态。
4. 返回的 JoinHandle 没有保存，表示这是 detached task。其错误、取消和 session shutdown 行为必须由内部协议或 LocalSet 生命周期承担。

`Arc` 出现在 local task 中不等于 task 可 `Send`。内部类型若不是 `Send + Sync`，`Arc<T>` 也不会神奇地满足这些 bound；Arc 这里只提供共享所有权。

## 单线程不等于没有并发问题

Local task 不会在同一时刻并行执行 Rust 指令，但它们会在 `.await` 处交错：

```text
task A：检查 state == Idle，然后等待 I/O
task B：把 state 改成 Closed，然后让出执行权
task A：恢复并按旧假设写入
```

所以仍需防范：

- check-then-act 跨 `.await` 失效。
- `RefCell` borrow guard 跨 `.await` 后，另一 task 再 borrow 导致运行时 panic。
- detached task 比 session 资源活得久。
- 一个 local task 做同步阻塞 I/O，卡住整条 session 线程。
- task panic 未被观察，关键清理没有执行。

单线程消除 data race，不消除逻辑 race、重入和饥饿。

## 不要把阻塞工作放进 local task

`std::fs` 大量 I/O、CPU 密集循环、阻塞锁和外部命令等待会阻塞 current-thread runtime，使该 session 的 timer、channel 和其他 task 都无法推进。

可选方向：

- 使用 Tokio 异步 I/O。
- 将有界阻塞工作交给 `spawn_blocking`，并用 owned Send 数据跨边界。
- 把 CPU 工作交给专门线程池，再通过 channel 返回结果。
- 将大循环分块并显式 yield，但这不是 CPU pool 的替代品。

`spawn_blocking` 的闭包必须 `Send + 'static`，因此不能直接把 `Rc<RefCell<_>>` 搬进去。先提取 owned、可发送的输入，返回结果后再在 local task 更新状态。

## task 所有权与 shutdown

`spawn_local` 返回 `JoinHandle<T>`，其基本规则与普通 Tokio task 相同：

- `handle.await` 观察输出或 panic。
- drop handle 会 detach，不会取消 task。
- `handle.abort()` 请求取消；future 在下次调度时被 drop。
- LocalSet/runtime 停止时，未完成 task 会被 drop，不能把这当作优雅 shutdown。

关键后台工作应有 owner：保存 handle，使用 `CancellationToken`/channel 通知停止，最后 await join。fire-and-forget 只适合失败与延迟都不影响调用者契约、且资源生命周期有明确上界的任务。

## 测试 local-only 代码

仓库测试常采用：

```rust
#[tokio::test(flavor = "current_thread")]
async fn local_actor_obeys_contract() {
    let local = tokio::task::LocalSet::new();
    local
        .run_until(async {
            let state = Rc::new(RefCell::new(Vec::new()));
            let child = tokio::task::spawn_local(run_actor(state.clone()));

            // 通过 channel/barrier 观察行为，不用真实 sleep 猜进度。
            request_shutdown().await;
            child.await.expect("local actor should not panic");
        })
        .await;
}
```

`current_thread` 与 `LocalSet` 解决不同问题，测试需同时复现生产边界。异步确定性见 [07 测试](./07-testing.md)。

## `Send` 错误诊断顺序

编译器报 “future cannot be sent between threads safely” 时：

1. 从诊断中的 `await occurs here` 找出跨 await 存活的非 Send 值。
2. 判断它本应缩短作用域、转成 owned snapshot，还是业务上确属线程绑定状态。
3. 若生产调用链是多线程 `spawn`，不要为了通过而换 `spawn_local`，应修正状态边界。
4. 只有整个 owner 明确运行于 LocalSet 时，才选择 local task。
5. 检查调用 `spawn_local` 的所有入口和测试是否都建立 local context。

## 阅读练习

1. 运行 [`spawn_local_rc`](./labs/async-demos/src/bin/spawn_local_rc.rs)，指出 runtime、`LocalSet` 和两个 local task 的嵌套关系，并说明断言为何不依赖 task 顺序。
2. 从 [`spawn_session_on_thread`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs) 画出调用线程、session thread、oneshot init channel、`SessionHandle` 和 `SessionActor` 的所有权方向。
3. 找到 `local.block_on` 结束条件，确认 session 完成后谁让线程退出、谁持有 `SessionThread` 的 join handle。
4. 在 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 选一个 detached `spawn_local`，回答失败如何可见、session shutdown 时是否必须完成、捕获值何时释放。
5. 阅读一个使用 `LocalSet::run_until` 的 session 测试，列出它复现了哪些生产约束，又没有覆盖专用 OS thread 的哪些行为。

完成标准：能从一个 async 调用链判断 future 为什么是或不是 `Send`，说明 local task 由哪个 LocalSet/线程驱动，并追踪其输入、结果、取消、panic 和 shutdown owner。
