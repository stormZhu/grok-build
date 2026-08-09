# 22. 仓库源码实验

这些实验把“会看文档”转成“会读和改仓库”。按顺序完成；每关控制在 30--90 分钟。除明确标为改码的任务外，不需要修改源码。答案只给验证方向，不代替你的推理。

如果某一关卡在基础语法或所有权，而不是仓库业务，请先用 [Rust 阅读 Katas](./labs/README.md) 的同主题微型案例缩小问题，再回到真实源码。Katas 验证语言机制，本章验证你能否把机制用于大型异步 workspace；二者不能互相替代。

## 统一产出格式

每关都提交一页以内的学习笔记：

```text
问题：我想解释什么？
入口：从哪个公开符号开始？
数据流：值/消息从哪里到哪里？
所有权：谁长期拥有状态？哪里 clone/borrow/move？
失败流：错误、取消、channel 关闭怎样传播？
不变量：哪条性质不能被改坏？
证据：定义、调用点、测试各一个。
验证：运行了什么命令，命令能证明什么？
未知：还有哪一点只是推测？
```

## 第 1 关：解码一个流类型

入口：[`xai-tool-runtime/src/tool.rs`](../../crates/common/xai-tool-runtime/src/tool.rs)

任务：

1. 从外向内解释 `ToolStream<T>`。
2. 画出 `Progress* -> Terminal` 的允许序列。
3. 解释 `terminal_only` 为什么要求 `T: Send + 'static`。
4. 找到 `ToolStreamItem::is_terminal`，判断它是否消费 item。
5. 找一个消费 `ToolStream` 的调用点，确认流结束与 Terminal 是两件不同的事。

自检：你应能说明 `Pin`、`Box`、`dyn`、关联类型 `Item`、`Send` 各自解决的问题。查 [11 Pin](./11-pin-and-future.md) 与 [20 语法解码](./20-syntax-decoder.md)。

验证命令：

```sh
rg -n '\bToolStream\b|ToolStreamItem::Terminal' crates/common/xai-tool-runtime
cargo test -p xai-tool-runtime --test tool_streaming
```

测试过滤器可能随仓库演进而匹配零项；必须检查 Cargo 输出的实际 test 数量，不能把退出码 0 自动当作验证成功。

## 第 2 关：强类型到动态分发

入口：[`Tool`](../../crates/common/xai-tool-runtime/src/tool.rs) 与 [`ToolDispatch`](../../crates/common/xai-tool-runtime/src/dispatch.rs)

任务：

1. 列出 `Tool` 的关联类型和每个 bound 的目的。
2. 解释为什么运行时不能简单持有 `Vec<Box<dyn Tool>>`。
3. 找到 `ToolDyn`，说明类型擦除发生在哪一层。
4. 比较 `Tool::execute` 的原生 `impl Future` 与 `ToolDispatch` 上的 `#[async_trait]`。
5. 指出 JSON 值何时进入边界，何时恢复成具体 Args。

自检答案方向：`Tool` 优先给具体实现静态类型与无装箱 Future；动态 registry 需要 object-safe 表面，所以适配层擦除 Args/Output。不要只回答“因为 dyn 不支持泛型”，要指出具体违反 object safety 的签名。

验证命令：

```sh
rg -n 'trait Tool\b|trait ToolDyn\b|impl<.*ToolDyn|trait ToolDispatch' crates/common/xai-tool-runtime/src
cargo test -p xai-tool-runtime --test trait_object_safety
```

## 第 3 关：错误的三个受众

入口：[`xai-tool-runtime/src/error.rs`](../../crates/common/xai-tool-runtime/src/error.rs)

任务：追踪 `ToolError` 的 `kind`、`detail`、`source`、`details` 分别服务谁。回答：

- 哪些字段发给模型或过 wire？
- 哪些字段只供开发者诊断？
- 为什么 `source` 上有 `#[serde(skip)]`？
- `Display` 与 `Debug` 的输出目标有何不同？
- 从 `ToolError` 转成 wire error 时，哪些 variant 需要附加结构化数据？

改码练习：在本 crate 的测试中新增一个断言，证明带 `source` 的错误序列化后不泄露 source 文本，同时 `detail` 仍保留。不要修改生产行为。

最小验证：

```sh
cargo test -p xai-tool-runtime --test error_conversion
```

## 第 4 关：从 crate 门面追到实现

入口：[`xai-grok-agent/src/lib.rs`](../../crates/codegen/xai-grok-agent/src/lib.rs)

任务：

1. 列出 crate 根公开导出的类型。
2. 追 `AgentBuilder` 到真实定义，再找 `build` 或最接近的最终构造方法。
3. 只看签名，把 builder 的必需输入、可选输入和最终输出列成表。
4. 找错误 enum，说明哪些底层错误通过 `#[from]` 自动转换。
5. 在调用方找一个真实 builder 链，解释每一步是在累积配置还是执行副作用。

自检：builder 链中按值接收 `mut self` 并返回 `Self`，表示每一步转移 builder 所有权；这通常不要求堆分配，也不同于 `&mut self` builder。

## 第 5 关：Actor 的请求与回复

