# 源码精读：上下文压缩如何重写历史而不丢失运行状态

长会话的难点不只是“token 太多”。真正的问题是：模型可见历史可以被有损压缩，但系统提示、项目规则、最后一个真实问题、正在运行的任务、token 预算和恢复证据不能一起被摘要成一段不透明文本；任何选择性保留的工具尾部还必须维持调用配对。

Grok Build 因此把 compaction 拆成三个 owner：`ChatStateActor` 提供权威 conversation 和 token 状态，`SessionActor` 决定何时、用什么输入压缩并负责提交，`xai-grok-compaction` 只做与宿主无关的“生成摘要 + 组装新历史”。本文沿着一次自动压缩把这三层串起来。

## 1. 先区分三种“变短”

| 机制 | 修改对象 | 是否持久化 | 目的 |
|---|---|---|---|
| request pruning | 本次 `ConversationRequest` 的 clone | 否 | 在采样前裁剪旧的大型工具结果 |
| retained pruning | `ChatStateActor` 内的旧工具结果 | 是 | 限制长会话常驻内存和后续请求体积 |
| full-replace compaction | 整个权威 conversation | 是，并写 checkpoint | 用摘要重建长期历史 |

[`request_builder.rs`](../../crates/codegen/xai-chat-state/src/actor/request_builder.rs) 在 context 使用率超过 50% 时才对请求副本运行 `prune_conversation`。它按从后往前遇到的 `User` 数量估算 turn age：最近若干轮不动，中等年龄的大结果保留头尾，足够老的结果替换为固定占位符。

这条路径不会改变“会话发生过什么”。真正改变权威历史的是 [`mutations.rs`](../../crates/codegen/xai-chat-state/src/actor/mutations.rs) 的 retained pruning 或 `replace_conversation(..., true)`。因此调试时不能看到 request 里少了一段文本，就断言 session 已经 compact。

## 2. Owner 与总调用链

```mermaid
flowchart TD
    TURN[Session turn loop] --> CHECK[check_auto_compact_needed]
    CHECK --> STATE[ChatStateActor token snapshot]
    CHECK -->|below threshold| SAMPLE[normal sampling]
    CHECK -->|threshold reached| AUTO[run_compact_only]
    CMD[/compact optional context] --> MANUAL[run_compact]
    OVER[context overflow] --> RECOVER[compact and resubmit]

    AUTO --> INNER[run_compact_inner]
    MANUAL --> INNER
    RECOVER --> INNER
    INNER --> PREP[prepare conversation + input ladder]
    PREP --> ENGINE[xai-grok-compaction sample summary]
    ENGINE --> ASSEMBLE[build_compacted_history]
    ASSEMBLE --> VALIDATE[sanitize + validate]
    VALIDATE --> CHECKPOINT[persist checkpoint]
    CHECKPOINT --> COMMIT[ChatState replace_conversation_for_compaction]
    COMMIT --> CONTINUE[auto-continue / next sampling]
```

关键边界如下：

- [`session/compaction.rs`](../../crates/codegen/xai-grok-shell/src/session/compaction.rs) 是产品编排 owner，知道模型、认证、hooks、memory、MCP、fork、持久化和 UI notification。
- [`code_compaction/compact.rs`](../../crates/common/xai-grok-compaction/src/code_compaction/compact.rs) 是算法边界，只依赖 `CompactionSampler`、通用 item trait 和 observer；它不读取 session，也不提交状态。
- [`code_compaction/assemble.rs`](../../crates/common/xai-grok-compaction/src/code_compaction/assemble.rs) 是纯函数边界，把明确的数据部件按固定顺序组装为新历史。
- `ChatStateActor` 是 conversation 的唯一写 owner。`SessionActor` 最后必须通过 handle 发命令，不能自己修改 `Vec<ConversationItem>`。

这个拆分使算法 crate 可以用 mock item 和 mock sampler 测试，同时确保涉及真实运行状态的决策留在 shell。

## 3. 自动触发不是简单的字符串长度判断

[`actor/queries.rs`](../../crates/codegen/xai-chat-state/src/actor/queries.rs) 的核心判断可以缩成：

