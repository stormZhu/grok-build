# 7. Rust 测试、替身与异步确定性

测试不是“跑过一些代码”，而是对可观察契约提供证据。先写要证明的行为、不变量和失败边界，再选择 unit、integration、doctest 或更高层测试。命令退出 0 只有在确认目标、feature、平台和实际 test 数量后才有意义。

## Rust 测试如何编译

### Unit test

```rust
fn parse(input: &str) -> Result<Value, Error> { ... }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_input() { ... }
}
```

unit test 与被测 module 一起编译，可访问 private item。适合纯转换、状态机、边界条件和小型 helper。

### Integration test

```text
package/
  src/lib.rs
  tests/protocol.rs
```

每个 `tests/*.rs` 是独立 crate，只能使用 library 的 public API。它能证明外部调用者真正可用的契约，也会单独链接一个 test binary。

### Doctest

Public doc comment 中的 Rust 代码块可由 `cargo test --doc` 编译/运行。它适合最小 API 用法；依赖复杂 runtime、环境或私有 item 的片段可标 `no_run`/`ignore`，但 `ignore` 也意味着 CI 不再检查其正确性，应谨慎使用。

### Binary/E2E/fixture test

启动真实进程、协议 peer 或持久化目录能覆盖组合行为，但更慢、更容易受平台/环境影响。它不能替代底层纯逻辑测试，而应证明跨 crate/进程的关键路径。

## 测试层级选择

| 要证明的内容 | 首选层级 |
| --- | --- |
| parser、转换、排序、状态转移 | 同 module unit test |
| public trait/type 契约 | integration test |
| Serde wire shape | exact JSON + round-trip + invalid fixture |
| channel 关闭/取消 | Tokio test + 受控 channel barrier |
| 文件系统行为 | tempdir + 真实 FS，或调用方使用 fake trait |
| CLI 参数/输出/退出码 | binary/integration test |
| 平台 API | target CI 或受 cfg 控制的平台测试 |
| 性能回归 | benchmark/指标，不用普通 wall-clock 断言 |

优先用最低成本、最确定且能观察目标契约的层级。不要为了“更真实”让所有 parser 测试启动整个 SessionActor。

## 一个好测试的结构

```rust
#[test]
fn missing_terminal_is_protocol_error() {
    // Arrange: 建立最小输入/替身。
    let stream = empty_stream();

    // Act: 执行一个行为边界。
    let error = block_on(call_terminal(stream)).unwrap_err();

    // Assert: 证明稳定、可观察的契约。
    assert_eq!(error.kind, ToolErrorKind::Custom);
    assert_eq!(error.code(), Some("stream_no_terminal"));
}
```

注释不是必须；结构应让失败时能立刻看出输入、行为和期望。一个测试同时验证十个不相关行为，失败定位会很差。

测试名描述条件与结果：`missing_terminal_returns_protocol_error` 比 `test_stream_2` 更有用。

## 断言语义，不绑定偶然实现

弱断言：

```rust
assert!(operation().is_err());
assert_eq!(events.len(), 3);
```

若契约关心错误分类和事件顺序，应更具体：

```rust
assert!(matches!(error.kind, ToolErrorKind::InvalidArguments));
assert_eq!(events, [Started, Progress("x"), Finished]);
```

但不要断言：

- HashMap 未承诺的遍历顺序。
- OS/依赖生成的完整错误措辞。
- 私有 helper 被调用次数（除非次数本身是外部契约）。
- 线程调度的偶然先后。
- 实际睡眠耗时精确到毫秒。

仓库可使用 `pretty_assertions` 改善结构化 diff；核心仍是选择稳定比较对象。

## 表驱动测试减少遗漏

```rust
#[test]
fn parses_boolean_spellings() {
    let cases = [
        (json!(true), Some(true)),
        (json!("yes"), Some(true)),
        (json!(0), Some(false)),
        (json!("maybe"), None),
    ];

    for (input, expected) in cases {
        assert_eq!(parse(&input), expected, "input: {input}");
    }
}
```

表驱动适合同一规则的等价类和边界。每个 case 的失败消息必须包含输入，否则循环只告诉你某次 assert 失败。

当 case 有复杂 setup 或不同断言逻辑时拆成独立测试，避免大表隐藏行为差异。

## Result 测试可以返回 Result

```rust
#[test]
fn round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let encoded = encode(&value)?;
    let decoded = decode(&encoded)?;
    assert_eq!(decoded, value);
    Ok(())
}
```

