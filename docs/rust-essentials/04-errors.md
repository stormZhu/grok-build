# 4. 错误处理、错误链与恢复边界

Rust 用 `Result<T, E>` 把失败写进函数契约。读仓库代码时不能只问“错误有没有返回”，还要问：谁需要匹配它、哪些语境必须保留、是否能重试、跨 channel/task/wire 后会变成什么，以及最终由谁展示。

## `?` 的准确含义

```rust
let config = load(path)?;
```

概念上近似：

```rust
let config = match load(path) {
    Ok(value) => value,
    Err(error) => return Err(From::from(error)),
};
```

因此 `?` 同时做两件事：

1. `Ok` 时取出内部值。
2. `Err` 时从**当前函数**提前返回，并尝试通过 `From`/`Into` 转成当前函数的错误类型。

它不会自动记录日志、重试、回滚，也不会把错误发给用户。那些动作发生在更上层边界。

`?` 也可用于 `Option`，此时遇到 `None` 从返回 `Option` 的当前函数提前返回。不要用 `.ok()?` 随意把 `Result` 错误压成缺失；这会丢掉失败原因。

## 先画错误层级

一次工具调用可能经过：

```text
std::io::Error / serde_json::Error
        |
        | From / map_err（保留机器分类）
        v
具体 crate 错误，例如 ToolError / AgentBuildError
        |
        | channel / task / timeout（新增传输或调度失败层）
        v
Session / application error
        |
        | 映射、脱敏、序列化
        v
wire error / 用户文本 / 模型可见 detail / developer log
```

每一层只添加本层知道的信息。底层 I/O 不知道“这是恢复 session”；上层知道资源与操作，却不应伪造底层 `ErrorKind`。

## Option 不是“简化版错误”

选择返回类型时先写业务语义：

| 类型 | 合适语义 |
| --- | --- |
| `Option<T>` | 没有值是正常且无需解释的结果 |
| `Result<T, E>` | 操作可能失败，调用者需要原因 |
| `Result<Option<T>, E>` | 操作可能失败；成功时目标也可能不存在 |
| `Option<Result<T, E>>` | 操作本身可选；一旦存在仍可能失败 |

配置键不存在可能是 `None`，配置文件打不开或语法错误应是 `Err`。把后两者转成 `None` 会让默认配置掩盖权限、损坏或安全策略问题。

## 具体错误类型与 `anyhow`

粗略选择：

| 场景 | 优先选择 |
| --- | --- |
| public library、trait、协议或可恢复业务分支 | 稳定的具体错误 enum/struct |
| 应用入口、一次性编排、内部顶层流程 | `anyhow::Result<T>` 加上下文 |
| wire 边界 | 明确可序列化、可兼容、可脱敏的协议错误 |

具体错误允许调用方匹配：

```rust
#[derive(Debug, thiserror::Error)]
enum AgentBuildError {
    #[error("IO error during agent construction: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid configuration: {0}")]
    InvalidConfig(String),
}
```

`#[from]` 生成 `From<std::io::Error>`，所以返回 `AgentBuildError` 的函数可对 I/O Result 使用 `?`。它也建立错误 source 链。

`anyhow::Error` 擅长在应用内部携带任意 source 链，却不应成为需要稳定匹配的公共协议。若调用者要判断 `PermissionDenied`、`Timeout`、`Cancelled`，仅靠格式化字符串很脆弱。

仓库的 [`AgentBuildError`](../../crates/codegen/xai-grok-agent/src/error.rs) 适合观察 `thiserror` 的具名 variant 与 `#[from]`。

## 为错误添加操作语境

```rust
use anyhow::Context;

fn read_config(path: &Path) -> anyhow::Result<String> {
    std::fs::read_to_string(path)
        .with_context(|| format!("read config {}", path.display()))
}
```

好的 context 回答“在做什么、操作哪个资源”：

```text
read config /project/.grok/config.toml
caused by: Permission denied (os error 13)
```

不好的 context 只是重复：

```text
I/O error: Permission denied
caused by: Permission denied
```

`context("static message")` 总是构造给定值；`with_context(|| ...)` 只在错误路径计算动态字符串。不要把 token、完整 prompt、密钥或用户隐私放入错误链，因为上层可能记录它。

