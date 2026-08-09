# 20. Rust 高密度语法解码

大型 Rust 仓库的难点常常不是某个单独概念，而是多个概念压在一行里。本章不是完整语法参考，而是一张针对本仓库高频写法的“拆句表”。原则只有一个：**从最外层类型向内读，从函数签名先于函数体读**。

## 先读懂一条真实类型

[`ToolStream`](../../crates/common/xai-tool-runtime/src/tool.rs) 定义为：

```rust
pub type ToolStream<T> = Pin<Box<dyn Stream<Item = ToolStreamItem<T>> + Send>>;
```

从外向内拆：

1. `ToolStream<T>` 是类型别名，不是新的运行时类型。
2. `Pin<...>`：里面的值不能再被安全地随意移动，供异步 Stream 使用。
3. `Box<...>`：值放在堆上，大小在编译期无需固定。
4. `dyn Stream<...>`：具体 Stream 实现被类型擦除，调用方只依赖契约。
5. `Item = ToolStreamItem<T>`：关联类型 `Item` 被指定。
6. `+ Send`：这个 trait object 可在线程间转移。

把它翻译成白话：“一个被固定在堆上的、可跨线程转移的任意 Stream；每次产生进度或终态。”

## 路径、类型和方法中的 `Self`

| 写法 | 含义 |
| --- | --- |
| `self` | 当前值，具体是拥有、共享借用还是可变借用由接收者决定 |
| `&self` | 共享借用当前值 |
| `&mut self` | 独占可变借用当前值 |
| `Self` | 当前正在实现的类型 |
| `Self::Args` | 当前 trait 实现选择的关联类型 |
| `Self::Terminal(_)` | 当前 enum 类型的 `Terminal` variant |
| `super::Type` | 父模块中的名字 |
| `crate::Type` | 当前 crate 根开始的绝对路径 |
| `::external_crate` | extern prelude 中的绝对路径，实际代码较少这样写 |

例如：

```rust
pub fn is_terminal(&self) -> bool {
    matches!(self, Self::Terminal(_))
}
```

这里 `self` 是 `&ToolStreamItem<T>`，`Self` 是 `ToolStreamItem<T>`，`_` 表示关心 variant 而不绑定内部值。

## 泛型约束怎样断句

```rust
pub fn terminal_only<T: Send + 'static>(
    result: Result<T, ToolError>,
) -> ToolStream<T>
```

- `<T: ...>` 声明类型参数。
- `Send` 表示 `T` 可被转移到另一线程。
- `'static` 在这里表示 `T` 不含短于程序所需范围的借用；不等于“它永远活着”。
- 参数拥有 `Result<T, ToolError>`，函数会消费它。

约束较长时移到 `where`，语义不变：

```rust
fn with_progress<T, P, F>(progress: P, terminal: F) -> ToolStream<T>
where
    T: Send + 'static,
    P: Stream<Item = ToolProgress> + Send + 'static,
    F: Future<Output = Result<T, ToolError>> + Send + 'static,
```

阅读顺序：先看返回类型，再看每个泛型在参数中扮演什么角色，最后看约束。

## `impl Trait` 的两个位置

```rust
fn new(detail: impl Into<String>) -> Self
```

参数位置的 `impl Into<String>` 近似“调用者可传任何能转换成 String 的具体类型”。每次调用仍是静态分发。

```rust
fn run(...) -> impl Future<Output = Result<Self::Output, ToolError>> + Send
```

返回位置的 `impl Trait` 表示“实现方返回一个具体但对调用者隐藏的类型”。它保留静态分发，和 `Box<dyn Trait>` 的运行时动态分发不同。

快速比较：

| 写法 | 谁选择具体类型 | 大小 | 分发 |
| --- | --- | --- | --- |
| `<T: Trait>` | 调用者 | 已知 | 静态 |
| 参数 `impl Trait` | 调用者 | 已知 | 静态 |
| 返回 `impl Trait` | 实现者 | 已知但隐藏 | 静态 |
| `dyn Trait` | 运行时装入的值 | 未知，需 `&`/`Box`/`Arc` | 动态 |

## 关联类型与 object safety

[`Tool`](../../crates/common/xai-tool-runtime/src/tool.rs) 中：

```rust
pub trait Tool: Send + Sync {
    type Args: for<'de> Deserialize<'de> + JsonSchema + Send + 'static;
    type Output: Serialize + ToolOutput + Send + 'static;
}
```

