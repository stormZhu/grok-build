# 1. 所有权、借用与生命周期

Rust 用所有权决定值何时释放，用借用决定谁可暂时访问。读仓库代码时先问三件事：当前表达式得到 owned value 还是 reference？这一步 move、reborrow 还是 clone？该值要不要跨 `.await`、task、线程或结构体生命周期保存？

## place、value 与 move

`String`、`PathBuf`、`Vec<T>` 等拥有 heap/resource。赋值、按值传参和按值模式匹配通常会 move：

```rust
let first = String::from("bash");
let second = first;

// 编译失败：String 的所有权已转给 second。
// println!("{first}");
```

move 通常只复制栈上的表示（pointer/length/capacity），并把原绑定标为不可再用；不会逐字节复制 heap payload。最终只有新 owner drop 资源，避免 double free。

函数签名直接表达转移：

```rust
fn consume(name: String) { /* 取得所有权 */ }
fn inspect(name: &str) { /* 临时只读借用 */ }
fn edit(name: &mut String) { /* 临时唯一可变借用 */ }
```

返回 owned value 会把所有权交给调用者；返回 reference 必须说明它借用自哪个输入。

## `Copy` 与 `Clone` 不同

整数、bool、许多小型纯值类型实现 `Copy`：

```rust
let first: u64 = 7;
let second = first;
assert_eq!(first, second); // 隐式按位复制
```

`Copy` 是隐式、低层复制语义；实现 `Drop` 的类型不能同时 Copy。`Clone` 是显式操作，可执行分配、递归复制或只增加引用计数：

```rust
let text_copy = text.clone();       // String：复制字符 buffer
let handle_copy = Arc::clone(&arc); // Arc：增加强引用计数
```

同样写 `clone`，成本和语义可能完全不同。读调用点时查看具体类型，不要把 clone 一律视为坏味道，也不要用它掩盖不清楚的 owner。

## 借用的核心规则

在同一段有效借用期内：

- 可以有多个 `&T`。
- 或恰好一个 `&mut T`。
- 两者不能重叠。
- reference 不能比被引用值活得久。

```rust
let mut values = vec![1, 2];

let first = &values[0];
println!("{first}"); // first 最后一次使用

values.push(3); // 共享借用已结束，可以可变访问
```

Non-lexical lifetimes 让借用常在最后一次使用后结束，而不是机械活到代码块末尾。但 `RefCell::Ref`、锁 guard 等拥有 Drop 的值最好用小作用域明确释放，见 [15 内部可变性](./15-interior-mutability-and-locks.md)。

## reborrow 与 `&mut`

`&mut T` 不是 Copy。把它直接 move 给另一个绑定会转移使用权，但函数调用常自动建立更短的 reborrow：

```rust
fn push_marker(value: &mut String) {
    value.push('!');
}

fn update(value: &mut String) {
    push_marker(value); // 等价于更短的 &mut *value reborrow
    push_marker(value); // 原 &mut 再次可用
}
```

看到可变引用在循环或多次调用中复用时，先考虑编译器是否建立了 reborrow，不要误以为底层 String 每次被 move。

若错误提示 borrow 跨太久，缩短引用/guard 的使用区间，或先提取 owned snapshot；不要第一反应上 `unsafe`。

## pattern 可能 move 字段

`match`/`if let`/`let` 的绑定方式决定字段是 move 还是 borrow：

```rust
struct Request {
    id: String,
    payload: Vec<u8>,
}

let request = make_request();

// move id，request 发生 partial move；仍可用未移动的 payload，
// 但不能再把整个 request 当完整值使用。
let Request { id, payload: _ } = request;
```

只想观察：

```rust
match &request {
    Request { id, payload } => {
        inspect(id);      // &String 可 coercion 为 &str
        inspect_bytes(payload); // &Vec<u8> 可 coercion 为 &[u8]
    }
}
```

也可在 pattern 中写 `ref`/`ref mut`，但仓库现代代码更常 match `&value`/`&mut value` 或先用 `as_ref`。

对 enum：

```rust
let maybe: Option<String> = Some("model".into());

if let Some(name) = maybe.as_ref() {
    // name: &String；maybe 没被消费
}

if let Some(name) = maybe {
    // name: String；maybe 被按值匹配
}
```

## `as_ref`、`as_deref` 与 `cloned`

这些 adapter 是读仓库 Option/Result 的高频语法：

```rust
let owned: Option<String> = Some("grok".into());

let by_ref: Option<&String> = owned.as_ref();
let as_str: Option<&str> = owned.as_deref();
let copied_owner: Option<String> = owned.as_ref().cloned();
```

- `as_ref` 把“拥有容器”变成“容器里的借用”，不消费原值。
- `as_deref` 再通过 `Deref` 把 `&String` 变 `&str`、`&PathBuf` 变 `&Path` 等。
- `cloned` 对 `Option<&T>`/iterator item 执行 Clone，产出 owned T。
- `copied` 只适用于 Copy 值。

