# 04 · 上下文管理

## 1. 问题定义

长会话 Agent 会遇到：

1. **模型 context window** 上限  
2. **HTTP body** 上限（尤其 inline base64 图片）  
3. **噪声**：过旧 tool 输出占用大量 token  
4. **KV-cache 友好性**：频繁改历史前缀会毁掉缓存命中  

Grok Build 用多层机制组合解决，而不是单一截断。

```
┌────────────────────────────────────────────────────────────┐
│  请求构建时（每轮 sample 前）                               │
│  ChatState::build_conversation_request                     │
│  · Image budget 驱逐                                       │
│  · Tool-result pruning（>50% 利用率）                      │
│  · Memory reminder 注入                                    │
│  · Conversation integrity repair                           │
└────────────────────────────────────────────────────────────┘
┌────────────────────────────────────────────────────────────┐
│  会话级 full-replace Compaction                            │
│  达阈值 / 溢出错误 / 手动 /compact                         │
│  · 可选 memory flush → 摘要采样 → 重建 history             │
└────────────────────────────────────────────────────────────┘
┌────────────────────────────────────────────────────────────┐
│  跨会话 Memory                                             │
│  idle flush / dream / memory_search 工具                   │
└────────────────────────────────────────────────────────────┘
```

---

## 2. ChatState：会话权威源

**Crate**：`xai-chat-state`

```
SessionActor ──Command──► ChatStateActor
     ▲                         │
     │     ChatStateEvent      │
     └─────────────────────────┘
```

### 2.1 核心状态

- `conversation: Vec<ConversationItem>`  
- `sampling_config`（model、context_window、api_backend…）  
- `prompt_index` / token 累计  
- `UsageLedger`（主 agent loop 与 subagent 分账）  

### 2.2 ConversationItem（`xai-grok-sampling-types`）

| 变体 | 含义 |
|------|------|
| `System` | 系统提示 |
| `User` | 用户输入（可带 `synthetic_reason`） |
| `Assistant` | 模型输出（含 tool_calls） |
| `ToolResult` | 客户端工具结果 |
| （hosted） | 服务端 agentic 工具轨迹，用于重放与 Responses 连续性 |

`SyntheticReason` 标记合成消息（如 ProjectInstructions、各类 reminder），便于 compact 去重与 UI 隐藏策略。

### 2.3 `build_conversation_request` 流水线

源码：`actor/request_builder.rs`

1. **Integrity**：调用前 handler 已 `ensure_conversation_integrity`（去重 tool result、修 dangling tool_call）  
2. **Memory 持久化注入**（可选）：把 reminder 写进真实 conversation，而不只 request clone  
3. **Image compaction**（接近 ~50MB body）：  
   - 驱逐最旧 inline 图片到 reclaim 水位  
   - **故意不每轮都做**：改前缀会 bust KV-cache  
4. **Prune tool results**（token 使用 > 50% context）：  
   - soft-trim（头尾 + `[…trimmed…]`）  
   - 或 hard-clear 为 `[Tool result omitted — too old]`  
5. 组装 `ConversationRequest { items, tools, hosted_tools, … }`  

---

## 3. Compaction：Full-Replace（Grok Build 主策略）

### 3.1 设计哲学

Grok Build **不** 采用「只保留 tail」为主策略（那是 Grok Chat 的 intra/inter）。  
而是：

> 用 LLM **总结整段对话**，再 **整表替换** conversation 为：  
> `新 System + 摘要 + 用户原始查询（及必要 reminder）`

实现：

- 算法 / prompt：`xai-grok-compaction::code_compaction`  
- 触发 / 持久化 / 重试 / two-pass：`xai-grok-shell/src/session/compaction*.rs`  

### 3.2 策略配置（Agent 侧）

`xai-grok-agent::CompactionPolicy`：

| 字段 | 默认 | 含义 |
|------|------|------|
| `auto_compact_threshold_percent` | 85 | 占用 context 百分比触发 |
| `compact_model` | None | 专用压缩模型 |
| `memory_flush_enabled` | false | compact 前是否 memory flush |
| `wall_clock_budget_secs` | 300 | 摘要生成墙钟上限 |
| `two_pass_enabled` | false | 预取 pass1 + 边界 pass2 |

共享默认阈值常量：`DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT`。

### 3.3 触发点（Session）

| 时机 | 说明 |
|------|------|
| 采样前 `check_auto_compact_needed` | 常规阈值 |
| 采样错误 context length | 恢复路径 |
| tool 后 `check_preflight_overflow` | 工具结果撑爆前 |
| 模型切换 `maybe_compact_on_model_switch` | 新窗口更小 |
| 用户 `/compact` | 手动，可带 user_context |
| two-pass prefire | 接近阈值时后台先总结前缀 |

### 3.4 摘要提示词

两种 `SummaryPromptKind`：