实现者为一个工具选择唯一的 `Args` 和 `Output`。这比在每个方法上写 `<Args, Output>` 更能表达“一种工具对应一组输入输出”。

这个 trait 的方法返回依赖 `Self` 的 `impl Future`，因此项目不直接使用 `dyn Tool`。它通过 object-safe 的 [`ToolDispatch`](../../crates/common/xai-tool-runtime/src/dispatch.rs) 把参数擦除为 `serde_json::Value`，在运行时边界恢复动态分发。

读 trait 时要问：

- 调用者需要知道具体实现类型吗？
- 关联类型何时从强类型变成 JSON 或其他擦除类型？
- `Send + Sync` 是给值、Future，还是 trait object 的约束？

## `for<'de>` 不是普通生命周期参数

```rust
type Args: for<'de> Deserialize<'de>;
```

这叫 higher-ranked trait bound（HRTB），读作：“对于任意反序列化输入生命周期 `'de`，Args 都能反序列化。”它比在外层固定一个具体 `'de` 更强，适合接收任意临时输入缓冲区。

遇到 `'static` 与 `for<'a>` 时不要混淆：

- `T: 'static`：`T` 不携带短生命周期借用。
- `for<'a> Trait<'a>`：对每一个可能的 `'a` 都实现该 trait。
- `&'static str`：引用的数据确实在整个程序期间有效，字符串字面量常属于此类。

## turbofish 与类型推断

```rust
let value = serde_json::from_value::<PartialResultPayload>(json)?;
let names = iter.collect::<Vec<_>>();
```

`::<...>` 被昵称为 turbofish，用于显式告诉泛型函数目标类型。`_` 仍交给编译器推断。

以下三种写法常等价：

```rust
let names: Vec<String> = iter.collect();
let names = iter.collect::<Vec<String>>();
let names = iter.collect::<Vec<_>>();
```

当报错落在 `.collect()`、`.parse()`、`.into()` 上时，先检查目标类型是否足够明确。

## 模式不是只出现在 `match`

```rust
let TaskWakeAdmission { respond_to, fallback } = admission;
```

这是不可失败的解构，把两个字段移动到局部变量。

```rust
let PromptOrigin::TaskCompleted { task_id } = origin else {
    return None;
};
```

`let ... else` 用于“只接受这个形状，否则提前退出”。成功分支中的绑定可在后续使用。

```rust
if let Some(registry) = session.hook_registry.borrow().clone() {
    // registry 在此作用域中可用
}
```

```rust
match item {
    ToolStreamItem::Progress(_) => continue,
    ToolStreamItem::Terminal(result) => return result,
}
```

模式可能移动、复制或借用字段，取决于被匹配表达式以及绑定方式：

```rust
match value { ... }      // 通常消费 value
match &value { ... }     // 匹配借用，value 仍可用
Some(ref text) => ...    // 显式借用内部字段；现代代码更多依赖 match ergonomics
Some(text) => ...        // text 的类型由被匹配对象是值还是引用决定
```

## `?`、`From` 和嵌套 Result

```rust
let value = parse(input)?;
```

近似于：

```rust
let value = match parse(input) {
    Ok(value) => value,
    Err(error) => return Err(From::from(error)),
};
```

因此读 `?` 时要同时看当前函数返回类型和可用的 `From` 实现。异步关闭代码里常见：

```rust
match timeout(duration, ack).await {
    Ok(Ok(Ok(()))) => {}
    Ok(Ok(Err(error))) => { /* 业务操作失败 */ }
    Ok(Err(_)) => { /* oneshot sender 被丢弃 */ }
    Err(_) => { /* timeout 自己超时 */ }
}
```

从外向内读：`timeout Result` → `oneshot Result` → `业务 Result`。不要把三个 `Err` 当成同一种失败。

## 闭包、捕获与 `move`

```rust
.map(|(mut watcher, mut changes)| {
    let session = session.clone();
    tokio::task::spawn_local(async move {
        while let Some(change) = changes.recv().await {
            session.reload_skills_from_disk().await;
        }
    })
})
```

- `|args| expression` 是闭包。
- 闭包会按实际使用方式借用、可变借用或取得环境变量所有权。
- `async move` 把捕获值移入 Future，使 Future 能比当前栈帧活得更久。
- 在 `move` 前 `session.clone()` 是在选择“移动一个共享句柄”，不是深拷贝 Actor。

