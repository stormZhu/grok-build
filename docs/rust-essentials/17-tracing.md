# 17. `tracing`：事件、span 与异步上下文

`tracing` 不是把字符串打印到 stderr 的另一套宏。它产生结构化 event 和 span；subscriber/layer 决定过滤、格式化、写文件或导出 telemetry。读日志调用点时要同时追踪控制流、字段契约和 subscriber 配置。

## event、span、subscriber 三层

```text
业务代码
  -> event：某个时刻发生了什么
  -> span：一段工作及其上下文字段
       -> Subscriber / Layer：是否启用、如何记录、写到哪里
```

一个 event：

```rust
tracing::info!(
    session_id = %session_id,
    turn = turn_number,
    cached = cache_hit,
    "prompt completed"
);
```

一个 span：

```rust
let span = tracing::info_span!(
    "tool_call",
    tool = %tool_name,
    call_id = %call_id,
);

run_tool().instrument(span).await
```

span 有进入/退出和嵌套关系，event 是 span 生命周期中的一个点。只有创建 span 不代表下游 async 工作自动在其中执行，必须正确 enter/instrument。

## level 与 target

常用 level 从细到严重为 `TRACE`、`DEBUG`、`INFO`、`WARN`、`ERROR`。level 表示事件重要性，不是输出目的地。

`target` 默认是 Rust module path，也可显式指定：

```rust
tracing::info!(
    target: xai_grok_telemetry::memory_log::TARGET,
    session_id = %session.id,
    current_len,
    "memory idle flush started"
);
```

显式 target 只是路由标签。是否进入专用文件由 layer/filter 决定，不是宏自己打开文件。仓库 [`memory_log.rs`](../../crates/codegen/xai-grok-telemetry/src/memory_log.rs) 用 `EnvFilter` 只接收 `xai_memory=trace`，再用 fmt layer 写入 `memory.log`。

读到 target 时向两边追：

1. 哪些调用点发出这个 target。
2. 哪个 layer/filter 消费它。
3. feature/env 是否让 layer 实际安装。
4. writer guard 是否被保留到进程结束。

## 结构化字段语法

`tracing` 字段不是只能拼进 message 的文本：

```rust
tracing::warn!(
    operation = "config_reload",
    path = %path.display(), // Display
    error = ?error,        // Debug
    retries,               // 等价于 retries = retries
    "reload failed"
);
```

常见形式：

| 语法 | 记录方式 |
| --- | --- |
| `field = value` | 记录 tracing Value |
| `field = %value` | 使用 `Display` |
| `field = ?value` | 使用 `Debug` |
| `field` | 同名局部变量简写 |
| `field = tracing::field::Empty` | span 创建时保留字段，之后 `record` |
| `parent: &span` | 显式指定 parent |
| `parent: None` | 创建 root event/span |

字段名应稳定、低基数且可查询。不要把 session id、结果码、耗时都埋进一条格式字符串；也不要把任意用户文本当作字段名。

`Debug` 可能打印结构体的所有字段，不自动脱敏。Serde 的 `#[serde(skip)]` 也不影响 Debug。

## 延迟记录 span 字段

某些字段只有工作完成后才知道：

```rust
let span = tracing::info_span!(
    "request",
    outcome = tracing::field::Empty,
    elapsed_ms = tracing::field::Empty,
);

async {
    let started = Instant::now();
    let result = call().await;
    Span::current().record("elapsed_ms", started.elapsed().as_millis() as u64);
    Span::current().record(
        "outcome",
        if result.is_ok() { "ok" } else { "error" },
    );
    result
}
.instrument(span)
.await
```

字段必须在创建 span 时声明；对未声明名称调用 `record` 不会神奇增加新字段。仓库 compaction span 使用这种模式记录最终 outcome 和 token 数。

## `#[tracing::instrument]`

属性宏为函数调用创建 span，并可自动记录参数：

```rust
#[tracing::instrument(
    name = "session.spawn",
    skip_all,
    fields(
        session_id = %session_info.id.0,
        client_type = ?client_type,
        start_type = if initial_prompt_texts.is_empty() { "new" } else { "resumed" },
    ),
)]
async fn spawn_session_actor(/* many arguments */) -> Result<...> {
    // ...
}
```

这是仓库 [`spawn_session_actor`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs) 的形状。`skip_all` 很重要：该函数参数很多，自动 Debug 既昂贵又可能泄露 credential、prompt 或配置；只白名单记录诊断所需字段。

常见选项还有 `skip(arg)`、`level`、`target`、`parent`、`ret` 和 `err`。启用 `ret`/`err` 前确认返回类型的 Debug/Display 不含 secret 或大 payload。

属性宏只是生成 span/instrument 代码；遇到 trait bound 或 moved value 错误时，可按 [18 宏](./18-macros-and-raii.md) 的方法看展开后的普通 Rust。

## async 中不要长期持有 enter guard

同步代码可以：

```rust
let span = tracing::info_span!("parse");
let _guard = span.enter();
parse_input();
```

但不要让 `Entered` guard 跨 `.await`。future 被暂停后，同一线程可能 poll 其他 task，thread-local 当前 span 会被错误归到这段工作；guard 也可能使 future 不满足需要的 bound。

异步代码用：

```rust
use tracing::Instrument as _;

async move {
    step_one().await;
    step_two().await;
}
.instrument(span)
.await;
```