```rust
if exceeds_threshold(total_tokens, context_window, threshold_percent) {
    Some(AutoCompactTrigger {
        total_tokens,
        context_window,
        utilization_percent,
    })
} else {
    None
}
```

这里使用 `total_tokens`，而不是重新对字符串做一次 `bytes / 4`。原因是 `ChatState` 会吸收模型 provider 返回的真实 usage；provider 可能计算隐藏协议字段、图片、reasoning 或其它客户端估算不到的开销。

Session 层的 [`check_auto_compact_needed`](../../crates/codegen/xai-grok-shell/src/session/compaction.rs) 还要叠加策略 gate：

- 当前模型的 context window 和动态 threshold；
- debug 强制触发；
- 自动压缩是否因上一轮确定性失败被抑制；
- model switch 是否改变了窗口；
- 当前是否已有 compact/prefire 在途；
- agent 的 compaction policy 是否允许 two-pass、segments 等变体。

因此“85%”不是 compaction 的完整语义。它只是 token actor 给出的事实，是否执行仍归 Session 策略层。

## 4. 四条触发路径的差别

| 入口 | 用户是否等待 | 失败后的动作 | 成功后的动作 |
|---|---|---|---|
| `/compact` | 是 | 直接返回错误；不受 auto suppression 限制 | 通知完成，保留用户提供的压缩上下文 |
| turn 前/后自动阈值 | 通常是 | 分类并设置 suppression，避免每轮重复失败 | 写 auto-continue 信息并继续原任务 |
| sampling context overflow | 是 | compact 失败则终止或要求认证 | 重新构建 request，再采样 |
| two-pass prefire | 否，后台推测执行 pass 1 | 丢弃 cache，回退 single-pass | 阈值到达时只在关键路径执行 pass 2 |

overflow recovery 必须重新调用 ChatState 构建 request。旧 request 仍持有压缩前的 `items` clone，原样重试只会再次溢出。

## 5. `run_compact_inner` 的主干

`run_compact_inner` 很长，是因为它持有所有产品语义。按数据依赖可拆成七个阶段。

### 5.1 冻结输入事实

函数先从 ChatState 并发读取：

```text
conversation_len
system_message
full_conversation
sampling_config / context_window / model_id
```

随后记录 `tokens_before`、触发来源和 threshold，并派发 `PreCompact` hook。它不会在网络采样期间持有 ChatState 的可变借用；conversation 是一个快照，最终 commit 仍通过 actor command 串行化。

### 5.2 把摘要输入和重建状态分开

`prepare_conversation_for_summarization` 或 `prepare_conversation_for_verbatim_summarization` 生成给摘要模型看的历史。与此同时，系统提示、AGENTS.md、最后真实用户 query、recent messages、运行任务和编辑文件等状态先被结构化捕获；宿主随后可以按 compaction policy 决定哪些逐字重放、哪些只进入摘要或 reminder。

这是最重要的信息分级：

```text
必须逐字保留     system prompt / project instructions / last real query
结构化重新生成   running tasks / subagents / edited files / MCP state
允许有损压缩     较早的对话、推理和工具细节
单独留存证据     transcript/checkpoint/update log
```

如果把四类信息都交给 summarizer，自然语言摘要就会同时承担权限、状态和审计职责，任何一次遗漏都可能永久损坏会话。

### 5.3 输入降级梯子

摘要请求本身也可能超过 context window。shell 因此拥有一个从高保真到低保真的 input ladder，而公共 compaction crate 只返回 `context_overflow` 分类：

```text
verbatim input
  -> fitted input（按预算拟合）
  -> lossy/simplified input
  -> deterministic failure
```

只有 context overflow 才进入下一档。401、credit block、schema error 或其它失败不能通过删更多历史修复，必须交给各自的恢复路径。

代码给摘要输出预留固定预算，避免把输入塞满整个 window 后没有生成 summary 的空间。这对应生成模型的基本约束：

```text
input_tokens + max_output_tokens <= context_window
```

### 5.4 有界重试与退化检测

[`code_compaction/sample.rs`](../../crates/common/xai-grok-compaction/src/code_compaction/sample.rs) 负责单档输入内的 bounded retry。它区分：

