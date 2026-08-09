# 怎样阅读这个项目的英文源码

技术英语阅读的第一目标不是把英文变成中文，而是恢复代码作者表达的结构：对象是谁、做了什么、受到什么条件限制、失败时怎样处理。

## 1. 先读代码，再读英语

源码中的英文通常同时依赖代码事实。阅读一段 doc comment 时，按以下顺序进行：

1. 看被注释的是 module、type、field 还是 function；
2. 看签名中的输入、输出、`async`、可见性和泛型约束；
3. 再读注释，判断它是在描述职责、原因、不变量还是失败行为；
4. 到调用方验证自己的理解。

例如 [SessionHandle] 的注释：

```rust
/// Handle for interacting with a session actor.
#[derive(Clone)]
pub struct SessionHandle {
    pub cmd_tx: mpsc::UnboundedSender<SessionCommand>,
    // ...
}
```

不要先逐词翻译。代码已经给出三个证据：

- 它是一个公开 `struct`；
- 它可以 `Clone`；
- 它包含发送 `SessionCommand` 的 sender。

因此 `for interacting with` 描述的是用途：这个 handle 是外部与 actor 交互的入口。

源码锚点：[handle.rs](../../crates/codegen/xai-grok-shell/src/session/handle.rs)

## 2. 找到句子主干

长注释先只保留“主语 + 谓语 + 宾语/补语”。

原文：

> Callers hold a `SessionHandle` and send `SessionCommand` messages via the internal channel.

主干：

```text
Callers hold a handle.
Callers send messages.
```

补充信息：

```text
什么 handle：SessionHandle
什么 messages：SessionCommand messages
通过什么：via the internal channel
```

最后再恢复完整含义：调用方持有 `SessionHandle`，并通过内部 channel 发送 `SessionCommand` 消息。

### 常见主干动词

这个仓库的注释经常用以下动词表达组件职责：

| 动词 | 阅读时应追问 |
| --- | --- |
| `own` | 谁负责这份状态的生命周期和修改？ |
| `hold` / `store` | 保存的是权威值、共享引用还是 snapshot？ |
| `send` / `receive` | 通过普通调用、channel 还是协议？ |
| `route` / `dispatch` | 根据什么选择下游实现？ |
| `build` / `assemble` | 输入来自哪些来源？ |
| `emit` / `publish` | 谁消费输出，顺序是否重要？ |
| `retain` / `preserve` | 哪条信息或不变量不能丢？ |
| `prevent` / `avoid` | 注释在解释哪一种反例？ |
| `fall back` | 主路径为什么会失败，替代行为是否保守？ |

## 3. 拆解名词短语

技术英语会把大量限定信息放到名词前面。应从最右侧的中心词向左读。

```text
session actor command enum
                       ^ 中心词：enum
               ^ command enum
         ^ actor command enum
^ session actor command enum
```

它表示“供 session actor 使用的 command enum”，不是四个并列概念。

本项目中的例子：

| 名词短语 | 中心词 | 含义 |
| --- | --- | --- |
| `session actor command enum` | `enum` | Session Actor 的命令枚举 |
| `turn-end signals snapshot` | `snapshot` | turn 结束时的 signals 快照 |
| `client-visible event ordering` | `ordering` | 客户端可观察到的事件顺序 |
| `model-facing content blocks` | `blocks` | 提供给模型的内容块 |
| `single-item terminal stream` | `stream` | 只含一个终态 item 的流 |
| `per-session OS thread` | `thread` | 每个 session 独享的系统线程 |
| `idle-unload decision` | `decision` | 空闲时是否卸载的决定 |

练习时，在中心词下划线，再从右向左添加限定。不要按中文语序从左到右猜。

## 4. 识别压缩后的关系

源码注释为了简洁，经常省略重复成分。

### 分词短语

```text
the message returned to the caller
```

`returned to the caller` 修饰 `message`，相当于：

```text
the message that is returned to the caller
```

### `if any`

```text
Current running prompt/turn id, if any.
```

表示“当前正在运行的 prompt/turn id，如果存在的话”。代码中的 `Option<String>` 与这个限定互相印证。

### `used by` / `used for`

```text
Used by the leader's idle-unload decision.
```

这是省略主语的被动表达，完整形式是：

```text
This method is used by the leader's idle-unload decision.
```

### 破折号或括号

```text
Falls back to `true` (conservative: keep the session resident, never unload)
if the actor is unreachable.
```

括号解释为什么 `true` 是保守选择，不是新的控制流分支。

## 5. 区分四种注释功能

阅读时给每段注释标一个标签，比完整翻译更有效。