看到长链时逐步标 item type，比猜方法名有效；见 [20 语法解码](./20-syntax-decoder.md)。

## owned 与 borrowed 容器边界

常见搭配：

| Owned | Borrowed view | 适合 |
| --- | --- | --- |
| `String` | `&str` | 保存文本 / 临时读取文本 |
| `PathBuf` | `&Path` | 保存路径 / 调用路径 API |
| `Vec<T>` | `&[T]` / `&mut [T]` | 可增长集合 / 固定范围 view |
| `Box<T>` | `&T` | heap ownership / 临时访问 |
| `Arc<T>` | `&T` | 共享 owner / 当前调用借用 |

API 通常接受最弱、最通用的 borrowed view，只有需要保存、跨 task 或转移时才取得 owned value：

```rust
fn parse(input: &str) -> Parsed;
fn open(path: &Path) -> Result<File>;
fn enqueue(job: Job); // queue 必须拥有 job
```

泛型 `impl AsRef<Path>` 让调用者传多种形状，但函数内部每次 `as_ref()` 只得到临时 reference；要保存仍需 `to_path_buf`。

## 仓库边界一：借用参数变成 task-owned 值

[`session/file_system.rs`](../../crates/codegen/xai-grok-shell/src/session/file_system.rs) 的 `write_file` 接受借用，进入 `spawn_blocking` 前转 owned：

```rust
pub async fn write_file(
    abs_path: &Path,
    content: &str,
    create_dirs: bool,
) -> Result<()> {
    let abs_path = abs_path.to_path_buf();
    let content = content.to_string();

    tokio::task::spawn_blocking(move || {
        if create_dirs && let Some(parent) = abs_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&abs_path, content.as_bytes())
    })
    .await??;

    Ok(())
}
```

类型流：

```text
调用期间借用：&Path / &str
        |
        | to_path_buf / to_string
        v
task 自己拥有：PathBuf / String
```

`spawn_blocking` closure 可能比当前函数栈帧活得久，要求 `Send + 'static`。`move` 只把 closure 捕获的 owned values 移进去；若仍捕获 `&Path`/`&str`，引用生命周期通常不满足。

`'static` 不表示 task 永远运行，只表示 future/closure 不借用短生命周期栈数据。

## 仓库边界二：结构体保存拥有状态

[`ShellState::init`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs) 接收 `cwd: &Path`，返回拥有值：

```rust
#[derive(Debug, Clone)]
pub struct ShellState {
    pub cwd: PathBuf,
    pub snapshot: String,
    pub shell: ShellKind,
}

pub async fn init(shell: ShellKind, cwd: &Path, /* ... */) -> Result<Self, ComputerError> {
    // ...
    Ok(Self {
        cwd: cwd.to_path_buf(),
        snapshot: String::new(),
        shell,
    })
}
```

`ShellState` 会跨后续命令和 async 调用保存，不能引用 `init` 调用者临时传入的 Path。构造时复制路径是明确的 ownership boundary。

## 仓库边界三：guard 变 owned snapshot