- transient：同样输入稍后重发可能成功；
- deterministic：同样输入重发没有意义；
- context overflow：由宿主缩小输入后再试；
- empty/degenerate summary：网络成功，但结果不能承担历史替换。

这其实是两个嵌套循环：内层对同一输入做有界采样重试，外层只在 overflow 时切换 input ladder。把两者合成一个“最多重试 N 次”会浪费请求，也会让确定性错误产生重试风暴。

### 5.5 重建 canonical history

通用 [`assemble_compacted_history`](../../crates/common/xai-grok-compaction/src/code_compaction/assemble.rs) 支持以下顺序；这是它的纯函数契约，不是排版偏好：

```text
0  original System
1  user-info / project-layout prefix
2  AGENTS.md reminder               optional
3  last real user query             optional, wrapped in <user_query>
4  recent messages                  optional, verbatim
5  cleaned compaction summary
6  dynamic system reminder          optional
```

当宿主选择保留 recent messages 时，摘要放在它们之后以维持工作尾部顺序；动态 reminder 放在最后，让刚重建的运行状态处于模型最容易注意的位置。AGENTS.md 使用专用 item constructor，恢复时的幂等 guard 可以识别它，避免重复注入。

当前 shell 的实际调用更保守：`run_compact_inner` 先构建完整 `CompactionStateContext`，再传入 `state_context.for_compaction()`；该方法把 `recent_messages` 清空，但保留 last query、任务、子代理、MCP、todo 和编辑文件状态。也就是说，当前 Grok Build 依靠 summary 覆盖工作 transcript，并用结构化 reminder 恢复活跃状态，以换取确定的 token 回收；通用 assembler 仍保留 recent-tail 能力供其它策略使用。`use_short_prompt` 当前固定为 `false`，因此走默认 summary ordering。

组装后的 history 还会经过 `sanitize_compacted_history` 和 `validate_compacted_history`。典型不变量包括：

- 第一条仍是有效 system message；
- assistant tool call 与 tool result 不形成悬空配对；
- summary 不是空串或只含控制标签；
- 为 Messages backend 处理 reasoning 兼容性；
- 如果策略保留 recent tail，其内部顺序和 tool-call 配对仍有效。

### 5.6 先排队恢复证据，再切换权威状态

成功路径会先把 compaction request artifact、summary/segment 信息和 checkpoint 发送给 persistence actor，然后调用：

```rust
chat_state_handle
    .replace_conversation_for_compaction(compacted_history);
```

checkpoint 包含重建 conversation 所需的数据，并由 `CompactionCheckpoint` marker 在 `updates.jsonl` 中引用。恢复逻辑可由 marker 找到 checkpoint，而不是尝试从 UI delta 猜出 compact 后历史。

这里要准确区分“发送顺序”和“磁盘事务”：`persist_compaction_checkpoint` 只向 persistence channel 发送 file message 和 marker，没有等待 fsync ack。代码让恢复意图先于 ChatState replace 入队，缩小了崩溃窗口，但并不宣称 checkpoint、marker 和 `chat_history.jsonl` 是一个原子事务；磁盘失败仍要由 persistence error 和 replay 的缺失/损坏分支处理。

fork 还有一个细节：checkpoint 先保存 self-contained compacted candidate，随后 `resolve_forked_compacted_history` 才决定 live conversation 是否重新挂回 inherited prefix。普通 session 的 candidate 就是提交历史；fork 的逐字 prefix 属于运行时派生状态，不能把 checkpoint 文件机械理解为所有路径下内存 `Vec` 的逐项镜像。

### 5.7 提交阶段的重新计数与继续

提交阶段先记录 `record_compaction_at(prompt_index)` 并排队 checkpoint，随后 replace conversation；成功后再清除可恢复的 suppression/prefire cache、更新计数和通知，并在自动路径恢复原 turn。auto-continue 不是把旧 future 从暂停点继续轮询，而是用 compact 后的 ChatState 构建新的采样输入。

## 6. Token reseed 为什么使用比例

本地 estimator 与 provider usage 往往不同。假设压缩前：

