# 项目英语分级练习

这些练习使用仓库内的真实英文材料。每关同时训练技术理解与英文表达，建议按顺序完成。不要只提交翻译；每关都要求源码证据和自己的英文输出。

## 统一答题格式

```markdown
## Source

文件与符号：

## Sentence skeleton

主语：
谓语：
宾语或补语：
条件/原因/目的：

## Technical evidence

- Definition:
- Call site:
- Failure or invariant:

## My answer

英文回答。

## New phrases

最多 5 个。

## What remains uncertain

尚未被源码或测试验证的内容。
```

## Level 1：认识项目

### 练习 1：一句话说明产品

材料：根 [README](../../README.md) 开头两段。

任务：

1. 找出描述产品类别的中心词；
2. 列出它可以执行的四类动作；
3. 区分 `interactively`、`headlessly` 和 `embedded in editors`；
4. 不照抄原文，用 40--60 个英文单词介绍 Grok Build。

必须使用：

```text
terminal-based
codebase
run in ... mode
```

自检：产品名或类别应出现在第一句，不要从仓库历史开始介绍。

### 练习 2：读仓库结构表

材料：[README 的 Repository layout](../../README.md#repository-layout)。

任务：从下列 crate 中选三个，分别写一句英文职责说明：

- `xai-grok-pager`
- `xai-grok-shell`
- `xai-grok-tools`
- `xai-grok-workspace`

句型：

```text
X is responsible for ...
X provides ...
X acts as the boundary between ...
```

进阶：再写一句说明三个 crate 怎样协作，必须使用 `while` 或 `through`。

### 练习 3：理解构建说明

材料：[README 的 Development](../../README.md#development)。

用英文回答：

1. Why should development commands target a specific crate?
2. Which command checks types without producing a release binary?
3. When should you run `cargo fmt --all`?

答案必须区分 `check`、`test`、`clippy` 和 `fmt`，不要统一写成 `build commands`。

## Level 2：读类型和注释

### 练习 4：`SessionHandle`

材料：[handle.rs](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 顶部 module comment，以及 `SessionHandle` 定义。

任务：

1. 拆解 `the Clone + Send proxy for interacting with a session actor`；
2. 解释 `Callers hold a SessionHandle` 中 `hold` 的含义；
3. 从字段中找出 handle 与 actor 通信的证据；
4. 用 5 句英文回答以下问题：

```text
What is a SessionHandle?
Does cloning it create another actor?
How does it communicate with the actor?
```

禁止使用不准确表达：

```text
The handle is the actor.
Clone copies the whole session.
The handle calls the actor directly.
```

### 练习 5：`SessionCommand`

材料：[commands.rs](../../crates/codegen/xai-grok-shell/src/session/commands.rs) 顶部 module comment 和 `SessionCommand` 定义。

任务：

1. 找出 `defines the message protocol used to drive a session actor` 的主干；
2. 解释 `was extracted from ... to keep ... focused on behaviour` 的目的关系；
3. 选择一个带 `respond_to` 的 command，说明 request/reply 流程；
4. 用英文区分 `Prompt` command 和一个简单查询 command。

提示：`drive` 在这里不是“驾驶汽车”，而是通过消息使状态机工作。

### 练习 6：保守 fallback

材料：`SessionHandle::is_busy` 的 doc comment 和实现。

填空后再脱离模板复述：

```text
The method sends ______ to the actor.
The actor replies through ______.
If the actor is unreachable, the method falls back to ______.
This is conservative because ______.
```

进阶问题：Why would returning `false` be dangerous here?

## Level 3：读设计理由和不变量

### 练习 7：Tool stream invariant

材料：[tool.rs](../../crates/common/xai-tool-runtime/src/tool.rs) 顶部注释以及 `ToolStream`、`ToolStreamItem`。

任务：

1. 解释 `canonical streaming entry point`；
2. 将 `[Progress(_)*, Terminal(Result<T, ToolError>)]` 写成自然英文；
3. 区分 stream 结束和出现 terminal item；
4. 用 60--90 个英文单词解释 `terminal_only` 和 `with_progress` 分别适合什么情况。

必须使用：

```text
emit
at most / exactly one
invariant
```

技术自检：`arbitrarily many Progress items` 包括零个；`exactly one Terminal` 不等于仅允许一个 item。

### 练习 8：Cancellation safety

材料：[ToolBridge](../../crates/codegen/xai-grok-tools/src/bridge.rs) 的 `# Cancellation Safety` 注释。

先画因果链：

```text
bash command is running
  -> registry lock is held
  -> user cancels
  -> cancellation needs terminal access
  -> terminal is stored separately
```

再用英文回答：

```text
Why would storing the terminal only behind the registry lock make cancellation harder?
```

要求使用 `while`、`without` 和 `so that` 中至少两个。

### 练习 9：Generated root manifest

材料：根 [README](../../README.md) 中关于 `Cargo.toml` 的 Important 提示，以及中文 [Cargo workspace 指南](../rust-essentials/06-cargo-workspace.md) 用于确认技术含义。

任务：写一段给新贡献者的英文提醒，必须包括：

- 哪个文件是 generated；
- 为什么日常不应手改；
- 应优先改哪里；
- 什么时候需要上游生成源参与。

字数限制：80--120 词。目标是准确，不追求复杂词汇。

## Level 4：跟踪行为

### 练习 10：一个 Actor request/reply

材料：

- [handle.rs](../../crates/codegen/xai-grok-shell/src/session/handle.rs)
- [commands.rs](../../crates/codegen/xai-grok-shell/src/session/commands.rs)
- [run_loop.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs)

选择 `IsBusy` 或另一个简单 query command，找到：

1. oneshot channel 创建；
2. command 发送；
3. Actor 的 match branch；
4. reply 发送；
5. receiver await；
6. 两条失败路径。

最终产出：一张数据流图和 100--150 词英文解释。每个动作尽量使用准确动词：`create`、`embed`、`send`、`receive`、`compute`、`reply`、`await`、`fall back`。

### 练习 11：事件循环

材料：[run_loop.rs](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 中的 `run_session`。

第一次只找 `tokio::select!` 的输入源，不深入辅助函数。为每个分支写一句：

```text
This branch waits for ...
When ..., it ...
The loop continues/exits because ...
```

最终用英文回答：Why is an actor loop a useful owner of mutable session state?

注意：不要把“单线程执行域”绝对化成“整个程序只有一个线程”。描述当前 session 的 ownership 模型即可。

### 练习 12：一次工具调用

结合 [工具调用管线](../deep-dives/tool-call-pipeline.md)，选择一个具体工具，标出四种边界：

```text
function call
async await
channel message
JSON/wire boundary
```

用 150--200 词英文说明：

- typed arguments 在哪里；
- 何时变成 JSON；
- 谁 dispatch tool；
- progress 和 terminal 怎样返回；
- error 最终面向 user、model 还是 developer。

这是综合练习。无法从源码确认的内容必须使用 `may`、`appears to` 或明确写 `remains unverified`。

## Level 5：工程输出

### 练习 13：解释一次测试结果

选择一个你实际运行过的 focused test，写 80--120 词：

```text
I ran ...
It executed ... tests.
It verifies that ...
It does not cover ...
Therefore, the result provides evidence for ..., but not for ...
```

禁止只写 `All tests passed, so the change is correct.`

### 练习 14：写变更说明

选择一次真实的小改动，按以下顺序写英文说明：

1. `Problem`：当前行为或文档缺口；
2. `Change`：具体修改；
3. `Reasoning`：为什么这样改；
4. `Verification`：实际运行的检查；
5. `Limitations`：未覆盖内容。

控制在 150 词以内。删掉不影响审查的学习过程描述。

## 自检参考，而不是标准答案

完成一关后，用下面的问题审查答案：

- 是否先说清对象，再说实现细节？
- 主语是否明确，还是大量使用含糊的 `it`？
- `call`、`send`、`emit`、`return` 是否与真实边界一致？
- 是否把可能性写成了确定事实？
- 是否说明 fallback、error 或 cancellation 的外部行为？
- 是否至少引用了一个定义和一个行为证据？
- 能否删掉 20% 的词而不损失信息？

答案不要求接近母语者，但必须让另一个工程师能够据此找到代码并验证结论。