这适合 setup 失败直接终止，减少大量 unwrap。若要断言具体错误，仍应 `unwrap_err`/match，而不是用 `?` 提前离开。

## fake、stub、mock 与真实依赖

术语无需教条，但要清楚测试替身在证明什么：

| 替身 | 主要特点 | 适合证明 |
| --- | --- | --- |
| fake | 有简化但可工作的实现，如内存 FS | 调用方行为与状态变化 |
| stub | 对输入返回预设结果 | 成功/失败分支 |
| mock | 记录并断言交互 | 外部调用协议、次数/参数确为契约时 |
| simulator | 模拟时间/网络/状态机 | 复杂场景探索，但模型准确性需验证 |
| real dependency | 真实文件/进程/service | 集成契约，成本与不稳定性更高 |

仓库 [`MockFs`](../../crates/codegen/xai-grok-tools/src/computer/local/mock_fs.rs) 实现与生产相同的 `AsyncFileSystem`：

```rust
pub struct MockFs {
    files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
}

#[async_trait::async_trait]
impl AsyncFileSystem for MockFs {
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>, ComputerError> {
        self.files.read().await.get(path).cloned().ok_or_else(|| {
            ComputerError::IOError(
                format!("File not found: {}", path.display()),
                Some(std::io::ErrorKind::NotFound),
            )
        })
    }
}
```

这是仓库实现的核心片段，不是伪造的简化 API。返回值 `.cloned()` 脱离 lock guard，调用方拿到 owned bytes；缺失文件同时保留 `ErrorKind::NotFound`，下游才能按错误类别分支。Fake 应遵守生产 trait 的关键契约：错误分类、路径语义、并发与返回所有权；过于宽松的 fake 会让错误实现通过。

## 不要 mock 纯内部细节

若为了测试每个函数都抽 trait，代码会充满无业务意义的动态边界。优先在真正外部/非确定边界注入：文件系统、时钟、网络、随机源、进程、权限 gateway。

纯函数通过输入输出直接测试；Actor 可通过 handle/channel 的公开可观察结果测试，不必暴露内部字段。

## 文件系统测试

```rust
let dir = tempfile::tempdir()?;
let path = dir.path().join("config.toml");
std::fs::write(&path, input)?;
```

- tempdir guard drop 时清理；不要只保存 `dir.path()` 后立刻丢 guard。
- 不写用户 home、真实 repo 或固定 `/tmp/name`。
- 测试 symlink、权限、原子 rename 时考虑平台差异。
- 不依赖目录遍历顺序，必要时排序。
- 失败后若需保留 artifact 排障，使用测试框架约定而非取消所有清理。

涉及当前工作目录的 API 会修改进程全局状态，尽量让函数接受显式 Path；否则必须串行并恢复。

## 环境变量和其他进程全局状态

环境变量、cwd、全局 tracing subscriber、locale、时区、单例 cache 会被同一 test binary 的并行测试共享。

可靠策略：

- 通过参数/依赖注入避免全局读取。
- 使用 RAII guard 保存并恢复旧值，包括 panic 路径。
- 必须时用串行锁/`serial_test`，但它只协调采用同一机制的测试。
- 测试名称和 fixture 使用唯一临时值。
- 不假设测试执行顺序。

仅加 `--test-threads=1` 会掩盖并发问题并拖慢整个 binary，不是首选设计。

## Tokio test 的 runtime 选择

```rust
#[tokio::test]
async fn request_gets_reply() { ... }

#[tokio::test(flavor = "current_thread")]
async fn local_actor_works() { ... }
```

默认/显式 runtime flavor 应匹配生产模型：

- current-thread 适合 `LocalSet`、`spawn_local` 与确定的单线程 actor。
- multi-thread 能暴露 Send/同步问题，但调度顺序不可假定。
- 纯同步逻辑继续使用 `#[test]`，不为了调用一个 helper 建 runtime。

测试 feature 中需要 Tokio `macros`/`rt`，虚拟时间还需要 `test-util`；检查 manifest 的 dev-dependencies。

## 用事件屏障，不用拍脑袋 sleep

脆弱测试：

```rust
spawn_actor();
tokio::time::sleep(Duration::from_millis(50)).await;
assert!(actor_probably_started());
```

慢机器可能没启动，快机器浪费时间。改用可观察事件：

