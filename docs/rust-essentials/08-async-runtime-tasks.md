# 8. async、Future、task 与 `Send`

Rust async 是协作式并发：`async` 把一段控制流编译成 Future 状态机，executor 反复 poll 它；Future 无法继续时返回 `Pending` 并安排将来被唤醒。它不是“自动创建线程”，也不会自动并行、取消或管理子任务生命周期。

读本仓库异步代码时，需要同时追踪四张图：普通函数调用、Future 的 `.await`、spawn 出的 task、channel/取消信号。只追函数名会漏掉真正的控制流边。

## 调用 async fn 只构造 Future

```rust
async fn fetch() -> Result<Data, Error> {
    let response = client.get(url).await?;
    parse(response)
}

let future = fetch(); // 构造 Future；函数体尚未被 executor 推进
let data = future.await?;
```

概念上，`async fn fetch() -> T` 接近：

```rust
fn fetch() -> impl Future<Output = T>
```

Future 必须被 `.await`、spawn 或由其他组合器 poll 才会推进。创建后立即丢弃通常什么也没执行；编译器会对未使用 Future 给出 `must_use` warning。

参数表达式在调用时求值，但 async 函数体的副作用在 Future 被 poll 后发生。不要依赖“调用了 async fn，所以日志/发送已经执行”。

## poll、Pending 与 Waker

Future 的核心 trait 近似：

```rust
trait Future {
    type Output;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output>;
}
```

- `Ready(value)`：Future 完成并产出一次 Output。
- `Pending`：现在无法继续；Future 应确保资源就绪后调用 waker。
- executor 不应无条件忙轮询 Pending Future，否则会浪费 CPU。
- Future 完成后通常不应再次 poll，除非具体实现另有保证。

`.await` 的直觉是：“poll 子 Future；若 Pending，保存当前状态并把控制权还给 executor；将来从这里继续。”局部变量若在 await 后仍要用，会成为状态机字段。

## `.await` 是潜在重入点

```rust
state.phase = Phase::Loading;
let data = load().await;
state.phase = Phase::Ready;
```

在 `load().await` 期间，同一线程可运行其他 task；若 `state` 通过共享机制暴露，其他 task 可能观察到 Loading。每个 await 都要问：

- await 前的不变量是否暂时暴露？
- 哪些借用、锁 guard 和临时值跨 await 存活？
- Future 被取消并在此 drop 时，状态停在哪里？
- 恢复时依赖的数据可能被其他 task 改变吗？

这也是为什么 async 函数不能被当作“同步函数加几个 await”。

## 并发不等于并行

```rust
let a = read_a().await;
let b = read_b().await;
```

这是串行等待。若两者独立：

```rust
let (a, b) = tokio::join!(read_a(), read_b());
```

`join!` 在**当前 task** 中并发 poll 两个 Future，不创建独立 task，也不要求它们 `'static`。一个分支持续做 CPU 工作不 yield，仍会拖住另一个。

```rust
let a = tokio::spawn(read_a_owned());
let b = tokio::spawn(read_b_owned());
let (a, b) = tokio::try_join!(a, b)?;
```

spawn 创建可独立调度的 task，增加 `Send + 'static`、JoinError 和生命周期管理。只有需要独立调度、后台存活或并行执行时才应 spawn；“想同时 await”不必然需要 task。

并行还取决于 runtime worker 数和工作类型。单线程 LocalSet 上多个 task 只有并发，没有多核并行。

## `tokio::spawn` 的契约

```rust
let handle = tokio::spawn(async move {
    fetch().await
});
let value = handle.await??;
```

通常要求：

```text
Future: Send + 'static
Future::Output: Send + 'static
```

- `Send`：task 挂起后可能在另一个 worker 恢复。
- `'static`：Future 不能借用创建者栈上会提前失效的数据；它并非永久存活。
- `async move`：把捕获值移动进 Future，使其拥有数据；不自动让非 Send 值变 Send。

`handle.await` 外层错误是 `JoinError`（task panic 或被取消），内部 `Result` 是业务错误。详见 [04 错误处理](./04-errors.md)。

## Future 为什么不是 Send

编译器检查的是“哪些值跨 await 存在于状态机中”，不是 async 函数内出现过什么类型。