### `RESPONSIBILITY`：职责

```text
Owns the registry and dispatches tool calls via `call_new_tool()`.
```

回答“它做什么”。

### `RATIONALE`：原因

```text
The `terminal` field is stored separately from the registry lock to enable
cancellation during tool execution.
```

回答“为什么这样设计”。遇到 `to enable`、`so that`、`because`、`otherwise` 时重点关注。

### `INVARIANT`：不变量

```text
Stream invariant: at most arbitrarily many `Progress` items, ending in
exactly one `Terminal`.
```

回答“无论实现怎样变化，什么必须成立”。这类句子应进入测试和 code review 依据。

### `FAILURE`：失败语义

```text
Falls back to `true` if the actor is unreachable.
```

回答“异常情况下外部能观察到什么”。不要把它弱化成“可能有错误”。

## 6. 用标点理解逻辑

| 标记 | 常见作用 | 阅读策略 |
| --- | --- | --- |
| 冒号 `:` | 给出解释、列表或结论 | 右侧通常展开左侧概念 |
| 分号 `;` | 连接关系密切的完整信息 | 分成两句读，再判断因果或对照 |
| 括号 `()` | 补充定义、例子或理由 | 先跳过，理解主干后再放回 |
| 反引号 | 代码符号或字面值 | 回到定义确认，不按普通英文猜 |
| `/` | 并列近义概念或复合标签 | 结合类型判断，不默认等于“或者” |
| `->` | 数据流、转换或允许序列 | 明确每个节点是状态、值还是步骤 |

## 7. 阅读 Rust 签名时怎样用英语

以 `is_busy` 为例：

```rust
pub async fn is_busy(&self) -> bool
```

可以分层说：

```text
`is_busy` is a public asynchronous method.
It borrows the handle immutably.
It eventually returns a Boolean value.
```

不要说 `It returns a bool immediately`，因为 `async fn` 返回的是 future，调用方需要 `.await` 才得到最终布尔值。

常用表达：

| Rust 结构 | 推荐英文 |
| --- | --- |
| `&self` | It borrows the receiver immutably. |
| `&mut self` | It takes a mutable borrow of the receiver. |
| `self` | It takes ownership of the receiver. |
| `Option<T>` | The value may be absent. / It returns an optional T. |
| `Result<T, E>` | It returns either a value or an error. |
| `Arc<T>` | The value is shared through reference counting. |
| `mpsc::Sender<T>` | It can send T values to a single receiving side. |
| `oneshot::Sender<T>` | It carries exactly one reply value. |
| `T: Send + 'static` | T can be moved across task/thread boundaries and holds no non-static borrow. |

这里的英文解释必须服从 Rust 语义。例如 `'static` 不等于“这个值永远存在”。

## 8. 阅读错误和 fallback

遇到失败逻辑时，写出四格表：

| 问题 | `SessionHandle::is_busy` 的答案 |
| --- | --- |
| 正常结果是什么？ | Actor 回复当前是否有 running 或 queued work。 |
| 什么会失败？ | command 发送失败，或 reply sender 在回复前被丢弃。 |
| 调用方看到什么？ | 方法返回 `true`。 |
| 为什么这样选择？ | 避免错误地卸载可能仍有工作的 session。 |

然后用英文连接：

```text
Under normal conditions, the actor computes whether the session has running
or queued work. If the command cannot be sent or the reply is dropped, the
method falls back to `true`. This conservative default prevents the leader
from unloading a session by mistake.
```

## 9. 三遍阅读法

### 第一遍：不查词

只回答：这段材料在讲职责、数据流、原因还是失败？圈出认识的代码符号。

### 第二遍：拆结构

标出主干、限定条件和逻辑连接。最多查 3--5 个阻碍理解的词组。

### 第三遍：回到代码验证

找定义、一个调用方以及一个失败分支或测试。把“我认为”改成“源码表明”。

最后闭卷写 3 句英文，不回看原文。复述比重新阅读更能暴露薄弱点。

## 10. 本篇练习

阅读 [ToolBridge 注释](../../crates/codegen/xai-grok-tools/src/bridge.rs)，完成：

1. 找到两个 `RESPONSIBILITY` 句子；
2. 找到一个 `RATIONALE` 段落；
3. 解释 `dispatch`、`registry lock` 和 `without blocking on the lock`；
4. 用不超过 80 个英文单词回答：What does `ToolBridge` do, and why is its terminal handle stored separately?

完成证据不是中文翻译，而是：源码行、你的英文答案，以及一次技术准确性纠错。

[SessionHandle]: ../../crates/codegen/xai-grok-shell/src/session/handle.rs