```rust
let (ready_tx, ready_rx) = oneshot::channel();
spawn(async move {
    initialize().await;
    let _ = ready_tx.send(());
    run().await;
});
ready_rx.await.expect("actor should announce readiness");
```

生产 API 若没有可观察 ready/ack，测试困难可能揭示真实生命周期契约缺失。

## 为等待设置测试超时

Channel 测试若实现错误可能永久等待：

```rust
let reply = tokio::time::timeout(Duration::from_secs(1), reply_rx)
    .await
    .expect("actor did not reply before test deadline")
    .expect("actor dropped reply sender");
```

超时是测试的故障保险，不应替代确定性事件。CI 预算要足够覆盖正常慢机器，又不能让死锁挂数分钟。

失败消息区分“deadline elapsed”和“sender dropped”，方便定位。

## 虚拟时间

Tokio test-util 可暂停时间：

```rust
#[tokio::test(start_paused = true)]
async fn retries_after_backoff() {
    let task = tokio::spawn(run_retry_loop());
    tokio::time::advance(Duration::from_secs(30)).await;
    // assert state/event
    task.abort();
}
```

适合 timer、backoff、interval，不适合真实网络/文件耗时。注意：

- 代码必须使用 Tokio time，不是 `std::thread::sleep`/wall clock。
- 推进时间后让 executor poll（必要时 `yield_now` 或等事件）。
- interval 的 missed-tick 行为也属于契约。
- 虚拟时间不会自动解决 channel race。

不要断言“真实 10ms 内完成”来测试逻辑 backoff。

## 测试 task 生命周期

异步测试结束时 runtime 会处理残留 task，但不能依赖这种隐式清理证明生产 shutdown 正确。每个 spawn 要有 owner：

```rust
struct ActorGuard {
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

impl ActorGuard {
    async fn shutdown(self) {
        self.cancel.cancel();
        self.task.await.expect("actor task should not panic");
    }
}
```

必要时 Drop 做 abort 兜底，但正常测试路径仍应走正式 shutdown 并 await。只调用 `abort()` 会跳过 flush/cleanup，不足以测试优雅关闭。

测试结束前检查：sender clone、server task、子进程、临时 listener 和 global subscriber 是否回收。

## 取消测试要覆盖阶段

至少考虑：

```text
取消前：操作尚未开始
执行中：停在可控 barrier/await
副作用后、ack 前：结果是否 unknown/committed
完成后：迟到 cancel 是否无害
父取消：是否传播子任务
子取消：是否错误反向取消父任务
```

通过 oneshot/barrier 把任务停在指定阶段，再触发 token，避免靠时间猜测。断言最终状态、reply、task join 与资源清理，而不只断言 token 已 cancelled。

## 并发测试不要依赖一种 interleaving

普通测试无法穷举线程调度。先将关键状态转移提取为纯函数/小状态机，用表驱动测试；再用受控 barrier 构造几个高风险顺序：

```text
send -> cancel -> receive
receive -> side effect -> cancel -> ack
close receiver -> concurrent send
```

压力循环偶尔能发现问题，但失败难复现，不能替代模型和同步边界分析。若项目采用 loom 等工具，再用它系统探索小并发模型；不要自行写随机 sleep 当“并发测试”。

## Serde/协议测试组合

详见 [05 Serde](./05-serde-and-wire-compat.md)。完整组合：

- exact serialization shape。
- hand-written legacy/invalid fixture deserialize。
- round-trip。
- unknown/missing/null 行为。
- 错误/source/secret 不泄露。
- schema 与实际字段一致。

仓库 [`notification_serde.rs`](../../crates/common/xai-tool-runtime/tests/notification_serde.rs) 的 helper 同时 round-trip 并返回 JSON，让各测试继续断言 type tag 和字段。

## Snapshot/golden test

Snapshot 适合较大稳定输出（渲染、prompt、协议 fixture），但 review 必须阅读 diff。危险做法是看到失败就整体 accept。

更新前确认：

- 变化是需求还是环境/排序噪声。
- 时间、路径、UUID、hash 是否应 normalize。
- snapshot 是否含 secret/用户数据。
- 旧格式兼容是否需要保留独立 fixture。

对于少量关键字段，显式 assert 通常比大 snapshot 更清楚。

## Compile-fail 与类型契约

某些契约是“这段代码不应编译”，例如 `ToolDispatch` 必须 object-safe、`Rc` 不应跨线程。可用 doctest `compile_fail`、trybuild 类框架，或本目录 [compile-fail katas](./labs/README.md) 验证诊断。

