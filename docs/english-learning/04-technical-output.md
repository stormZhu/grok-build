# 技术英语输出手册

阅读能力不会自动变成表达能力。本页提供与项目学习直接相关的输出模板：源码复述、学习记录、问题描述、commit message 和验证说明。

模板是起点，不是最终答案。熟练以后应逐渐减少模板痕迹。

## 1. 30 秒源码复述

先使用四句结构：

```text
X is ...
It owns/provides/coordinates ...
It communicates with Y through ...
If ..., it ...
```

以 `SessionHandle` 为例：

```text
`SessionHandle` is a clonable proxy for a session actor. It provides methods
for sending commands without exposing all actor state. The handle sends
`SessionCommand` values through an MPSC channel, and query commands can carry
a oneshot sender for the reply. If the actor is unreachable, individual
methods choose an explicit fallback or return an error.
```

检查：这里说的是“proxy”，没有把 handle 和 actor 混为一体；也没有说所有方法都使用同一个 fallback。

## 2. 解释一条调用链

使用时间顺序和边界词：

```text
First, the caller ...
It then sends ... through ...
When the actor receives ..., it ...
The result is sent back through ...
Finally, the caller awaits ...
```

避免每句都用 `Then`。可以替换为：

```text
after
before
once
while
when
as a result
```

注意 `once` 在这里表示“一旦”，不是“曾经”：

```text
Once the actor receives the command, it computes the reply.
```

## 3. 解释设计原因

可靠结构是“选择 + 反例 + 结果”：

```text
The implementation stores X separately from Y so that ...
If X were protected only by Y, ... could not ... while ...
Keeping them separate allows ... without ...
```

不要只写 `This design is better`。必须说清它防止了什么，或者允许了什么。

常用目的表达：

| 表达 | 示例 |
| --- | --- |
| `to` | The actor serializes commands to protect its state. |
| `so that` | The sender is cloned so that multiple callers can submit work. |
| `in order to` | 可以使用，但通常比 `to` 冗长 |
| `This allows ... to ...` | This allows the caller to await a typed reply. |
| `This prevents ... from ...` | This prevents the leader from unloading a busy session. |

## 4. 描述错误和不确定性

### 已被代码确认

```text
If `send` fails, the method returns `true`.
The implementation explicitly handles a closed reply channel.
```

### 合理推断但未验证

```text
This suggests that the fallback is intended to prevent premature unloads.
The path appears to be unreachable during normal shutdown, but I have not
verified it with a test.
```

### 只是不知道

```text
It is not yet clear which caller triggers this branch.
This remains unverified.
```

不要用 `maybe` 覆盖所有不确定性。区分 `may`（允许/可能）、`appears to`（从证据推断）和 `I have not verified`（明确证据边界）。

## 5. 英文学习记录

### 短版模板

```markdown
## What I studied

Today I traced ...

## What I learned

X owns ...
Y communicates with X through ...
When ..., the system ...

## Evidence

The type definition shows ...
The call site confirms ...
The test verifies ...

## Open question

I have not yet verified ...
```

### 一篇合格示例

```text
Today I traced the `SessionHandle::is_busy` request and reply. The handle
creates a oneshot channel and embeds its sender in `SessionCommand::IsBusy`.
The session actor computes whether a turn is running or an input is queued,
then sends the Boolean reply through the oneshot sender. The handle awaits the
receiver. If either channel path fails, it returns `true`, which keeps the
session resident. I have not yet checked which test covers the actor-shutdown
case.
```

这段文字包含定义、数据流、失败和未知项，没有使用复杂语法。

## 6. 提问模板

好的技术问题应包含观察、预期和证据缺口。

```text
I am tracing ...
I can see that ...
I expected ..., but ...
Which component owns ..., and which call site confirms it?
```

示例：

```text
I am tracing how a prompt reaches `SessionActor`. I can see that the handle
sends `SessionCommand::Prompt`, but I have not found where the queued input
becomes the active turn. Which function owns that transition, and which test
verifies the single-running-turn invariant?
```

这比 `I don't understand SessionActor` 更容易得到准确回答。

## 7. Commit message

常见结构：

```text
<area>: <imperative summary>
```

示例：

```text
docs: add project-based English learning materials
session: preserve error context when mapping replies
tools: cover empty progress stream in terminal test
```

标题使用祈使动词原形：`add`、`fix`、`preserve`、`clarify`、`reject`、`handle`。避免：

```text
updated docs
some fixes
fix bug
```

需要正文时说明原因和可观察行为：

```text
Explain the request/reply vocabulary with examples from SessionHandle.
Add guided exercises that require source evidence and English output.
```

## 8. 变更说明

```markdown
## Problem

The existing documentation explains the architecture in Chinese, but it does
not provide a structured way to practice technical English with the source.

## Change

Add ...

## Verification

- Checked ...
- Verified ...

## Limitations

This is a documentation-only change; no runtime behavior was exercised.
```

`Problem` 不要写成个人感受，`Change` 不要只列文件名，`Verification` 不要声称命令证明了它没有覆盖的行为。

## 9. 描述测试证据

从弱到强：

```text
The code compiles.
The focused test passed.
The test executed three cases covering ...
The integration test exercised the request/reply path and asserted ...
```

推荐包含实际 test 数量和断言对象：

```text
The command executed four tests. They cover successful replies, a closed
command channel, a dropped oneshot sender, and the conservative fallback.
They do not cover leader reconnection.
```

## 10. 自我纠错顺序

每段英文只改三遍：

1. `Technical accuracy`：actor、handle、channel 和状态 owner 是否说对；
2. `Clarity`：每个代词指向是否清楚，动作顺序是否明确；
3. `Language`：冠词、时态、搭配和自然度。

技术写作中，下面的修改比替换“高级词汇”更重要：

```text
模糊：It handles it and returns it.
清楚：The actor handles the command and returns the Boolean reply.

不准确：The handle invokes the actor function.
准确：The handle sends a command to the actor through a channel.

冗长：Due to the fact that the receiver is closed ...
简洁：Because the receiver is closed ...
```

## 11. 每周输出任务

| 日期 | 输出 | 限制 |
| --- | --- | --- |
| 第 1 次 | 介绍一个 type | 50--80 词 |
| 第 2 次 | 解释一条数据流 | 80--120 词 |
| 第 3 次 | 解释一个失败分支 | 60--100 词 |
| 周末 | 闭卷口头复述本周主题 | 2 分钟，不看稿 |

完成后只挑一段重写。反复改同一段，比每天写很多未经反馈的内容更有效。