```rust
async fn bad(state: Arc<std::sync::Mutex<State>>) {
    let guard = state.lock().unwrap();
    network_call().await;
    use_state(&guard);
}
```

`MutexGuard` 跨 await，可能不实现 Send；即使实现，持锁等待网络也通常不好。缩短作用域：

```rust
let snapshot = {
    let guard = state.lock().unwrap();
    guard.snapshot()
};
network_call(snapshot).await;
```

常见非 Send 来源：`Rc`、`RefCell` 借用 guard、某些锁 guard、裸指针/FFI handle、`dyn Trait` 缺少 `+ Send`、借用包含非 Sync 类型。

诊断会给出 “value is used across an await” 和 `tokio::spawn` 引入 bound 的位置。使用 [编译器错误地图](./23-compiler-error-atlas.md) 从 bound 来源反向追。

## `spawn_local` 与 LocalSet

```rust
tokio::task::spawn_local(async move {
    use_rc_state(state).await;
});
```

`spawn_local` 允许 `!Send` Future，因为 task 固定在同一线程。但它必须运行在 `LocalSet` 或 local runtime 上下文中，否则运行时 panic。

本仓库 SessionActor 使用单线程所有权模型：actor 可持有 `RefCell` 等本地状态，并由 `spawn_local` 启动相关任务。不能为了统一写法把它随意换成 `tokio::spawn`；那会要求整个捕获图满足 Send，且可能破坏“状态只在一个线程访问”的设计。

反方向也一样：如果工作需要跨 worker 并行或 API 明确要求 Send，不能仅为绕过错误改用 `spawn_local`。

详见 [12 spawn_local](./12-spawn-local.md)。

## JoinHandle 的生命周期

Tokio `JoinHandle` drop 后 task 会 detached 并继续运行，不是结构化取消。必须明确所有者：

| 需求 | 常见做法 |
| --- | --- |
| 必须拿到结果 | 保存 handle 并 await |
| 后台运行直到 shutdown | 保存 handle + cancel token，关闭时 cancel 后 await |
| 异常离开作用域应停止 | Abort-on-drop guard 或拥有 task 集合 |
| 不关心结果但必须观测 panic | supervisor/join loop 记录 JoinError |
| 真正允许 detached | 注释说明进程级生命周期与错误观测方式 |

`handle.abort()` 请求取消异步 task；取消实际发生在 task 下一次可被 runtime 取消的调度点，随后 await handle 以确认结束。已经开始的 `spawn_blocking` 任务通常不能靠 abort 中止底层阻塞操作。

本仓库工具分发使用 `AbortOnDrop` 包裹 drainer handle，说明父流程退出时不能留下继续向已关闭 channel 写入的 task。

## JoinSet 与 FuturesUnordered

两者都按完成顺序消费，但抽象层不同：

| 工具 | 内容 | 在哪里执行 | 结果 |
| --- | --- | --- | --- |
| `FuturesUnordered<F>` | 一组 Future | 当前 task poll | `F::Output` |
| `JoinSet<T>` | 一组 spawned Tokio task | runtime 独立调度 | `Result<T, JoinError>` |

```rust
let mut futures = FuturesUnordered::new();
futures.push(operation_a());
while let Some(result) = futures.next().await { ... }
```

这里没有新 task，Future 可受当前 task 的取消直接影响。

```rust
let mut set = JoinSet::new();
set.spawn(async move { operation().await });
while let Some(joined) = set.join_next().await { ... }
```

这里每项是独立 task，必须处理 JoinError、关闭时 abort/drain，以及是否允许部分成功。

