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

[`SessionMemory::storage`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs#L67) 不把 `Ref` guard 交给调用方。先看这个方法涉及的两个结构体。实际源码中的 `SessionMemory` 还包含 flush 配置、计数器等其他字段；为了聚焦 `storage`，下面的示例只列出与本例相关的字段，其他字段用注释标记：

```rust
use std::cell::RefCell;
use std::path::PathBuf;

pub(crate) struct SessionMemory {
    // RefCell 让代码即使只有 &SessionMemory，也能在运行时借用并修改内部值。
    // Option::None 表示 memory 功能关闭，Some 表示持有一个存储句柄。
    pub storage: RefCell<Option<MemoryStorage>>,

    // 其余字段省略，例如 flush 配置、计数器和最近一次 flush 内容。
}

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    global_dir: PathBuf,
    workspace_dir: PathBuf,
    workspace_path: PathBuf,
    ephemeral: bool,
}

impl SessionMemory {
    /// 判断 memory 是否已启用。
    pub(crate) fn is_enabled(&self) -> bool {
        self.storage.borrow().is_some()
    }

    /// 从 RefCell 中复制出一个拥有的 MemoryStorage。
    pub(crate) fn storage(&self) -> Option<MemoryStorage> {
        self.storage.borrow().clone()
    }
}
```

关键字段不是直接的 `Option<MemoryStorage>`，而是外面包了一层 `RefCell`：

```text
SessionMemory
└── storage: RefCell<Option<MemoryStorage>>
                 └── Some(MemoryStorage) 或 None
```

`Ref` 就来自这层 `RefCell`。标准库中，`RefCell::borrow()` 的返回类型可以简化理解为：

```rust
impl<T> RefCell<T> {
    pub fn borrow(&self) -> Ref<'_, T>;
}
```

因此，当 `T = Option<MemoryStorage>` 时：

```rust
let guard: std::cell::Ref<'_, Option<MemoryStorage>> = self.storage.borrow();
```

这里的 `Ref<'_, Option<MemoryStorage>>` 就是运行时只读借用的 guard。它的作用类似锁的 guard：创建时把 `RefCell` 的“不可变借用计数”加一，销毁时再减一。只要它还活着，同一个 `RefCell` 就不能成功执行 `borrow_mut()`；违反规则不是编译错误，而是运行时 panic。

仓库里的方法只有一行：

```rust
impl SessionMemory {
    pub(crate) fn is_enabled(&self) -> bool {
        self.storage.borrow().is_some()
    }

    pub(crate) fn storage(&self) -> Option<MemoryStorage> {
        self.storage.borrow().clone()
    }
}
```

把这一行按类型展开，等价逻辑更容易看清：

```rust
use std::cell::Ref;

pub(crate) fn storage(&self) -> Option<MemoryStorage> {
    let snapshot: Option<MemoryStorage> = {
        // 第 1 步：运行时不可变借用 RefCell，得到 guard。
        let guard: Ref<'_, Option<MemoryStorage>> = self.storage.borrow();

        // 第 2 步：Ref<T> 实现 Deref<Target = T>，所以 *guard 得到
        // Option<MemoryStorage>。这里调用的是 Option 的 Clone，进而克隆
        // Some 里的 MemoryStorage；不是克隆或延长 guard。
        (*guard).clone()

        // 第 3 步：离开这个代码块时 guard 被 drop，RefCell 的借用结束。
    };

    // snapshot 是拥有自己的 PathBuf 数据的 Option<MemoryStorage>，
    // 已经不再引用 self.storage。
    snapshot
}
```

之所以能克隆内部值，是因为 [`MemoryStorage`](../../crates/codegen/xai-grok-memory/src/storage.rs#L27) 派生了 `Clone`。它的三个 `PathBuf` 字段都会复制出各自拥有的路径数据，`bool` 则直接复制。因此方法返回的 `Option<MemoryStorage>` 是独立的拥有值，其生命周期不再受 `SessionMemory` 或临时 `Ref` 限制。

需要特别区分下面两个类型：

```rust
Ref<'_, Option<MemoryStorage>> // 借用 guard，仍然绑定 self.storage
Option<MemoryStorage>          // 拥有值，不再借用 self.storage
```

如果方法改成直接返回 guard，借用就会被交给调用方：

```rust
pub(crate) fn storage_ref(&self) -> Ref<'_, Option<MemoryStorage>> {
    self.storage.borrow()
}

let held = memory.storage_ref();

// held 仍活着，下面试图取得可变借用会在运行时 panic。
let mut writable = memory.storage.borrow_mut();
```

这类 guard 也不适合跨 `.await` 保存：异步函数暂停后，guard 可能比预期存活更久，让其他逻辑无法取得可变借用。当前实现先在一个很短的同步作用域中克隆出拥有值，再释放 guard；调用方拿到 `Option<MemoryStorage>` 后可以安全地 `.await`、再次调用 `borrow()`，或在需要切换 memory 开关时调用 `borrow_mut()`。这就是这里“返回拥有值，而不是把内部借用泄露给调用方”的含义。

### 项目关键代码：把借用输入变成可长期保存的状态

[`ShellState`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs#L255) 初始化时接收借用的 `&Path`，但结构体保存拥有的值：

```rust
// 源码节选：ShellState 可以跨 async 调用和进程生命周期保存。
#[derive(Debug, Clone)]
pub struct ShellState {
    // PathBuf 拥有 cwd 的路径数据，不依赖 init() 调用方。
    pub cwd: PathBuf,
    // String 拥有可回放的 shell 快照。
    pub snapshot: String,
    pub shell: ShellKind,
}

pub async fn init(shell: ShellKind, cwd: &Path, /* ... */) -> Result<Self, ComputerError> {
    // cwd 在这里只是借用；构造 Self 时会转换/复制为拥有的数据。
    // ...
}
```

## 生命周期怎样读

生命周期通常不是“对象活多久”，而是**引用关系必须在何处保持有效**。例如 `fn find<'a>(items: &'a [Item]) -> Option<&'a Item>` 表示返回值借用自 `items`。调用方不能在使用返回引用前释放或修改该集合。

阅读复杂签名时先替换成一句话：返回值借用谁？它是否被跨 `.await`、跨 task 或放进长期状态？后两种情况通常需要拥有值或 `Arc`，不能仅保存临时引用。

## 常见误区

- `Arc::clone()` 只复制引用计数，不复制内部数据；它解决共享所有权，不自动保证可变访问安全。
- `.await` 可能让函数暂停，局部借用往往不能跨越需要 `'static` 的 `tokio::spawn` 边界。
- 不要把 `&str` 保存到由局部 `String` 派生的结构中；改为保存 `String` 或让结构借用一个更长寿命的输入。

## 阅读检查点

打开一个带 `Arc<dyn Trait>` 的字段，写下：哪个组件创建它、哪些任务 clone 它、底层对象何时最终析构？
