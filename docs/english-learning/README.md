# Grok Build 项目英语学习路线

这套材料面向正在通过 Grok Build 学习 Rust、异步编程和 Agent 运行时，同时希望提高技术英语能力的开发者。它不是一门脱离源码的英语课：所有阅读、词组和输出任务都尽量绑定本仓库的 README、源码注释、类型名和设计语义。

## 学习目标

完成这套材料后，应能做到：

1. 不逐词翻译，也能读懂大部分源码注释和项目文档；
2. 识别 Rust 与系统设计中常见的英文句型，例如 ownership、failure、fallback 和 lifecycle 描述；
3. 用 5--10 句英文准确解释一个类型、一条调用链或一次失败；
4. 写出简洁的学习记录、commit message 和变更说明；
5. 建立可持续复习的项目词组库，而不是积累孤立单词。

这套路线优先训练技术阅读和技术表达。听力、日常口语和考试词汇不在当前范围内。

## 材料索引

| 材料 | 用途 | 建议产出 |
| --- | --- | --- |
| [01-reading-source-code.md](./01-reading-source-code.md) | 学习怎样拆解 README、doc comment、函数名和错误信息 | 标注句子骨架，写一段英文释义 |
| [02-project-phrases.md](./02-project-phrases.md) | 掌握本项目高频词组和工程句型 | 每次选择 5 个词组造句 |
| [03-guided-practice.md](./03-guided-practice.md) | 完成由浅入深的真实仓库阅读任务 | 保存答案和证据，不只写翻译 |
| [04-technical-output.md](./04-technical-output.md) | 练习英文复述、学习笔记、commit 和问题描述 | 每次学习结束写 5--10 句 |
| [05-session-actor-lesson.md](./05-session-actor-lesson.md) | 用当前正在学习的 Session Actor 完成第一节综合课 | 英文解释 Handle、Actor 和 channel |
| [06-key-words-in-context.md](./06-key-words-in-context.md) | 基于仓库脚本统计关键单词，并结合源码原句学习多义词、搭配和项目语义 | 每次选择 5 个词做原文复述 |
| [词汇分析脚本](./scripts/README.md) | 扫描 Rust 注释和英文 Markdown，输出词频、文件覆盖率与原文上下文 | 重跑统计或查询指定单词 |

项目概念不熟悉时，先查中文 [项目术语表](../12-glossary.md)；Rust 语法卡住时，查 [Rust 必备知识](../rust-essentials/README.md)。本目录解决的是“如何读和表达”，不重复讲完整架构或 Rust 语法。

## 每次学习的 45 分钟闭环

### 1. 技术理解，20 分钟

选择一个足够小的问题，例如：

- `SessionHandle::is_busy` 怎样得到 Actor 的回复？
- `ToolStream` 为什么必须以一个 `Terminal` 结束？
- 一条 command 发送失败意味着什么？

先用中文确认输入、owner、边界和失败路径。技术含义不清楚时，不要强迫自己直接用英文推理。

### 2. 英文输入，10 分钟

阅读 5--20 行英文材料，可以是：

- 源码中的 `//!` 和 `///` 注释；
- 根 [README](../../README.md) 的一个小节；
- 一个函数签名及其错误信息；
- 测试名称和断言消息。

只标出三类内容：

```text
主干：谁做什么
限定：在什么条件下、为了什么
项目词组：可以在其他场景复用的表达
```

### 3. 英文输出，10 分钟

合上原文，用 3--5 句英文回答：

```text
What is it?
What does it own or do?
How does it communicate?
What happens when it fails?
```

### 4. 纠错与复习，5 分钟

只记录三类修改：

- `Accuracy`：技术含义是否准确；
- `Grammar`：是否存在影响理解的语法错误；
- `Naturalness`：工程师通常会怎样说得更简洁。

不要抄写整段改写。只保留一条自己下次能够复用的规则和 3--5 个词组。

## 六周建议路线

| 周次 | 技术主题 | 英语重点 | 完成证据 |
| --- | --- | --- | --- |
| 1 | Repository 与 crate 地图 | 名词短语、`is/contains/provides` | 60 秒介绍项目 |
| 2 | `SessionHandle` 与 command | 主动语态、channel 动词 | 解释一次 request/reply |
| 3 | `SessionActor` 与 `select!` | 时序、条件和 failure | 画图并用英文讲解 |
| 4 | Tool runtime | invariant、类型边界 | 解释 `Progress* -> Terminal` |
| 5 | Sampling 与 context | lifecycle、retry、fallback | 写 120 词技术摘要 |
| 6 | 小改动与验证 | commit、test evidence、limitations | 写一份英文变更说明 |

每周学习 3 次即可。一次只增加 5 个主动词组；读懂但暂时不会使用的词，不必全部加入复习表。

## 学习记录模板

建议在个人学习记录中为每次练习保留以下内容：

```markdown
# Topic

## Technical question

我想解释的问题：

## Source evidence

- Definition:
- Caller:
- Failure path:

## Five useful phrases

1.
2.
3.
4.
5.

## My explanation

用 5--10 句英文完成。

## Corrections

- Accuracy:
- Grammar:
- Naturalness:

## One-sentence recall

第二天闭卷写一句话。
```

## 怎样判断是否进步

不要用“今天背了多少单词”衡量。每两周重复解释同一个主题，并观察：

- 阅读同一段注释是否减少查词次数；
- 能否先说出句子主干，再补充限定条件；
- 是否开始使用 `own`、`route`、`propagate`、`fall back to` 等准确动词；
- 能否区分代码事实、推断和尚未验证的假设；
- 英文输出是否更短、更明确，而不只是更复杂。

达到“可以用简单英文准确解释”以后，再追求复杂句式。技术英语首先是准确的信息传递。
