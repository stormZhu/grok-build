# 7. tracing 日志宏

> 记录日志的工程语境、测试与任务传播见 [17. 测试](./17-testing.md) 和 [13. async、任务与 `Send`](./13-async-runtime-tasks.md)。

## 7.1 基本用法

```rust
use tracing;

// 不同级别的日志
tracing::trace!("最详细的调试信息");
tracing::debug!("调试信息");
tracing::info!("一般信息");
tracing::warn!("警告");
tracing::error!("错误");
```

## 7.2 带 target 的日志

项目中使用 `target` 参数将日志分流到不同的日志文件：

```rust
// 普通日志
tracing::debug!("MEMORY_DREAM_CHECK: timer fired");

// 分流到 memory 专用日志文件
tracing::info!(
    target: xai_grok_telemetry::memory_log::TARGET,
    "MEMORY_IDLE_FLUSH: timer fired (conversation {last_len} → {current_len})"
);
```

## 7.3 span 与任务上下文

事件是一次日志记录；span 表示一段带上下文的工作。对请求、工具调用或后台任务建立 span，字段会自动附着到 span 内的事件。`tokio::spawn` 创建新任务时需确认 tracing 上下文是否被传播；项目的 [`xai-tracing`](../../crates/common/xai-tracing/src/tokio.rs) 提供了相应辅助工具。

```rust
let span = tracing::info_span!("tool_call", tool = %name);
async move { run_tool().await }.instrument(span).await;
```

字段优先使用结构化形式：`path = %path.display()` 使用 Display，`error = ?err` 使用 Debug。避免把可查询字段拼进长字符串，也不要记录密钥、令牌或完整用户敏感内容。

## 7.4 结构化字段

```rust
// 用 key=value 语法插入结构化字段，可以被日志系统解析
tracing::info!(
    target: xai_grok_telemetry::memory_log::TARGET,
    "MEMORY_IDLE_FLUSH: skipped, no new messages since last flush (len={current_len})"
);
// 输出: "MEMORY_IDLE_FLUSH: skipped, no new messages since last flush (len=42)"
```
