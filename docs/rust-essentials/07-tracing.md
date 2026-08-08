# 7. tracing 日志宏

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

## 7.3 结构化字段

```rust
// 用 key=value 语法插入结构化字段，可以被日志系统解析
tracing::info!(
    target: xai_grok_telemetry::memory_log::TARGET,
    "MEMORY_IDLE_FLUSH: skipped, no new messages since last flush (len={current_len})"
);
// 输出: "MEMORY_IDLE_FLUSH: skipped, no new messages since last flush (len=42)"
```