1. **Structured** — 长模板 `full_replace_summary_prompt.txt`（分节：用户目标、进度、路径、错误、下一步…）  
2. **SelfSummary** — 短 `SELF_SUMMARIZATION_PROMPT`（给「继任助手」的压缩说明）  

可选 `/compact <text>` 的 user_context 会拼进提示。

压缩后 system 可切换为精简版：

```text
COMPACT_SYSTEM_PROMPT =
  "You are an AI coding agent. You operate in a workspace..."
```

（`xai-grok-agent` 的 `Agent::compact_system_prompt()`）

### 3.5 组装结果

`assemble_compacted_history` 产出新 items；session 层负责：

- 替换 ChatState conversation  
- 持久化  
- 维护 prompt_index / 回放边界  
- 注入 active-agent-state 的 `<system-reminder>`（todo、plan 等，见 `reminder` 模块）  

### 3.6 Trait 缝合（为何可复用）

| Trait | 角色 |
|-------|------|
| `CompactionItem` / `CompactionRole` | 抽象一条消息 |
| `CompactionItemFactory` | 重建消息 |
| `ItemTokenCounter` | 宿主可信 token 计数 |
| `CompactionSampler` | 调 LLM 摘要 |
| `FullReplaceObserver` | metrics |

Shell 实现这些 trait，把「产品状态」接到「纯算法」。

---

## 4. 请求级 Pruning vs 会话级 Compaction

| | Pruning | Compaction |
|--|---------|------------|
| 粒度 | 旧 tool 输出 | 整会话语义摘要 |
| 是否调 LLM | 否 | 是 |
| 对前缀 | 局部改写 | 整体替换 |
| 频率 | 每轮 build 可能 | 阈值/溢出/手动 |
| 目标 | 省 token、保近期细节 | 释放大量窗口、保任务连续性 |

两者叠加：compact 前可能已 prune；compact 后 conversation 变短，prune 压力下降。

---

## 5. Memory 系统

**Crate**：`xai-grok-memory` + session `memory_*` + tools `memory_search` / `memory_get`

### 路径

1. **Idle flush**：空闲定时器 + conversation 变长 → 模型写记忆  
2. **Dream**：周期性整理记忆库  
3. **First-turn inject**：新会话/恢复时把相关记忆作为 reminder  
4. **Compact 前 flush**（若 policy 开启）：先固化再摘要，防信息丢失  
5. **工具主动检索**：prompt 中 `<memory>` 段引导模型调用  

记忆与 compaction 的关系：**记忆是跨会话长期存储；compaction 是单会话窗口管理**。

---

## 6. 大 Prompt 与 Skill 预算

`prompt_build.rs` 常量（量级）：

| 常量 | 作用 |
|------|------|
| `LARGE_PROMPT_THRESHOLD` ≈ 25_000 | 触发 offload |
| head/tail + `ELISION_MARKER` | 请求内只留两端 |
| `SKILL_INLINE_BUDGET` ≈ 4_000 | skill 指令独立预算 |
| offload 文件 | 模型可用 read_file 读全文 |

避免「用户贴了整个仓库说明」直接撑爆第一轮。

---

## 7. Token 与 Usage

- **估计**：`estimate_*_tokens`（chat-state）用于触发阈值  
- **账单**：响应 `usage` 写回 ledger；主 loop 与 subagent 分计  
- **TurnSpanTotals**：单 turn 多 model call 累加 input/output/cache_read  
- **Incomplete**：cancel / 后台子代理未结束时 fail-closed 或 report-only 标记  

`UsageDrainOutcome` 区分：ledger 染色 vs 仅报告不完整。

---

## 8. 图像上下文

- 规范化：分辨率/体积上限、损坏丢弃、通知 UI  
- 线协议：仅 `data:` 或 http(s) URL（`file://` 必须 inline）  
- body 级 image eviction 与 conversation 级 text compact 独立  

---

## 9. Prompt Queue 与 Interjection

- **Prompt queue**（`xai-prompt-queue` + session）：用户在 busy 时排队下一条  
- **Interjection**：turn 中途插入用户消息，内层 loop 在 tool 间隙 drain，避免「整轮结束后才看见」  

这两者也是上下文管理的一部分：**控制何时把新 user token 并入 history**。

---

## 10. 研究切入点

| 问题 | 从哪读 |
|------|--------|
| 何时触发 compact？ | `session/compaction.rs`, `turn.rs` pre-sample 检查 |
| 摘要长什么样？ | `code_compaction/prompt.rs`, `templates/*.txt` |
| 如何重建 history？ | `code_compaction/assemble.rs` |
| prune 规则？ | `chat-state/actor/request_builder.rs` |
| AGENTS 注入是否重复？ | `prompt_build.rs` `conversation_has_project_instructions` |
| two-pass？ | `CompactionPolicy.two_pass_enabled` + prefire 逻辑 |

下一步：[05-prompt-engineering.md](./05-prompt-engineering.md)。
