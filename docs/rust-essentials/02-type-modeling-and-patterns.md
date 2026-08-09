# 2. 类型建模、模式匹配、迭代器与闭包

Rust 倾向将状态编码在类型中。读项目代码时，先问“这个类型允许构造哪些状态”，再问字段如何计算。`struct` 表达同时存在的数据，`enum` 表达互斥状态，模式匹配负责把状态重新拆开。

本章完成后，你应能解释：一个 `match` 是否移动字段、`Option<Result<T, E>>` 怎样变形、`iter()` 与 `into_iter()` 的 item 有何不同，以及闭包为什么只实现 `FnOnce`/`FnMut`/`Fn` 中的一部分。

## Rust 的块是表达式

`if`、`match` 和普通代码块都能产生值：

```rust
let label = if tokens == 0 {
    "empty"
} else {
    "non-empty"
};

let next = {
    let increment = 2;
    tokens + increment // 没有分号：这是块的返回值
};
```

末尾加分号会把表达式结果丢弃成 `()`。遇到“expected T, found ()”时，先检查分支末尾是否多了分号，或是否有一个分支没有产生同类型的值。

变量默认不可变；`mut` 允许同一绑定改变，shadowing 则创建新绑定：

```rust
let raw = " 42 ";
let raw = raw.trim();       // 新绑定，类型可以变化
let mut count = raw.parse::<u64>()?;
count += 1;                 // 同一绑定发生变化
```

读代码时不要把 shadowing 当作赋值。前一个绑定在该名字下被遮蔽，但它持有的值仍按正常作用域规则 drop。

## struct、tuple struct 与 newtype

普通 struct 的字段名表达语义：

```rust
struct PromptTurnOk {
    total_tokens: u64,
    completion_kind: PromptCompletionKind,
}
```

tuple struct 以位置区分字段；只有一个字段时常作为 newtype：

```rust
struct SessionId(String);
struct FrameSeq(u64);
```

newtype 在运行时通常没有额外成本，却能让编译器阻止 `SessionId` 与普通 `String`、`FrameSeq` 与任意 `u64` 混用。仓库协议层的 [`FrameSeq`](../../crates/common/xai-tool-protocol/src/ids.rs) 就把序号语义包回类型。

常见 struct 更新和解构：

```rust
let updated = Config {
    timeout: new_timeout,
    ..old
};

let Config { timeout, .. } = updated;
```

`..old` 会移动未实现 `Copy` 的剩余字段，可能导致 `old` 部分移动后不能整体使用。它不是自动 clone。

## enum 表达状态集合

```rust
enum Reply {
    Accepted { id: String },
    Rejected { reason: String },
}

fn message(reply: Reply) -> String {
    match reply {
        Reply::Accepted { id } => format!("accepted: {id}"),
        Reply::Rejected { reason } => reason,
    }
}
```

这里 `message` 按值接收 `Reply`，两个分支都把内部 `String` 移出。调用结束后整个 `reply` 被消费。

相比下面的“字段袋”：

```rust
struct WeakReply {
    accepted: bool,
    id: Option<String>,
    reason: Option<String>,
}
```

enum 无法表示“accepted=true 但没有 id”“成功和失败字段同时存在”等非法组合。这种设计原则常被概括为“make invalid states unrepresentable”。

仓库中的 [`SessionCommand`](../../crates/codegen/xai-grok-shell/src/session/commands.rs) 更进一步：每种 Actor 消息 variant 只携带该命令需要的数据和回复 channel。

```rust
pub enum SessionCommand {
    Initialize { system_prompt: String },
    GetToolOverrides {
        respond_to: oneshot::Sender<Option<ToolOverrides>>,
    },
    Prompt {
        prompt_id: String,
        prompt_blocks: Vec<ContentBlock>,
        // 其余 Prompt 专属字段……
    },
}
```

调用者不可能构造一个“GetToolOverrides 但没有 respond_to”的命令；类型在进入 actor 前就保证消息形状完整。

## match 的三个职责

`match` 同时做穷尽检查、解构和控制流分派：

```rust
match item {
    ToolStreamItem::Progress(progress) => handle(progress),
    ToolStreamItem::Terminal(Ok(output)) => return Ok(output),
    ToolStreamItem::Terminal(Err(error)) => return Err(error),
}
```