[`SessionMemory::storage`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 不把 `RefCell` guard 交给调用方：

```rust
pub(crate) fn storage(&self) -> Option<MemoryStorage> {
    self.storage.borrow().clone()
}
```

按类型展开：

```rust
let snapshot: Option<MemoryStorage> = {
    let guard: Ref<'_, Option<MemoryStorage>> = self.storage.borrow();
    (*guard).clone()
}; // guard drop，运行时 borrow 结束

// snapshot 已拥有 PathBuf 等字段，可跨 await 使用。
```

[`MemoryStorage`](../../crates/codegen/xai-grok-memory/src/storage.rs) 的 Clone 会复制其 PathBuf 字段，不是只增加 Arc 计数。这是为了让调用者释放 `RefCell` borrow 后独立操作；是否值得换成共享 handle 必须结合成本和并发模型判断。

## 生命周期标注表达关系

生命周期参数不会延长任何值，只描述 reference 之间已存在的关系：

```rust
fn find<'a>(items: &'a [Item], id: &str) -> Option<&'a Item> {
    items.iter().find(|item| item.id == id)
}
```

意思是：返回 reference 若存在，借用自 `items`，不能超过 items 的有效期；它不借用 `id`。

错误版本：

```rust
fn broken<'a>() -> &'a str {
    let local = String::from("temporary");
    &local
}
```

调用者可任意选择 `'a`，但局部 String 在函数返回时 drop，标注无法制造更长所有者。正确方向通常返回 `String`、由调用者传 buffer，或引用真正更长寿命的数据。

## 生命周期省略规则

常见签名无需显式标注：

```rust
fn first(input: &str) -> &str;
fn method(&self, key: &str) -> &Value;
```

简化理解：

- 每个 input reference 先获得独立生命周期。
- 只有一个 input lifetime 时，output reference 使用它。
- method 有 `&self`/`&mut self` 时，output 默认借用 self。
- 多个可能来源且没有 self 时，编译器无法猜，需显式标注或改 API。

显式 lifetime 解决“输出借用谁”的歧义，不解决 dangling reference。

## `&'static T` 与 `T: 'static`

两者不同：

- `&'static str` 是引用本身可在整个程序期间有效，常来自 string literal 或泄漏的全局数据。
- `T: 'static` 表示 T 不包含短于 static 的借用；owned `String` 满足它，即使 String 值几毫秒后就 drop。

`tokio::spawn` 要求 future `'static`，通常含义是 future 拥有捕获数据，没有借用当前栈，不是要求 task 活到进程结束。

不要用 `Box::leak` 或全局 static 为了解决普通生命周期报错；先找真正 owner。

## 借用跨 `.await`

引用可以跨 await，只要生成的 future 在整个借用期内不逃逸：

```rust
async fn inspect(path: &Path) -> Result<Metadata> {
    tokio::fs::metadata(path).await
}
```

调用者 await `inspect` 时，future 借用 path，调用者必须让 path 活到完成。问题常出在：

- future 被 `spawn`，需要 `'static`。
- guard 跨 await 阻止其他访问或使 future 非 Send。
- 可变借用跨 await 后，其他 task 逻辑需要同一状态。
- future 被保存进结构体/集合，比原 owner 活得久。

解决选择取决于语义：缩短 guard、clone Arc/handle、复制 owned snapshot、把操作留在 owner actor，或让 API 的 lifetime 显式传播。不要机械加 `move`；`move` 只移动捕获物，若捕获物本身是 reference，仍然是 reference。

## 返回 reference 还是 owned value

返回 reference：

- 无分配/复制。
- 调用者受 owner 的 borrow 约束。
- 适合同步、短期 view。

返回 owned：

- ownership 清晰，可跨 task/channel/await。
- 可能分配或 clone。
- 适合 snapshot、缓存结果、协议 message。

也可返回 `Arc<T>`/`Bytes` 等共享 owned handle，或 `Cow<'a, T>` 在无需修改时借用、需要规范化时 owned。不要为“零拷贝”让整个调用链背负复杂 lifetime，先测量复制是否真是瓶颈。

## 所有权错误诊断顺序

| 错误/症状 | 先问 |
| --- | --- |
| E0382 use of moved value | 哪个赋值、调用或 pattern 按值消费；后续是否应借用或重排 owner |
| E0502/E0499 borrow conflict | 哪个 reference 最后一次使用；能否缩小作用域或拆分字段/slice |
| E0515 return local reference | 返回值真正应由谁拥有 |
| E0716 temporary dropped | 临时 owner 是否需先绑定局部变量 |
| future not `Send` | 哪个 guard/reference 跨 await；是否真的应 spawn 到多线程 |
| `'static` bound 不满足 | task/trait object 是否保存了短借用；应拥有、共享还是让调用者 await |

编译器错误地图与可运行反例见 [23](./23-compiler-error-atlas.md) 和 [labs](./labs/README.md)。

## Clone 决策清单

调用 Clone 前回答：

1. clone 的具体类型是什么，深复制还是引用计数？
2. 新 owner 为什么需要独立活过原 owner/guard/task？
3. 能否只借用到最后一次同步使用？
4. clone 是否为了释放锁/borrow 后跨 await，这是明确设计收益吗？
5. payload 是否大到需要 profile 证据？
6. 改成 Arc 会不会引入共享可变性和更复杂同步？
7. 协议/channel 是否本来就要求 owned message？

目标不是“零 clone”，而是每个 clone 都有 ownership 理由。

## 阅读练习

1. 运行 [labs](./labs/README.md) 的 kata 01、02、05、11，并在运行前标出 move/borrow/item type。
2. 从 [`write_file`](../../crates/codegen/xai-grok-shell/src/session/file_system.rs) 画出 `&Path/&str -> PathBuf/String -> spawn_blocking` 的 owner 转移。
3. 在 [`SessionMemory`](../../crates/codegen/xai-grok-shell/src/session/memory_state.rs) 对比返回 `Ref` 与返回 `Option<MemoryStorage>` 会给调用者什么约束。
4. 找一个 `Option<String>::as_deref()` 调用，逐步写出链中每个类型，并确认原 Option 是否仍可使用。
5. 找一个 struct 中的 `Arc<dyn Trait>`，写出创建者、clone task、最后一个 owner 和底层对象的 Drop 时机。
6. 对一个 E0382/E0502 compile-fail 练习提出两种修复，比较 API、分配和并发语义，而不只让错误消失。

完成标准：能为一个真实函数画出 owner/borrow 的开始与结束，预测 pattern、clone、`as_ref`、task spawn 对类型的影响，并解释生命周期标注连接了哪些输入与输出。
