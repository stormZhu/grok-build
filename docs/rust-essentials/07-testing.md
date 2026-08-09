# 7. Rust 测试与可替换边界

测试应证明可观察行为和失败边界，而不是仅覆盖私有实现。先选最小测试层级，再选择同步还是 Tokio 运行时。

```rust
#[test]
fn parses_default() { /* 纯同步逻辑 */ }

#[tokio::test]
async fn stops_when_cancelled() { /* I/O、channel 或 task */ }
```

## 常用模式

- 纯转换、验证和状态机用 `#[test]`；不为方便而引入 async。
- 使用 `#[tokio::test]` 验证通道、超时、取消与异步 I/O；测试必须有完成条件，避免无期限等待。
- 通过 trait 注入 fake/mock，而不是访问真实网络、用户目录或生产凭证。
- 用 `tempfile::tempdir()` 隔离文件系统；临时目录随 guard drop 清理。
- 环境变量是进程全局状态。修改后必须恢复，并用串行机制避免测试竞争。

## 项目中的锚点

- [`MockFs`](../../crates/codegen/xai-grok-tools/src/computer/local/mock_fs.rs#L12) 展示 `AsyncFileSystem` 的测试替身。
- [`persistence_tests.rs`](../../crates/codegen/xai-grok-shell/src/session/persistence_tests.rs#L71) 覆盖异步持久化成功和失败分支。
- [`loader.rs`](../../crates/codegen/xai-grok-config/src/loader.rs#L702) 使用 `tempfile` 与环境变量 guard 测试配置加载。

### 仓库代码摘录：用内存实现验证 I/O 契约

[`MockFs`](../../crates/codegen/xai-grok-tools/src/computer/local/mock_fs.rs#L12) 不接触真实磁盘：

```rust
// 源码节选：mock 的状态完全在内存中，测试无需写入真实工作区。
pub struct MockFs {
    files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
}

#[async_trait::async_trait]
impl AsyncFileSystem for MockFs {
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>, ComputerError> {
        // read().await 只在取得锁时等待；cloned() 让返回值脱离锁 guard。
        self.files.read().await.get(path).cloned().ok_or_else(|| {
            ComputerError::IOError(
                format!("File not found: {}", path.display()),
                Some(std::io::ErrorKind::NotFound),
            )
        })
    }
}
```

它与生产实现共享 `AsyncFileSystem`，所以调用方测试能验证读写失败和状态变化，而不会依赖工作目录、权限或机器速度。

### 项目关键代码：测试也要拥有后台 Actor 的生命周期

[`ActorGuard`](../../crates/codegen/xai-grok-shell/src/session/persistence_tests.rs#L4) 用 RAII 式包装避免测试遗留 task：

```rust
struct ActorGuard {
    handle: PersistenceHandle,
    task: tokio::task::JoinHandle<()>,
}

impl ActorGuard {
    async fn stop(self) {
        // 先请求停止，再 await，确保测试结束前 task 不再运行。
        self.task.abort();
        let _ = self.task.await;
    }
}
```

写 async 测试时，不能只断言结果；还要明确后台任务由谁停止，否则后续测试可能继承它的状态或日志。

## 异步测试检查表

测试发消息后要 await 哪个可观察结果？发送端或接收端关闭时的错误是否断言？取消发生在操作前、操作中、操作后各有何结果？涉及时间时能否通过受控事件而不是长 sleep 驱动？

## 阅读检查点

为一个新增 trait 方法写出至少一个生产实现测试和一个 fake/mock 驱动的调用方测试，确保契约而非偶然实现被覆盖。