模式中常见符号：

| 模式 | 含义 |
| --- | --- |
| `_` | 匹配任意值但不绑定 |
| `name @ pattern` | 匹配 pattern，同时把整个值绑定为 name |
| `..` | 忽略剩余字段或元素 |
| `A | B` | 或模式；两边必须绑定相同名字和类型 |
| `Some(value)` | 解开单个 tuple variant |
| `Variant { field, .. }` | 解开 struct variant，字段同名简写 |
| `0..=3` | 包含端点的范围模式 |
| `value if predicate` | match guard；在模式成功后再判断 |

match arm 必须产生可统一的类型。一个 arm `return`、`break`、`continue` 或 `panic!` 时，其类型是永不返回的 `!`，可以与其他 arm 协调。

## 按值匹配还是按引用匹配

这是阅读 enum 时最重要的所有权问题：

```rust
match event {       // 通常消费 event
    Event::Text(text) => use_owned(text),
}

match &event {      // 借用 event
    Event::Text(text) => use_borrowed(text), // text 通常是 &String
}
```

Rust 的 match ergonomics 会根据被匹配值是 `T`、`&T` 还是 `&mut T` 自动调整绑定，因此现代代码很少到处写 `ref`。不确定绑定类型时，在编辑器中悬停，或暂时添加一个有明确参数类型的辅助函数让编译器检查。

`matches!` 返回 bool，但其第一个表达式仍遵守普通 move 规则：

```rust
let done = matches!(&event, Event::Finished { .. }); // 显式借用，event 仍可用
```

## `if let`、`let ... else` 与 `while let`

只关心一种形状时无需写完整 `match`：

```rust
if let Some(config) = maybe_config {
    apply(config);
}
```

需要失败时提前退出，`let ... else` 能让成功路径保持较少缩进：

```rust
let PromptOrigin::TaskCompleted { task_id } = origin else {
    return None;
};
use_task_id(task_id);
```

反复处理直到模式不再匹配时使用 `while let`：

```rust
while let Some(item) = stream.next().await {
    consume(item);
}
```

选择规则：所有业务状态都重要时用 `match`；仅执行可选动作时用 `if let`；失败必须离开当前控制流时用 `let ... else`；循环消费同一形状时用 `while let`。

## Option 与 Result 是普通 enum

```rust
enum Option<T> { None, Some(T) }
enum Result<T, E> { Ok(T), Err(E) }
```

它们的组合顺序表达不同契约：

| 类型 | 白话 |
| --- | --- |
| `Option<Result<T, E>>` | 操作可能不存在；若存在，执行可能失败 |
| `Result<Option<T>, E>` | 操作一定执行；成功结果可能没有值 |
| `Result<Vec<T>, E>` | 整体成功得到集合，或整体失败 |
| `Vec<Result<T, E>>` | 每个元素独立成功或失败 |

`transpose()` 在前两种形状间转换：

```rust
fn parse_limit(raw: Option<&str>) -> Result<Option<u32>, ParseIntError> {
    raw.map(str::parse::<u32>).transpose()
}
```

`?` 对 `Option` 和 `Result` 都能提前传播，但当前函数返回类型必须兼容。不要为了缩短代码而把本应区分的“缺失”和“失败”压成同一种状态。

高频组合子：

| 方法 | 类型形状 | 用途 |
| --- | --- | --- |
| `map` | `F<T> -> F<U>` | 转换内部成功值 |
| `and_then` | `F<T> -> (T -> F<U>) -> F<U>` | 串联也可能缺失/失败的操作 |
| `filter` | `Option<T> -> Option<T>` | 值不满足谓词时视为缺失 |
| `ok_or_else` | `Option<T> -> Result<T,E>` | 为缺失补充错误语义 |
| `map_err` | `Result<T,E> -> Result<T,F>` | 转换错误而不碰成功值 |
| `unwrap_or_else` | 容器为空/失败时计算默认值 | 默认值计算昂贵或依赖错误时 |

仓库核心路径通常避免无解释的 `unwrap()`：panic 会越过可恢复错误边界。测试中 `unwrap()` 可以表达“此条件失败即测试失败”，生产代码则先判断这是否真是不变量。