## `map_err` 用于改变边界语义

```rust
sender.send(command)
    .map_err(|_| SessionError::ActorClosed)?;
```

使用 `map_err` 的理由应是“从底层错误映射到当前层契约”，不是仅为了换一句文本。若源错误对调试仍有价值，保留 source：

```rust
operation().map_err(|source| MyError::Operation { source })?
```

若错误类型刻意不暴露 source（例如跨 wire），在开发日志中记录受控信息，同时向外输出稳定分类和安全 detail。

## 同一个失败有多个受众

[`ToolError`](../../crates/common/xai-tool-runtime/src/error.rs) 明确拆分：

```rust
pub struct ToolError {
    pub kind: ToolErrorKind,       // 机器分类
    pub detail: String,            // 模型/用户可读说明
    #[serde(skip)]
    source: Option<anyhow::Error>, // 开发诊断，不上 wire
    pub details: Option<Value>,    // 受控结构化元数据
}
```

这四项不能用一条字符串替代：

- `kind` 驱动重试、HTTP/wire 映射和 UI 行为。
- `detail` 应具体、可行动，但不能泄露内部敏感信息。
- `source` 保留因果链和调试细节，通过 `#[serde(skip)]` 阻止序列化。
- `details` 放经过设计的 tool id、retry-after 或字段验证报告。

[`From<ToolError> for ToolErrorWire`](../../crates/common/xai-tool-runtime/src/error.rs) 是关键边界：它把内部分类映射成协议 variant，而不是直接序列化任意 anyhow 链。

相关测试在 [`error_conversion.rs`](../../crates/common/xai-tool-runtime/tests/error_conversion.rs)。

## channel 会增加失败层

请求/回复 Actor 常见两次独立失败：

```rust
let (reply_tx, reply_rx) = oneshot::channel();

cmd_tx.send(Command::Run { reply: reply_tx })
    .await
    .map_err(|_| Error::ActorClosed)?;

reply_rx.await
    .map_err(|_| Error::ReplyDropped)?
```

- `send` 失败：接收端已关闭，命令没有进入 actor。
- `reply_rx.await` 失败：命令曾成功入队，但 reply sender 在发送结果前被 drop。
- `reply_rx.await?` 成功取得的值还可能是 `Result<T, DomainError>`，这是业务层失败。

不要把三者都显示为“channel error”。是否能安全重试取决于 actor 是否可能已经产生副作用。

仓库 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 中大量方法使用该形状。比较 `get_model_metadata` 的默认降级和 `kill_background_task` 的显式错误：不同业务语义决定不同关闭策略。

## task 和 timeout 也会嵌套 Result

```rust
let joined = tokio::spawn(async { operation().await }).await;
```

类型大致为：

```text
Result<Result<T, OperationError>, JoinError>
       ^ 业务操作                  ^ task panic/cancel
```

若外层还有 `timeout`，再增加 `Elapsed`。从外向内逐层匹配：

```rust
match tokio::time::timeout(limit, handle).await {
    Err(_elapsed) => { /* 等待超时；handle 是否被 drop/abort 要单独判断 */ }
    Ok(Err(join_error)) => { /* task panic 或被取消 */ }
    Ok(Ok(Err(operation_error))) => { /* 业务失败 */ }
    Ok(Ok(Ok(value))) => { /* 成功 */ }
}
```

`handle.await??` 很短，但只适合两层错误都能正确 `From` 到当前错误且无需分别处理的场景。排障时先展开。

## “已提交但确认丢失”不是普通失败

持久化、支付、远端 mutation 等操作可能出现：操作已生效，但回复丢失。仓库 [`DurableAppendError`](../../crates/codegen/xai-grok-shell/src/session/persistence.rs) 区分：

```rust
enum DurableAppendError {
    NotCommitted(io::Error),
    Committed(io::Error),
    AcknowledgementLost(io::Error),
}
```

如果把它们压成 `io::Error` 后无条件重试，可能重复提交。恢复策略必须知道副作用是否发生、操作是否幂等、是否有 idempotency key 或可查询状态。

## 重试是策略，不是错误处理默认值