入口：[`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs)、[`SessionCommand`](../../crates/codegen/xai-grok-shell/src/session/commands.rs)、[`run_session`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)

先画图再读实现：

```text
caller
  | SessionCommand  (mpsc)
  v
SessionActor/run_session
  | reply            (oneshot / completion channel / notification)
  v
caller or client
```

任务：选择一个有 `respond_to` 的简单 command，不要先选 `Prompt`。追踪 sender 的创建、command 发送、actor match 分支、reply 发送和 receiver 等待。回答：

- 为什么 command enum 持有 `oneshot::Sender<T>`？
- sender 被 actor 丢弃时 receiver 得到什么？
- `UnboundedSender::send` 失败意味着什么？
- handle clone 是否复制 actor 状态？
- actor 为什么可以在许多拆分文件中拥有多个 `impl SessionActor`？

完成简单 command 后，再把 `Prompt` 作为第二遍练习。它的完成路径更复杂，不适合第一次直接进入。

## 第 6 关：读一个 `select!` 事件循环

入口：[`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)

任务：找到 `run_session` 的主要 `tokio::select!`，只提取每个 branch 的三项信息：等待什么、改变什么状态、是否可能退出循环。先不进入 branch 调用的辅助函数。

用表格整理：

| 分支输入 | 状态变化 | 退出/继续 | 取消安全关注点 |
| --- | --- | --- | --- |
| command receiver | 待填写 | 待填写 | receiver 的 `recv` |
| completion receiver | 待填写 | 待填写 | 完成消息是否会丢 |
| session event | 待填写 | 待填写 | flush/通知语义 |

然后回答：

- 是否使用 `biased;`，这对优先级意味着什么？
- 哪些分支有 `if` guard？guard 为 false 时 Future 是否被 poll？
- 分支被取消后，下次循环是否可安全重建？
- channel 全关闭时靠哪条路径退出？

查阅 [10 tokio::select!](./10-tokio-select.md) 后再做第二遍，不要先看教程里的项目解释。

## 第 7 关：锁为何在 `.await` 前释放

入口仍为 [`run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 中 `admit_task_completion_wake`。

任务：

1. 标出 `state.lock().await` 得到 guard 的位置。
2. 找到显式 `drop(state)`。
3. 列出 drop 之后发生的异步或外部操作。
4. 思考若 guard 跨过这些操作，会造成什么等待关系。
5. 区分 `parking_lot::Mutex` 与 `tokio::sync::Mutex` 在当前模块的别名。

改码练习：不改生产代码，为 `yolo_toggle_report` 增加一个表驱动测试，覆盖四种 `(was, actual)` 组合；如果现有测试已完整覆盖，则解释它为什么已满足要求，并选择另一个同文件纯 helper 写边界测试。学习中发现已有证据后不制造重复测试，也是一项工程能力。

## 第 8 关：完成一个真实小改动

从以下类别选一个当前确实存在的改进，不预设仓库一定有 bug：

- 为纯解析或转换函数补一个缺失边界测试。
- 把一个模糊错误断言改为验证类型/variant 和关键上下文。
- 为 public item 补充一个能解释不变量的 doc test 或单元测试。
- 对一个小 enum 的 match 做穷尽性测试。
- 修复一处经静态检查证实失效的文档源码锚点。

执行顺序：

1. 用 `git status --short` 确认并保护现有改动。
2. 写明行为不变量与失败示例。
3. 先补失败测试，确认它确实执行且按预期失败。
4. 做最小实现。
5. 运行 focused test、目标 crate 测试、`cargo fmt --check` 或仓库约定格式化。
6. 阅读 diff，确认没有无关格式或生成文件变化。

不要为了完成练习而改公共 API、线协议、Actor 调度或 `unsafe`。这些改动的验证范围远大于入门练习。

## 进阶挑战：一条完整工具调用链

完成前八关后，结合 [工具调用管线精读](../deep-dives/tool-call-pipeline.md)，选择一种具体工具，从模型产生 tool call 开始，追到工具执行、进度事件、Terminal 结果和重新进入采样。

必须标记四类边：

```text
普通函数调用  ---->
异步 await      ~~~~>
channel 消息    ==>>
序列化边界      -JSON->
```

最终说明：

- 哪一层仍是具体 Args/Output？
- 哪一层变成 JSON 或协议类型？
- 谁负责权限？
- 谁负责取消？
- 谁保证 Progress/Terminal 不变量？
- 错误最终作为开发日志、用户信息还是模型上下文出现？

## 毕业标准

当你能在不依赖现成导读答案的情况下完成以下任务，可以认为已经具备“阅读本仓库不费力”的 Rust 基础：

- 15 分钟内从 crate 根找到类型定义、trait impl 和两个调用点。
- 把一个五层组合类型逐层翻译，并指出所有权与线程约束。
- 画出 Actor 的 command、reply、取消和退出路径。
- 区分编译错误、业务错误、channel 关闭和超时。
- 找到现有测试真正覆盖的行为，而不是只看测试名。
- 做一个局部改动，用与风险相称的命令证明它正确。
- 对尚未验证的推测明确标为未知，而不是用直觉补全。

完成后继续以真实任务为课程：每次改动前挑一个相关专题复习，改动后把新认识补回自己的源码笔记。Rust 的熟练度会随着可验证的修改次数增长。