仓库 [`tool_calls.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/tool_calls.rs) 用 `FuturesUnordered` 并发驱动工具 Future；[`search_bootstrap.rs`](../../crates/codegen/xai-grok-shell/src/session/storage/search_bootstrap.rs) 用 `JoinSet` 管理索引任务。

## 并发上限与背压

同时创建一万个 Future 不等于高性能。资源可能先耗尽：文件描述符、连接池、内存、API quota、磁盘队列。

```rust
let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT));
let permit = semaphore.acquire().await?;
run_one().await?;
drop(permit);
```

permit 是 RAII guard，作用域结束归还名额。注意何时 acquire：先 spawn 海量等待 permit 的 task 仍会占内存；可在生产任务前限制创建速率。

仓库 search bootstrap 同时使用 `JoinSet`（任务拥有/回收）和 `Semaphore`（资源上限），职责不能互换。

## CPU 与阻塞工作

长 CPU 计算或阻塞系统调用直接运行在 async worker 上，会让同 worker 的其他 Future 无法及时 poll：

```rust
let result = tokio::task::spawn_blocking(move || blocking_library_call()).await??;
```

使用前回答：

- 阻塞操作是否有真正异步 API？
- `spawn_blocking` 并发数是否有上限？
- 超时/取消只能停止等待，还是能停止底层操作？
- 输入输出是否值得跨线程 move？
- CPU 批量工作是否更适合 Rayon/专用线程池？

不要把一个同步函数标成 async 期待它自动 yield。只有 `.await` 到 Pending Future 或显式 `yield_now` 等位置才会让出。

## task 中的 panic

task panic 通常不会直接让整个 Tokio runtime 进程退出；它在 JoinHandle 上表现为 `JoinError`。如果 handle 被 detached 且无人观察，关键后台任务可能静默死亡。

supervisor 应决定：记录、重启、降级还是关闭上层 actor。重启前同样检查操作是否幂等以及状态是否已部分更新。

## tracing 上下文不会凭空完整传播

仓库 [`spawn_traced`](../../crates/common/xai-tracing/src/tokio.rs) 给 spawned Future instrument 当前 span：

```rust
pub fn spawn_traced<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future.instrument(Span::current()))
}
```

spawn 是观测边界。新任务应携带 session/tool/call id 或 span，否则日志时间线难以重建。详见 [17 tracing](./17-tracing.md)。

## Actor 是 task 所有权模型

Actor 并不等于“用了 mpsc”。完整模型是：

```text
一个 task 独占可变状态和 receiver
多个 Handle clone sender
command 按值进入队列
需要结果的 command 携带 oneshot sender
shutdown 关闭入口、取消子任务、flush 并 join
```

好处是大部分状态无需跨线程锁；代价是队列背压、actor 死亡、长命令阻塞队列和 shutdown 都必须设计。

从 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 到 [`SessionCommand`](../../crates/codegen/xai-grok-shell/src/session/commands.rs)，再到 [`run_session`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 是本仓库最重要的 async 阅读路径。

## 阅读 spawn 的固定检查表

遇到一个 `spawn`，逐项写出：

1. 谁创建，为什么不能直接 await？
2. `async move` 捕获哪些 owned 值？每个 clone 是深复制还是句柄复制？
3. task 是 Send 还是 local？对应执行环境在哪里建立？
4. 谁保存 JoinHandle，谁观察 panic/结果？
5. 谁发取消，task 在哪些 await 点观察？
6. shutdown 是否 await task 结束？有无预算？
7. task 操作的资源是否有并发上限？
8. span/correlation id 如何传播？

若回答不了第 4--6 项，这个 task 的生命周期仍未读懂。

## 动手练习

1. 运行 Katas 第 12、13 关和 E0277 compile-fail：[`labs/README.md`](./labs/README.md)。标准线程/channel 不能模拟 Tokio poll，但能隔离 `Send` 与所有权。
2. 运行 [`actor_request_reply`](./labs/async-demos/src/bin/actor_request_reply.rs)，为 actor task、mpsc command 和 oneshot reply 分别标出 owner；再运行 [`spawn_local_rc`](./labs/async-demos/src/bin/spawn_local_rc.rs)，解释 `Rc` 为何没有跨线程。
3. 在 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 找一个 `spawn_local`，列出所有 move 捕获。
4. 比较 [`tool_calls.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/tool_calls.rs) 的 `FuturesUnordered` 与 [`search_bootstrap.rs`](../../crates/codegen/xai-grok-shell/src/session/storage/search_bootstrap.rs) 的 `JoinSet`，解释为何前者未必需要独立 task。
5. 找一个 `spawn_blocking`，写出底层操作在 timeout 后是否仍可能继续。
6. 结合 [09 channel/取消](./09-channels-cancellation-streams.md) 和 [10 select](./10-tokio-select.md) 画出 task 的正常完成、取消、panic、父流程提前退出四条路径。

完成标准：看到 `spawn(async move { ... })` 时，能在阅读函数体前先说清捕获所有权、Send 来源、结果层级和关闭责任。