重试前回答：

- 错误是暂态还是永久？
- 操作是否幂等？部分成功怎么办？
- 有无最大次数、总时间预算和指数退避？
- 是否尊重 `retry_after`？
- 取消后是否立即停止重试？
- 多个客户端同时重试是否需要 jitter 防止惊群？

典型分类：

| 类别 | 常见策略 |
| --- | --- |
| timeout、临时网络不可用、明确 rate limit | 有预算地重试 |
| unauthorized | 刷新凭证一次，再按分类处理 |
| permission denied、invalid arguments | 不重试，给可行动信息 |
| cancelled | 通常立即传播并清理 |
| committed/ack lost | 先查询或用幂等键，不能盲重试 |
| 数据损坏、协议不变量违反 | fail closed，记录足够诊断 |

## panic、unwrap 与 expect

panic 表示当前执行路径无法继续维护程序不变量，不是普通错误返回。区分：

- 用户输入、网络、文件、权限、配置：通常可预期，应返回错误。
- 程序内部不变量被破坏：可使用 assert/panic，但应有明确证据和测试。
- 测试 setup：`unwrap`/`expect` 可简洁表达“失败即测试失败”。
- 进程入口：可以统一展示后退出，但先考虑清理与日志。

`expect("semaphore is never closed")` 比裸 `unwrap()` 多提供不变量陈述；它仍需靠所有权设计证明 semaphore 确实不会关闭。

不要机械把每个 unwrap 改成 `?`：这会改变函数签名和恢复边界。先判断 panic 是否可达、输入是否外部可控、task panic 会由谁 join。

## 清理依赖所有权，而非“成功路径末尾”

`?` 提前返回时局部变量仍按逆序 drop，因此 RAII guard 是可靠清理机制：临时文件 guard、锁 guard、作用域计数器等不需要在每个 Err 分支重复处理。

异步 task、子进程和远端操作仅靠 Drop 未必能完整收尾：`JoinHandle` drop 不会默认取消 Tokio task；进程可能需要 kill+wait；持久化可能需要 flush ack。应设计显式 shutdown/cancel/join 协议，并用 RAII 作为异常路径兜底。

## 错误测试应证明分类和边界

弱断言：

```rust
assert!(operation().is_err());
```

更有价值：

```rust
let error = operation().unwrap_err();
assert_eq!(error.kind, ToolErrorKind::InvalidArguments);
assert!(error.detail.contains("field name"));
```

根据契约选择验证：

- variant/kind 是否正确。
- source 是否保留且不会越过 wire。
- detail 是否可行动且不含敏感数据。
- retry metadata 是否存在。
- channel 关闭前/后对应不同错误。
- 已提交与未提交失败是否区分。

不要断言整段系统错误文本，OS 和依赖版本可能改变措辞；稳定协议文本除外。

## 阅读一个错误路径的固定方法

1. 写出函数完整返回类型。
2. 对每个 `?` 标注源错误和 `From` 目标。
3. 对每个 `map_err` 写明丢弃/保留了哪些信息。
4. 找最上层匹配或日志位置，确认谁决定重试、降级或展示。
5. 检查跨 task/channel/timeout 后是否多一层错误。
6. 检查 wire/日志是否泄露 source、路径、token 或用户内容。
7. 找测试证明分类，不只证明“失败了”。

## 动手练习

1. 运行 Katas 第 4、9、13 关：[`labs/katas.rs`](./labs/katas.rs)。
2. 阅读 [`ToolError`](../../crates/common/xai-tool-runtime/src/error.rs) 到 `ToolErrorWire` 的转换，为每个字段标注受众。
3. 运行真实错误转换测试：

```sh
cargo test -p xai-tool-runtime --test error_conversion
```

4. 从 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 选一个 oneshot 方法，写出“发送失败、回复丢失、业务错误”三层中实际存在的层级。
5. 遇到编译错误时使用 [编译器错误诊断地图](./23-compiler-error-atlas.md) 的工作表，不以增加 clone/unwrap 作为完成标准。

完成标准：你能沿一条 `?` 链说明最终错误类型、保留的 source、机器分类、重试权和用户/模型可见文本分别由哪一层决定。