```text
本地估算 estimate_at_last_response = 80k
provider total_tokens              = 100k
压缩后本地 base_estimate           = 20k
```

简单把 20k 设为新 total 会立刻丢掉 provider 侧约 25% 的相对开销；简单加回固定 20k overhead，又可能在历史缩短后严重高估。`replace_conversation(..., true)` 使用比例 reseed：

```text
ratio = pre_replace_total / estimate_at_last_response
new_total = min(base_estimate * ratio, pre_replace_total)
```

例中得到 `20k * 1.25 = 25k`。上限保证 compaction 不会在 UI 上让使用量反而升高。随后下一次 provider usage 会重新校准真实值。

这是估算系统常见的 calibration 思路：保留测量模型的相对误差，而不是假设误差是固定常数。

## 7. Two-pass prefire：用 cache 换关键路径延迟

two-pass 把 conversation 切成大 prefix 和较小 tail：

```text
pass 1: prefix -> NOTE_1
pass 2: NOTE_1 + tail -> final summary
```

默认在 auto threshold 之前若干百分点触发后台 pass 1。等真正到阈值时，如果 NOTE_1 已完成，用户只等待 pass 2。

缓存不能只按 session ID 命中。`AsyncCompactionCache` 至少记录：

- `prefix_len`；
- prefix 内容 fingerprint；
- `model_slug`；
- NOTE_1 文本和 pass-1 latency。

pass 2 前重新检查当前 conversation 的 prefix 和 model。edit、rewind、fork 分支或 model switch 都会使 cache stale；此时宁可回退 single-pass，也不能用旧世界的 NOTE_1 重写新历史。

这与 CPU cache coherence 是同一类问题：加速结果只有在它依赖的输入版本仍有效时才能复用。这里用 fingerprint 做低成本版本证明，用 single in-flight guard 避免重复 speculative work。

prefire 和正式 compact 共享 `CompactCancelGate`。gate 使用 holder count 而不是 bool，因为两个 scope 可能重叠；最后一个 holder 退出后，下次独立 compact 才安装新的 `CancellationToken`。

## 8. Suppression 是防重试风暴的状态机

自动压缩失败后不能每个 turn 都无条件再打一遍请求。[`compaction_config.rs`](../../crates/codegen/xai-grok-shell/src/session/compaction_config.rs) 定义了不同寿命的 suppression：

| 状态 | 原因 | 清除条件 |
|---|---|---|
| `SUPPRESS_TURN` | 暂时性其它错误 | 下一 turn |
| `SUPPRESS_STICKY` | size/schema 等同输入不可修复 | 成功 compact、rewind 或 context budget 改变 |
| `SUPPRESS_UNTIL_SUCCESS` | credit/spending block | 一次正常模型请求成功 |
| `SUPPRESS_AUTH` | 401/认证过期 | login 或 token refresh |

credit 和 auth 必须分开。等待一次模型 200 可以证明 credit block 消失，但当历史已经溢出时，要求先完成正常采样再清除 auth suppression 会形成死锁；认证恢复必须由凭据事件解锁。

手动 `/compact` 不受 auto suppression 限制，用户可以在外部条件修复后主动重试。

## 9. Fork、rewind 与 inherited prefix

fork session 可能固定继承 parent prefix。压缩后若无条件重新拼回整个 prefix，结果可能仍超过 auto threshold，形成“压缩成功但马上再次压缩”的循环。

`resolve_forked_compacted_history` 会估算重新固定 prefix 后的 token 使用；若仍有压力，就释放 inherited prefix，采用已经从完整 conversation 生成的 self-contained summary。这里的取舍是：fork lineage 的逐字继承弱于 session 可继续运行的不变量。

rewind 则必须清理 two-pass cache 和可能的 sticky size suppression，因为历史缩短后，旧的“不可压缩”判断已经不再成立。

## 10. 理论视角：compaction 是受约束的有损编码

可以把原历史记为 `H`，摘要为 `S = f(H)`，运行状态为 `R`，不可压缩规则为 `P`。新历史不是简单的 `S`，而是：

```text
H' = assemble(P, last_query(H), recent(H), S, reconstruct(R))
```

