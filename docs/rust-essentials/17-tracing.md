# 17. tracing 日志宏

> 记录日志的工程语境、测试与任务传播见 [7. 测试](./07-testing.md) 和 [8. async、任务与 `Send`](./08-async-runtime-tasks.md)。

## 17.1 基本用法

```rust
use tracing;

// 不同级别的日志
tracing::trace!("最详细的调试信息");
tracing::debug!("调试信息");
tracing::info!("一般信息");
tracing::warn!("警告");
tracing::error!("错误");
```

## 17.2 带 target 的日志

项目中使用 `target` 参数将日志分流到不同的日志文件：

```rust
// 源码节选自 run_loop：target 用于把 memory 事件交给专用 subscriber/filter。
tracing::info!(
    target: xai_grok_telemetry::memory_log::TARGET,
    "MEMORY_IDLE_FLUSH: timer fired (conversation {last_len} → {current_len})"
);
```

这条日志在 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L320) 中紧邻原子计数更新和后台 task 创建，排查 flush 时应同时读这段控制流，而不是只搜索日志文本。

## 17.3 span 与任务上下文

事件是一次日志记录；span 表示一段带上下文的工作。对请求、工具调用或后台任务建立 span，字段会自动附着到 span 内的事件。`tokio::spawn` 创建新任务时需确认 tracing 上下文是否被传播；项目的 [`xai-tracing::spawn_traced`](../../crates/common/xai-tracing/src/tokio.rs#L16) 提供了相应辅助工具。

```rust
let span = tracing::info_span!("tool_call", tool = %name);
async move { run_tool().await }.instrument(span).await;
```

字段优先使用结构化形式：`path = %path.display()` 使用 Display，`error = ?err` 使用 Debug。避免把可查询字段拼进长字符串，也不要记录密钥、令牌或完整用户敏感内容。

### 项目关键代码：spawn 时保留当前 span

[`spawn_traced`](../../crates/common/xai-tracing/src/tokio.rs#L16) 在创建后台任务前将当前 span 附着到 future：

```rust
pub fn spawn_traced<F>(future: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    // Instrument 在 poll future 时恢复创建点的 tracing 上下文。
    tokio::spawn(future.instrument(Span::current()))
}
```

这适合需要继承父请求上下文的 task。若后台工作需要自己的字段和生命周期，应显式新建 `info_span!`，而不是只继承父 span。

## 17.4 结构化字段

```rust
// 用 key=value 语法插入结构化字段，可以被日志系统解析
tracing::info!(
    target: xai_grok_telemetry::memory_log::TARGET,
    "MEMORY_IDLE_FLUSH: skipped, no new messages since last flush (len={current_len})"
);
// 输出: "MEMORY_IDLE_FLUSH: skipped, no new messages since last flush (len=42)"
```
