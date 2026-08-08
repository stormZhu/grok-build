# 1. 所有权、借用与生命周期

## 什么时候需要它

看到 `String`、`PathBuf`、`Vec<T>`、`Arc<T>`，或编译器报 “borrowed value does not live long enough” 时，先问：谁负责释放值，当前代码是移动、借用还是显式复制？

```rust
fn label(name: &str) -> String {
    format!("tool:{name}") // 借用输入，返回新拥有的字符串
}

let name = String::from("bash");
let first = label(&name);   // 不移动 name
let second = name;          // 这里才移动所有权
```

- `T` 表示拥有值；传参或赋值通常移动它。
- `&T` 是只读借用，`&mut T` 是唯一可变借用；借用存活时不能发生冲突访问。
- `Clone` 是显式复制语义，不要用它掩盖不清晰的所有权设计。
- `String`/`PathBuf` 拥有数据；`str`/`Path` 通常以借用形式出现。接口优先接受 `&str`、`&Path`，需要保留时再 `to_owned()`。

## 项目中的锚点

- [`TruncationConfig`](../../crates/codegen/xai-grok-tools/src/types/context.rs#L16) 传递拥有的配置，避免跨调用保存短生命周期借用。
- [`ShellState`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs#L258) 保存拥有的 `PathBuf` 和 `String` 快照。
- [`SessionMemory`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs#L11) 用 `RefCell` 保存只在单线程 Actor 内访问的可变状态。

### 仓库代码摘录：借用后立即释放

[`SessionMemory::storage`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs#L67) 不把 `Ref` guard 交给调用方：

```rust
pub(crate) fn storage(&self) -> Option<MemoryStorage> {
    self.storage.borrow().clone()
}
```

这里 clone 的是 `Option<MemoryStorage>`，不是延长 `RefCell` 的借用。调用方随后可以 `.await` 或再次借用 storage，不会因旧 guard 仍存活而 panic。

## 生命周期怎样读

生命周期通常不是“对象活多久”，而是**引用关系必须在何处保持有效**。例如 `fn find<'a>(items: &'a [Item]) -> Option<&'a Item>` 表示返回值借用自 `items`。调用方不能在使用返回引用前释放或修改该集合。

阅读复杂签名时先替换成一句话：返回值借用谁？它是否被跨 `.await`、跨 task 或放进长期状态？后两种情况通常需要拥有值或 `Arc`，不能仅保存临时引用。

## 常见误区

- `Arc::clone()` 只复制引用计数，不复制内部数据；它解决共享所有权，不自动保证可变访问安全。
- `.await` 可能让函数暂停，局部借用往往不能跨越需要 `'static` 的 `tokio::spawn` 边界。
- 不要把 `&str` 保存到由局部 `String` 派生的结构中；改为保存 `String` 或让结构借用一个更长寿命的输入。

## 阅读检查点

打开一个带 `Arc<dyn Trait>` 的字段，写下：哪个组件创建它、哪些任务 clone 它、底层对象何时最终析构？