系统希望同时满足：

```text
tokens(H') << tokens(H)
policy(H') == policy(H)
active_state(H') ~= active_state(H)
tool_integrity(H') == valid
recover(checkpoint, replay metadata) ~= H'
```

自然语言摘要只能近似第二、三项，因此代码把 policy 和 active state 移出有损通道，用逐字复制和结构化重建保证。checkpoint 与 replay metadata 让第五项不必从模型输出或 UI delta 反推；fork prefix 等派生状态仍需要恢复逻辑共同解释。

## 11. 调试路线

| 症状 | 第一处证据 | 下一步 |
|---|---|---|
| 达到阈值但未 compact | suppression 状态、threshold、context window | `check_auto_compact_needed` span |
| compact 请求反复 overflow | attempt artifact 的 input ladder 档位 | summary reserve、verbatim 配置 |
| compact 后规则消失 | assembled history 中 ProjectInstructions item | AGENTS reminder 提取和幂等 guard |
| compact 后立刻再次触发 | reseed token、fork prefix projected tokens | provider ratio、prefix release |
| edit/rewind 后摘要内容过时 | prefire fingerprint/model | cache invalidation |
| 重启后回到 compact 前 | `CompactionCheckpoint` marker 和文件 | persistence actor 写入顺序 |
| tool result 断配 | validation/sanitize 结果 | recent tail 的边界选择 |

建议至少记录 `session_id`、trigger、pre/post tokens、context window、input ladder attempt、outcome、prefire hit/stale 和 checkpoint ID。只看最终 summary 文本不足以定位触发、提交或恢复问题。

## 12. 测试与阅读顺序

推荐按以下顺序阅读：

1. [`request_builder.rs`](../../crates/codegen/xai-chat-state/src/actor/request_builder.rs)：先分清 request pruning。
2. [`actor/queries.rs`](../../crates/codegen/xai-chat-state/src/actor/queries.rs)：理解 token threshold 事实。
3. [`session/compaction.rs`](../../crates/codegen/xai-grok-shell/src/session/compaction.rs)：追 trigger、input ladder、assembly 和 commit。
4. [`code_compaction/compact.rs`](../../crates/common/xai-grok-compaction/src/code_compaction/compact.rs)：看通用采样边界。
5. [`code_compaction/assemble.rs`](../../crates/common/xai-grok-compaction/src/code_compaction/assemble.rs)：固定新历史顺序。
6. [`actor/mutations.rs`](../../crates/codegen/xai-chat-state/src/actor/mutations.rs)：理解 replace 与 token reseed。
7. [`helpers/replay.rs`](../../crates/codegen/xai-grok-shell/src/session/helpers/replay.rs)：从 checkpoint 反向验证恢复。

关键测试应覆盖四类证据：

- compaction crate 的 pure assembly、empty/degenerate 和 retry classification；
- ChatState 的 threshold、provider overhead reseed、sampling config 保留；
- shell 的 input ladder、two-pass cache stale、cancel 和 suppression；
- replay 的 checkpoint 缺失、损坏、跨 compaction rewind。

可先运行现有缩小实验：

```bash
cargo run --manifest-path docs/rust-essentials/labs/async-demos/Cargo.toml \
  --bin mini_context_compaction
```

实验只演示 request-only pruning 与 full replace 的语义差别；生产代码中的 provider 校准、two-pass、checkpoint 和恢复仍要以上述源码测试为准。

## 13. 阅读练习

1. 为什么 summary 不能直接替换为一条 `System` message？列出会被破坏的至少三个 owner 边界。
2. 构造 provider estimate 比例为 1.4 的例子，计算压缩后的 reseed token，并解释上限何时生效。
3. 在 pass 1 完成后插入一次 rewind，指出哪些字段能阻止 stale NOTE_1 被应用。
4. 若新增一种 compaction backend，哪些逻辑应进入 `CompactionSampler`，哪些必须留在 `SessionActor`？
5. 设计一个 crash test，分别让进程停在 checkpoint message 入队前、文件写入后 marker 写入前，以及 ChatState replace 后，写出每个恢复结果的期望。
