# 3. trait、泛型与动态分发

trait 是行为契约。先读 trait 方法、关联类型和边界，再读实现；不要从具体实现反推接口意图。

```rust
trait Store: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
}

fn load<S: Store>(store: &S, key: &str) -> Option<String> { store.get(key) }
fn load_dyn(store: &dyn Store, key: &str) -> Option<String> { store.get(key) }
```

| 形式 | 选择时机 |
| --- | --- |
| `impl Trait` / `<T: Trait>` | 编译期已知具体类型，静态分发，适合通用辅助函数 |
| `dyn Trait` | 运行时替换实现，常与 `Arc` 合用，适合插件、后端和 mock |
| 关联类型 | 一个实现固定其输出类型，避免每次使用处重复泛型参数 |

## 项目中的锚点

- [`AsyncFileSystem`](../../crates/codegen/xai-grok-tools/src/computer/types.rs#L52) 与 [`TerminalBackend`](../../crates/codegen/xai-grok-tools/src/computer/types.rs#L286) 定义可替换的 I/O 边界，生产和测试实现共用同一契约。
- [`ManagedGatewayToolCaller`](../../crates/codegen/xai-grok-tools/src/types/resources.rs#L568) 展示 `#[async_trait]`、`Send + Sync` 与 `Arc<dyn Trait>`。
- [`MockFs`](../../crates/codegen/xai-grok-tools/src/computer/local/mock_fs.rs#L12) 是通过 trait 替换真实文件系统的简单例子。

### 仓库代码摘录：I/O 契约先于实现

[`AsyncFileSystem`](../../crates/codegen/xai-grok-tools/src/computer/types.rs#L52) 明确了工具层对文件系统的最小要求：

```rust
// 源码节选。async_trait 负责把 async 方法转换为 trait 可表达的 Future。
#[async_trait::async_trait]
pub trait AsyncFileSystem: Send + Sync {
    // &Path 是借用输入：调用者保有路径；返回 Vec 则转移读取到的字节所有权。
    async fn read_file(&self, path: &Path) -> Result<Vec<u8>, ComputerError>;

    // &[u8] 避免调用方为了写入而复制数据；实现决定何时写入完成。
    async fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), ComputerError>;

    async fn delete_file(&self, path: &Path) -> Result<(), ComputerError>;
}
```

`Send + Sync` 不是装饰：该接口会被放进 `Arc<dyn AsyncFileSystem>`，因此真实实现和 mock 都必须能满足并发调用边界。

### 项目关键代码：运行时可替换的工具客户端

[`ManagedGatewayToolClient`](../../crates/codegen/xai-grok-tools/src/types/resources.rs#L568) 将 trait object 放入 `Arc`：

```rust
#[async_trait::async_trait]
pub trait ManagedGatewayToolCaller: Send + Sync {
    async fn call_tool(
        &self,
        call_id: &str,
        arguments: serde_json::Value,
        caller: &str,
    ) -> Result<ManagedGatewayToolCallResponse, xai_tool_runtime::ToolError>;
}

// Arc 使同一个具体实现可被多个资源持有；dyn 允许在运行时注入不同后端。
pub struct ManagedGatewayToolClient(pub Arc<dyn ManagedGatewayToolCaller>);
```

调用方只依赖 `call_tool` 契约；生产实现、远端网关和测试替身不必出现在同一个泛型实例化中。

## `Send` 和 `Sync`

`Send` 表示值可移交到另一线程，`Sync` 表示 `&T` 可在多线程共享。`tokio::spawn` 通常要求 future 为 `Send + 'static`；`spawn_local` 则服务于 `!Send` 的单线程 Actor。两者是调度和所有权约束，不是性能标签。

## 设计检查

新增 trait 前先确认它是否真的隔离了外部边界或测试替身。仅为减少一个调用而抽象，会使 `dyn`、生命周期、错误类型和调用路径更难理解。