## 迭器首先是所有权选择

对 `Vec<T>`：

| 写法 | item 类型 | 集合之后能否继续用 |
| --- | --- | --- |
| `values.iter()` | `&T` | 能 |
| `values.iter_mut()` | `&mut T` | 能，借用结束后可用 |
| `values.into_iter()` | `T` | 不能，集合被消费 |
| `for value in &values` | `&T` | 能 |
| `for value in &mut values` | `&mut T` | 能 |
| `for value in values` | `T` | 不能 |

迭代器是惰性的。adapter 构造新迭代器，consumer 才驱动它：

```rust
let names: Vec<String> = tools
    .iter()                         // Iterator<Item = &Tool>
    .filter(|tool| tool.enabled())  // adapter
    .map(|tool| tool.name().to_owned())
    .collect();                     // consumer，决定目标集合
```

常见 adapter：`map`、`filter`、`filter_map`、`flat_map`、`take`、`skip`、`chain`、`enumerate`。常见 consumer：`collect`、`fold`、`find`、`any`、`all`、`sum`、`count`。

当链包含大量副作用、多个提前退出条件、异步 `.await` 或必须解释的中间状态时，普通 `for` 循环通常更清晰。迭代器不是“更 Rust”的装饰，而是描述转换管线的工具。

## collect 不只生成 Vec

`collect` 的目标由上下文决定：

```rust
let values: Vec<_> = iter.collect();
let by_name: HashMap<_, _> = pairs.collect();
let parsed: Result<Vec<_>, _> = strings.map(str::parse::<u32>).collect();
```

最后一行遇到首个 `Err` 就返回错误，否则收集全部成功值。这是 `Result` 实现 `FromIterator` 带来的行为。看到 `collect::<Result<Vec<_>, _>>()?` 时，应读成“批量转换，任一失败则提前传播”。

## 闭包与 Fn/FnMut/FnOnce

闭包根据函数体如何使用捕获值，自动实现一个或多个调用 trait：

| trait | 捕获值的使用 | 调用次数能力 |
| --- | --- | --- |
| `Fn` | 只需共享借用 | 可重复调用 |
| `FnMut` | 需要可变借用 | 可重复调用，但调用闭包需可变访问 |
| `FnOnce` | 会消费捕获值 | 至少可调用一次，可能只能一次 |

它们是包含关系：实现 `Fn` 的闭包也满足 `FnMut` 和 `FnOnce`；实现 `FnMut` 的也满足 `FnOnce`。

```rust
fn apply_twice(mut f: impl FnMut(i32) -> i32, value: i32) -> i32 {
    let first = f(value);
    f(first)
}
```

`move` 强制闭包取得捕获值所有权，但不必然让它只能调用一次；是否 `FnOnce` 取决于函数体是否把捕获值消费掉。`async move` 常用于让 Future 拥有 sender、`Arc` 或配置快照，从而能比创建它的栈帧活得更久。

## 项目阅读路径

1. 从 [`SessionCommand`](../../crates/codegen/xai-grok-shell/src/session/commands.rs) 任选三个 variant，列出每个消息的必需字段和回复语义。
2. 打开 [`ToolStreamItem`](../../crates/common/xai-tool-runtime/src/tool.rs)，解释嵌套的 `Terminal(Result<T, ToolError>)` 为什么比两个独立 Option 更可靠。
3. 打开 [`ResponseOutcome`](../../crates/common/xai-tool-protocol/src/envelope.rs)，找它的序列化实现如何穷尽匹配 variant。
4. 搜索 `.collect::<Result<Vec<_>, _>>()` 或相近写法，逐段标注 iterator item 类型。

## 动手练习

运行 [Rust 阅读 Katas](./labs/README.md) 的第 1--6 关，再处理两个 compile-fail 案例：

```sh
docs/rust-essentials/labs/check.sh
rustc --explain E0382
rustc --explain E0502
```

最后选择仓库中的一个 `match`，写下：被匹配值的类型、按值还是按引用、每个绑定的类型、是否穷尽、哪个 variant 携带错误或回复。只有能回答这五项，才算读懂了这个 match，而不只是看懂分支里的业务代码。