判断 clone 是否合理，先看类型：`Arc::clone`、sender clone 和 `String::clone` 成本与语义完全不同。

## 方法链要按类型阶段切开

```rust
path.extension()
    .and_then(|ext| ext.to_str())
    .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
```

逐段写出类型：

```text
extension()   -> Option<&OsStr>
and_then(...) -> Option<&str>
is_some_and   -> bool
```

高频组合子：

| 方法 | 白话 |
| --- | --- |
| `map` | 有值就转换，保持容器形状 |
| `and_then` | 有值就调用另一个也返回容器的操作，避免嵌套 |
| `filter` | 不满足条件就变成 `None` |
| `unwrap_or` / `unwrap_or_else` | 缺失时给默认值 |
| `ok_or` / `ok_or_else` | `Option` 转 `Result` |
| `is_some_and` | 有值且谓词成立 |
| `then_some` | bool 为真时产生 `Some(value)` |
| `collect` | 将迭代器消费成集合或其他目标 |

方法链超过认知负荷时，把每一段结果类型写在纸上；无需先读闭包内部所有细节。

## 借用转换与智能指针

仓库中常见的转换不一定分配：

| 表达式 | 通常发生什么 |
| --- | --- |
| `String::as_str()` | `String` 借用为 `&str` |
| `PathBuf::as_path()` | `PathBuf` 借用为 `&Path` |
| `Option::as_ref()` | `Option<T>` 借用为 `Option<&T>` |
| `Result::as_ref()` | `Result<T,E>` 借用为 `Result<&T,&E>` |
| `Arc::clone(&x)` | 原子增加引用计数，共享同一分配 |
| `Box::pin(x)` | 放到堆上并固定 |
| `value.into()` | 目标类型明确时调用 `Into`，是否分配取决于实现 |
| `value.as_ref()` | 通过 `AsRef` 获得借用视图，具体目标需由上下文推断 |

`Deref` 还能触发自动解引用，所以 `Arc<T>` 上能调用 `T` 的 `&self` 方法。读不清时把自动步骤展开：`arc.method()` 可近似想成 `T::method(&*arc)`。

## 属性和 derive 要当作代码生成/配置读

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolProgress { ... }
```

- `derive` 生成 trait 实现。
- `serde(...)` 配置生成的序列化实现。
- `#[cfg(test)]` 只在测试配置编译。
- `#[cfg(target_os = "macos")]` 只在目标平台编译。
- `#[non_exhaustive]` 告诉外部调用者 match 时必须保留兜底分支。
- `#![forbid(unsafe_code)]` 是 crate 级属性，作用于整个 crate。

属性可能改变 wire format、可用 API 和编译目标，不能当作注释略过。

## 一行复杂签名的固定解法

看到下面这类代码：

```rust
fn execute(&self, ctx: C, args: Self::Args)
    -> impl Future<Output = ToolStream<Self::Output>> + Send
```

依次回答：

1. 这是 trait 声明、trait impl、固有 impl，还是自由函数？
2. 接收者是 `self`、`&self` 还是 `&mut self`？
3. 参数所有权是否进入函数？
4. 最外层返回的是 Future、Result、Option、Stream 中哪一个？
5. Future 完成后得到什么？Stream 每项又是什么？
6. 哪些约束服务于跨线程，哪些服务于序列化，哪些服务于动态分发？

如果能稳定回答这六问，仓库中绝大多数“吓人的类型”就只是多层普通类型。

## 即时练习

不看答案，解释以下三句：

```rust
source.as_ref().map(|e| e.as_ref() as &_)
(was != actual).then_some(actual)
while let Some(item) = stream.next().await
```

答案：

1. 把 `Option<anyhow::Error>` 借用成 `Option<&anyhow::Error>`，再映射成编译器推断的 error trait object 引用。
2. 状态确实变化时返回 `Some(actual)`，否则返回 `None`。
3. 异步等待 Stream 下一项，只要仍有 `Some(item)` 就循环；`None` 表示流结束。

然后打开 [`tool.rs`](../../crates/common/xai-tool-runtime/src/tool.rs)，任选三个公开签名，用本章方法逐层翻译成白话。能翻译不等于完全理解实现，但它会把问题从“整行都不会”缩小为一个可查的具体概念。