`Instrument` 在每次 poll future 时进入 span，poll 返回时退出，和异步调度边界一致。

## spawn 后的上下文传播

不要假设 `tokio::spawn` 自动继承当前业务 span。若后台 task 属于当前请求，可显式 instrument：

```rust
let span = tracing::Span::current();
let task = tokio::spawn(async move {
    run_background().await;
}.instrument(span));
```

仓库 [`xai_tracing::spawn_traced`](../../crates/common/xai-tracing/src/tokio.rs) 封装了这个模式：

```rust
pub fn spawn_traced<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    tokio::spawn(future.instrument(Span::current()))
}
```

继承 parent 适合“当前请求的一部分”。长期后台 worker 若跨多个请求，继承创建时的 session/turn span 可能产生错误归属；它应有自己的生命周期 span，并在处理每条 message 时使用 message 携带的 correlation id 或显式 parent。

`spawn_local` 同样要分析 context；线程固定不等于 span 自动正确传播。

## subscriber、layer 与 filter

业务 crate 通常只发 event/span，composition root 安装 subscriber：

```rust
let subscriber = tracing_subscriber::registry()
    .with(console_layer.with_filter(console_filter))
    .with(file_layer.with_filter(file_filter))
    .with(otel_layer);

tracing::subscriber::set_global_default(subscriber)?;
```

概念分工：

- Registry 保存 span 元数据与关系。
- Layer 观察 span/event，可格式化、聚合或导出。
- Filter 决定哪些 callsite/target/level 对某 layer 启用。
- Writer 决定输出目标。
- Appender guard 可能负责后台 flush，过早 drop 会丢日志。

全局 subscriber 通常只能设置一次。测试并行设置全局 subscriber 容易互相干扰；优先使用 `tracing::subscriber::with_default` 或仓库测试 helper 给当前作用域安装 collector。

## 错误日志应保留分类与链

`anyhow`/`thiserror` 错误常有 source chain。只记录 `error = %error` 可能只有顶层 context；只用 `?error` 可能输出过多内部或敏感信息。日志政策应决定：

- 稳定 `error_code`/category 字段。
- 安全的顶层 message。
- 调试层是否记录 source chain。
- 路径、HTTP status、retryable 是否独立字段。
- 用户可见错误与内部 tracing 是否分层。

同一个失败只在负责处理或终止传播的边界记录，避免每层 `?` 前都 error 一次造成重复噪声。错误分层见 [04 错误处理](./04-errors.md)。

## 敏感数据与基数

禁止或谨慎记录：

- token、cookie、Authorization header、私钥。
- 完整 prompt、文件内容、环境变量。
- 未脱敏 URL query。
- 任意错误 Debug 中附带的原始 request/response。
- 把用户输入作为 span/event 名称或动态字段名。

session/request id 常有诊断价值，但在 metrics/remote telemetry 中可能形成高基数或隐私问题；依据具体 sink 的政策选择 hash、采样或不上传。一个 event 可能同时进入本地文件和远程 layer，不能只按本地日志风险判断。

## 性能与可观测性契约

disabled callsite 通常有低开销，但高频循环中的 span/event、复杂 Debug、clone 大 payload 仍需审查。可先：

- 把循环内逐项日志降到 trace，保留聚合计数。
- 用稳定字段记录 count/duration/outcome。
- 避免为了日志提前构造大 String。
- 对昂贵字段先检查 span 是否 disabled，仓库 auth recovery 有相应例子。
- 用采样或专用 target 隔离 firehose 数据。

日志不是测试替代品。关键状态转移应有断言和结果类型，tracing 用于解释运行中的路径。

## 排障阅读法

遇到一条日志时：

1. 用精确 message/field/target 搜调用点。
2. 阅读它前后的 branch、状态更新和 await。
3. 找 span 创建点与 parent，确认字段来自哪个生命周期。
4. 找 subscriber filter，确认部署环境是否实际收集。
5. 检查相邻事件缺失是分支未走、filter 丢弃、task context 丢失还是 appender 未 flush。
6. 用 correlation 字段串起同一请求，不能依赖并发日志的文本顺序。

## 测试什么

可观测性是外部契约时，测试 collector 捕获结构化 event，断言 target/level/关键字段，而不是完整格式化时间戳。还应覆盖：

- secret 不出现在 event 字段/message。
- disabled layer 不创建目标文件。
- appender guard 生命周期允许 flush。
- spawned task 保留或有意切断 parent。
- 同一失败不会在一个边界重复上报。

## 阅读练习

1. 从 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 的 memory target event 跟到 [`memory_log.rs`](../../crates/codegen/xai-grok-telemetry/src/memory_log.rs)，画出 target、filter、writer 和 guard。
2. 阅读 [`spawn_session_actor`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs) 的 `#[instrument]`，解释每个字段为什么使用 `%`、`?` 或表达式，以及为什么 `skip_all`。
3. 找一个 `tracing::field::Empty`，追踪后续 `Span::record` 的所有出口；确认错误/取消路径是否留下可理解的 outcome。
4. 找一个 `tokio::spawn`/`spawn_local`，判断它应继承当前 span、建立 child span，还是故意成为独立 worker。

完成标准：能从一个 event 找到其 span 和 subscriber 路径，正确传播 async task 上下文，并设计可查询、低泄漏、不过度高基数的字段。
