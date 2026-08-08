# 2. 类型建模、模式匹配与迭代

Rust 倾向将状态编码在类型中。读项目代码时，`enum` 往往比布尔标志更能说明合法状态集合。

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

## 关键工具

- 用 `struct` 表达一组必须同时存在的数据；用 newtype 包住容易混淆的基础类型。
- 用 `enum` 表达互斥状态，`match` 会强制处理新增变体。
- `Option<T>` 表示值可能缺失，`Result<T, E>` 表示操作可能失败；不要用空字符串或魔法数替代。
- `if let`、`let ... else` 适合只关心一个分支；有多个业务分支时优先 `match`。
- 迭代器链应在转换清晰时使用；需要多个中间状态、异步操作或有副作用时，普通 `for` 循环通常更易读。

## 项目中的锚点

- [`ChatStateCommand`](../../crates/codegen/xai-chat-state/src/commands.rs#L56) 是 Actor 命令枚举：变体同时携带命令所需数据与回复通道。
- [`GoalStatus`](../../crates/codegen/xai-grok-shell/src/session/goal_tracker.rs#L64) 用枚举维护可序列化的业务状态。
- [`ToolOutput`](../../crates/codegen/xai-grok-tools/src/types/output.rs#L624) 展示工具输出的类型化表达与 trait 实现。

### 仓库代码摘录：命令的类型化回复

[`ChatStateCommand`](../../crates/codegen/xai-chat-state/src/commands.rs#L56) 让不同变体携带不同的数据和确认语义：

```rust
PushUserMessage { item: ConversationItem },
PushUserMessageAndAck {
    item: ConversationItem,
    reply: oneshot::Sender<()>,
},
```

这比 `kind + Option<reply>` 更可靠：发送者无法构造“要求确认却没有回复通道”的非法组合。

## 闭包与集合

闭包会捕获周围变量：只读捕获实现 `Fn`，可变捕获实现 `FnMut`，移动捕获可能只能调用一次而成为 `FnOnce`。遇到 async 闭包报错，先检查被捕获的值是否已移动，以及 future 是否需要 `Send`。

`iter()` 产生 `&T`，`iter_mut()` 产生 `&mut T`，`into_iter()` 消耗集合并产生 `T`。这是所有权错误最常见的来源之一。

## 阅读检查点

为一个现有 `match` 新增假想变体，列出编译器会迫使你更新的分支；这正是枚举对状态机维护的价值。