错误文本和次要诊断会随 rustc 变化；优先锁定是否失败、核心错误码或关键类型关系，不比较整段 stderr。

## 测试过滤器陷阱

精确运行 integration target：

```sh
cargo test -p xai-tool-runtime --test tool_streaming
```

只运行其中一个测试：

```sh
cargo test -p xai-tool-runtime \
  --test tool_streaming \
  streaming_ok_emits_progress_then_terminal -- --exact
```

列出实际名称：

```sh
cargo test -p xai-tool-runtime --test tool_streaming -- --list
```

注意：

- `cargo test -p pkg word` 过滤 test function 全名，不选择 `tests/word.rs`。
- 过滤结果 0 项仍可能退出 0。
- unit test 全名包含 module path。
- `#[ignore]` 默认不运行；使用 `-- --ignored` 或 `-- --include-ignored`。
- required-features 未开启时 target 可能被跳过。
- doctest、lib、bin、integration target 是不同 test binary。

每次记录 `running N tests`、`filtered out` 和 target 名。

## 从小到大的验证顺序

```text
1. 单个 test（开发循环）
2. 所在 integration target / lib tests
3. 目标 package 全测试
4. all-targets check/clippy
5. 关键反向依赖者
6. feature/platform/CI 矩阵
```

越大越慢但覆盖不同，不是“大命令自动包含所有语义”。全 workspace 默认 feature 测试不证明 no-default-feature 或另一平台。

## Flaky test 排查

先分类不确定来源：

| 症状 | 常见根因 |
| --- | --- |
| 单独过、并行失败 | global state、固定端口/路径、共享 cache |
| CI 慢机失败 | sleep 假设、过紧 timeout、资源竞争 |
| 顺序变化 | HashMap、并发 completion、未排序 FS |
| 偶尔挂住 | channel clone 未 drop、task 未 join、取消丢失 |
| 跨平台失败 | 路径、换行、权限、signal/cfg |

重复运行只用于复现概率，不是修复：

```sh
for i in {1..20}; do
  cargo test -p <package> <filter> || break
done
```

找到不确定输入后用 barrier、唯一资源、排序、虚拟时间或显式 dependency 注入消除它。

## 新行为的测试设计清单

1. 正常成功路径。
2. 最小/空/边界输入。
3. 每种有业务意义的错误 variant。
4. channel close、task panic/cancel、timeout（若相关）。
5. 旧数据/新字段/未知值（若 Serde）。
6. cleanup：task、文件、锁、进程、sender。
7. 并发顺序与容量边界。
8. feature/platform 分支。
9. 断言是否观察 public contract。
10. 测试命令是否实际运行该项。

覆盖规模应与风险一致。修改纯 parser 不需启动完整 agent；修改 session shutdown 不能只测一个纯 helper。

## 项目阅读路径

1. [`tool_streaming.rs`](../../crates/common/xai-tool-runtime/tests/tool_streaming.rs)：Progress* + Terminal 的顺序不变量。
2. [`trait_object_safety.rs`](../../crates/common/xai-tool-runtime/tests/trait_object_safety.rs)：public dyn dispatch 和无 Terminal 错误。
3. [`notification_serde.rs`](../../crates/common/xai-tool-runtime/tests/notification_serde.rs)：round-trip + exact tag。
4. [`MockFs`](../../crates/codegen/xai-grok-tools/src/computer/local/mock_fs.rs)：trait fake。
5. [`cancel_running_task_tests.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_tests/cancel_running_task_tests.rs)：高风险异步取消矩阵；先读测试名和 support，再选一条路径。

## 动手练习

```sh
# 先确认 test 名称
cargo test -p xai-tool-runtime --test tool_streaming -- --list

# 精确运行一项
cargo test -p xai-tool-runtime \
  --test tool_streaming \
  streaming_ok_emits_progress_then_terminal -- --exact

# 运行该 package 所有 integration/unit/doc tests
cargo test -p xai-tool-runtime
```

然后完成 [源码实验](./22-repository-reading-labs.md) 第 3 或第 8 关。写测试计划时必须列出：它证明什么、没有证明什么、实际选择的 target/feature、后台资源由谁清理，以及失败时哪条断言能定位问题。

完成标准：能为一个真实改动选择最小但充分的测试层级，并从 Cargo 输出证明测试确实执行，而不是只报告命令成功。
