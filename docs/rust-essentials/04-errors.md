# 4. 错误处理与恢复边界

## 基本传播

`?` 在 `Ok` 时解包值，在 `Err` 时从当前函数提前返回并执行可用的 `From` 转换。

```rust
fn read_config(path: &std::path::Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("read config {}", path.display()))
}
```

为错误加的是**操作语境**，不是重复底层错误文本。调用者应能从错误链看出失败的资源、阶段和后果。

## 选择错误类型

- 应用边界、命令编排、一次性流程可使用 `anyhow::Result`，重点是传播和上下文。
- library/public trait/可恢复业务分支应定义或复用稳定的具体错误类型，调用方据此匹配或转换。
- `Option` 只表达“缺失是正常结果”；I/O、解析、权限失败不能悄悄压成 `None`。

## 项目中的锚点

- [`ComputerError`](../../crates/codegen/xai-grok-tools/src/computer/types.rs#L15) 与 I/O trait 展示在工具边界转换错误。
- [`execute_with_backoff`](../../crates/codegen/xai-grok-tools/src/retry.rs#L47) 区分操作错误和重试策略，而不是无条件重试。
- [`validate_requirements_layer`](../../crates/codegen/xai-grok-config/src/validation.rs#L186) 展示配置问题的记录与拒绝边界。

### 仓库代码摘录：区分 actor 已关闭与回复丢失

[`LocalTerminalBackend::run`](../../crates/codegen/xai-grok-tools/src/computer/local/terminal.rs#L2452) 把两类通道错误转成不同上下文：

```rust
// 源码节选。先把命令投递给唯一拥有终端状态的 Actor。
self.cmd_tx
    .send(TerminalCommand::Run {
        request,
        reply: reply_tx, // 回复端随命令一起移交给 Actor。
    })
    .await
    .map_err(|_| ComputerError::io("terminal actor shut down"))?;

// 再等待该请求专属的回复；此处失败表示 Actor 已接收但未能答复。
reply_rx.await
    .map_err(|_| ComputerError::io("terminal actor dropped reply channel"))?
```

前者表示无法投递命令，后者表示命令已投递但 Actor 未回复。调用者排障和重试策略可能不同，所以不能合并成笼统的 “channel error”。

### 项目关键代码：保留可恢复的 I/O 分类

[`ComputerError`](../../crates/codegen/xai-grok-tools/src/computer/types.rs#L14) 除了可读消息，还保留可供策略层判断的 `ErrorKind`：

```rust
#[derive(thiserror::Error, Debug, Clone)]
pub enum ComputerError {
    #[error("IO Error: {0}")]
    IOError(String, Option<std::io::ErrorKind>),
    #[error("UnQuoted command")]
    CommandNotQuoted,
}

impl From<std::io::Error> for ComputerError {
    fn from(err: std::io::Error) -> Self {
        // 文本用于诊断，kind 用于调用方判断 NotFound、PermissionDenied 等。
        Self::IOError(err.to_string(), Some(err.kind()))
    }
}
```

## 恢复策略

先分类：可重试的暂态错误、可降级的可选功能、必须拒绝的安全或数据一致性错误。配置、权限和策略相关路径默认应明确 fail-closed；只有在需求明确允许时才记录警告后继续。

## 阅读检查点

沿一个 `?` 回溯到顶层，说明最终由谁向用户显示、记录或转换该错误；如果中途丢失语境，补充位置通常就在最接近资源边界的调用处。
