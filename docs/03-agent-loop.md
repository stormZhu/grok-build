# 03 · Agent Loop 与底层逻辑

## 1. 双层循环

Grok Build 的「Agent Loop」是 **两层**：

```
┌─ 外层：Session 主循环 (run_session) ─────────────────────────┐
│  select! 处理命令 / 事件 / 空闲 memory flush / dream / MCP…   │
│  收到 Prompt 命令 → 启动一次 handle_prompt（一轮用户 Turn）    │
└────────────────────────────┬──────────────────────────────────┘
                             │
┌─ 中层：用户 Turn (handle_prompt) ────────────────────────────┐
│  解析 slash / skill / goal                                   │
│  注入 system / AGENTS / 用户消息                             │
│  loop { process_conversation_turn … }  // goal harness 可续  │
└────────────────────────────┬──────────────────────────────────┘
                             │
┌─ 内层：Agentic 采样环 (process_conversation_turn) ───────────┐
│  loop {                                                      │
│    准备 tools / 可能 compact                                 │
│    build_request → sample (LLM)                              │
│    若无 tool_calls → TodoGate / interjection 检查 → 结束或续  │
│    否则 execute_tool_calls → 写回结果 → continue             │
│  }                                                           │
└──────────────────────────────────────────────────────────────┘
```

源码锚点：

| 层 | 文件 |
|----|------|
| 外层 | `xai-grok-shell/.../acp_session_impl/run_loop.rs` → `run_session` |
| 中层 | `.../turn.rs` → `handle_prompt` |
| 内层 | `.../turn.rs` → `process_conversation_turn`（约 1799 行起 `loop`） |
| 工具 | `.../tool_calls.rs` → `execute_tool_calls` |
| 采样辅助 | `.../sampler_turn.rs` |

---

## 2. 外层：`run_session`

`run_session` 持有 `SessionActor` 的长生命周期，用 **biased `tokio::select!`** 合并：

- **空闲定时器**：memory idle flush（对话变长后写记忆）  
- **dream 检查**：记忆系统的后台「做梦」整理  
- **模型切换订阅**：laziness 策略重算  
- **ChatState 事件**：conversation reset、image budget 遥测  
- **SessionEvent**：回放缓冲 flush、通知合并  
- **SessionCommand**：用户 prompt、cancel、shutdown、配置变更…  
- **Turn completion 通道**：子任务/后台完成回调  

特点：

- 文件系统 watcher（`fs_watch`）按能力按需启动  
- MCP 初始化可在后台 `ensure_mcp_tools_initialized`  
- MCP liveness dispatcher 可自动重启客户端  

外层 **不** 直接调 LLM；它只调度「开一轮 Turn」。

---

## 3. 中层：`handle_prompt`

一次用户（或合成）prompt 的编排入口。大致阶段：

### 3.1 前处理

1. 记录 telemetry / span（`session.handle_prompt`）  
2. 激活 turn guard（`is_turn_active`）  
3. 通知 **lifecycle contributors** `on_turn_start`  
4. 检测 rewind 后「完全相同文本 → regeneration」信号  
5. **直通 bash**：特殊 meta 可跳过 agent，直接跑 shell  
6. **Slash 解析**  
   - 内置：`/goal`、`/compact`、模型切换等 → 可能提前 return  
   - Skill：`/skill-name` → rewrite / 注入 skill information  
7. 发出 `TurnStarted`（内部事件 + observability + turn hooks）  

### 3.2 构建用户消息与会话前缀

由 `prompt_build.rs` 等完成：

- 大 prompt 阈值（约 25KB）**offload 到文件**，上下文只留 head+tail  
- 图片规范化（压缩/丢弃/通知）  
- AGENTS.md / rules **按 workspace vs user 分桶** 注入  
- System prompt 安装策略：  
  - 顶层 resume：**保留** 持久化的 system  
  - subagent spawn：通常 **覆盖** 为新 prompt  
  - `preserve_inherited_system`：verbatim fork 保持父 system  

### 3.3 中层 goal 环

```rust
loop {
    let round = process_conversation_turn_with_recovery(...).await;
    if !Completed || refusal { break }
    if !goal_active { break }
    match run_goal_round_end().await {
        Continue(directive) => inject_goal_continuation_message(directive),
        EndTurn => break,
    }
}
```

Goal harness 可在「模型认为说完了」之后仍注入续跑指令，实现长程目标。

### 3.4 收尾

`TurnEnded` 事件、hooks `AfterTurn`、usage drain、lifecycle `on_turn_done` / abort 路径。

---

## 4. 内层：`process_conversation_turn`（核心）

### 4.1 每轮循环前置

1. `LoopStarted { loop_index }`  
2. **Drain interjections**（用户中途消息）  
3. Flush pending skill reminders  
4. Inject monitor 事件 / MCP reminder / first-turn memory  
5. 可选 **two-pass compaction prefire**（后台 pass1）  
6. **Pre-sampling auto-compact**（token 超阈值）  
7. 解析 tool 列表：  
   - plan mode 过滤  
   - backend_search 开启时去掉本地 `web_search`  
   - structured output 非原生后端时追加 `StructuredOutput` 伪工具  

### 4.2 构建请求并采样

```
chat_state_handle.build_request(tools, memory_reminder, …)
  → ConversationRequest { items, tools, hosted_tools, model, … }
  → SamplerHandle 流式采样
  → ConversationResponse
```

采样失败路径含：context length → compact 重试、401 → AuthManager 刷新、doom-loop 信号等（`process_conversation_turn_with_recovery`）。

### 4.3 写回模型输出

- Assistant item 进入 conversation  
- 服务端已执行的 hosted tool 结果也可能以特殊 item 落库  
- 流式时已通过 ACP 推 `AgentMessageChunk`；若只有 fallback 文本则补发  

### 4.4 无 tool_calls：结束还是续跑？

| 条件 | 行为 |
|------|------|
| TodoGate 判定 todos 未完成且未达 fire 上限 | `push_system_reminder` + `continue` |
| 有 pending interjection | drain 后 `continue` |
| structured output 需校验 | 校验文本 / 或 tool 路径 |
| 否则 | `finalize_turn_bookkeeping` → `TurnOutcome::Completed` |

**TodoGate**：防止模型在 todo 列表未清时「装完」；是提示词纪律 + 运行时门闩的组合。

### 4.5 有 tool_calls：执行

1. 可选拦截 `StructuredOutput`（非原生 schema 后端）  
2. `execute_tool_calls`（`tool_calls.rs`，支持并行）  
3. 结果变 `ToolResult` 写回 ChatState  
4. 检查 `max_turns`  
5. 检查 **preflight overflow** → compact → `continue`  
6. 下一轮 sample  

#### `execute_tool_calls` 要点

- **权限**：Workspace PermissionHandle；YOLO 快路径；plan mode 另有 **edit gate**（只读，连 always-approve 也拦）  
- **Hooks**：pre/post tool  
- **Auth retry**：工具 401 时共享 `OnceCell` 去重恢复  
- **Interjection**：`get_task_output` 等 wait 类工具可被用户消息打断  
- **Plan 批准**：`exit_plan_mode` 等可挂起等客户端决策  
- **FollowupMessage**：某些工具可要求追加用户轮后 `continue`  
- **PermissionReject / Cancelled**：结束整个 turn  

---

## 5. 控制流伪代码

```text
fn handle_prompt(user_blocks):
  resolve_slash_and_skills(user_blocks)
  emit TurnStarted
  append_user_message_to_chat_state(...)
  loop:  # goal harness
    outcome = process_conversation_turn()
    if not goal_continue: break
  emit TurnEnded
  return outcome

fn process_conversation_turn():
  tools = prepare_tool_definitions()
  loop:  # agentic
    pre_sample_hooks_and_compact()
    req = chat_state.build_request(tools)
    resp = sampler.sample(req)
    push assistant(+hosted results)
    if resp.tool_calls empty:
      if todo_gate_nudge: continue
      if interjection: continue
      return Completed
    match execute_tools(resp.tool_calls):
      Cancelled/Reject -> return
      Followup -> push user; continue
      Ok -> push tool results
    if max_turns: return MaxTurns
    if overflow: compact
```

---

## 6. 生命周期与扩展钩子

### 6.1 `xai-agent-lifecycle`

Contributor 接口（数据 in / 能力注入；**不拥有 loop 控制权**）：

| Trait | 时机 |
|-------|------|
| `TurnLifecycleContributor` | turn start / done / error / abort |
| `TurnInputContributor` | 向 turn 输入注入 fragment |
| `SessionLifecycleContributor` | 会话级 |
| `CommandContributor` | 自定义命令 |

Session 在 `handle_prompt` 边界调用，保证扩展可观察但不可劫持内层循环（除非通过公开的 command 路径）。

### 6.2 Tool Protocol Turn Hooks

`xai_tool_protocol::turn_hook`：`BeforeTurn` / `AfterTurn` payload 发给观测或外部系统。

### 6.3 用户 Hooks（项目脚本）

`xai-grok-hooks` + session `hooks_plugins`：在 tool 前后跑用户配置的 shell/脚本，可 deny。

---

## 7. Subagent 与并行

- 工具 `task` / `spawn_subagent` 创建 **子 SessionActor**  
- Coordinator（`agent/mvp_agent/subagent_coordinator.rs`、`session/acp_session_impl/spawn.rs`）管理：  
  - worktree / isolation  
  - 上下文继承（verbatim vs summarized fork）  
  - usage 折回主会话  
  - 孤儿 reconcile  
- 子代理用 **`subagent_prompt.md`** 模板（更短、更聚焦）  
- `PromptAudience::Subagent` 影响 AGENTS.md / personas 注入量  

---

## 8. 失败与韧性

| 场景 | 策略 |
|------|------|
| 采样 401 | AuthManager recover + 有界 backoff（防「按 1000ⁿ 指数」历史事故） |
| Context length 错 | 触发 compact，重试采样 |
| 空响应 / content filter | 用户可见 notice；refusal 标记 |
| Doom loop | `DoomLoopSignalCollector` + recovery policy |
| max_turns | `TurnOutcome::MaxTurnsReached` |
| 工具权限拒绝 | Cancelled + category |
| MCP 未就绪 | Blocking 策略等初始化；Progressive 先跑内置工具 |

---

## 9. 与采样层的边界

Session **不** 解析 SSE 细节。它：

1. 组装 `ConversationRequest`  
2. 调 `SamplerHandle`  
3. 消费 `ConversationResponse` / 流式事件做 UI 更新  

Sampler 三层（`xai-grok-sampler`）：

1. **SamplingClient** — 原始 chunk 流  
2. **stream_*** — 归一成 `SamplingEvent`（completions / responses / messages）  
3. **SamplerActor** — 并发请求、重试、取消、metrics  

---

## 10. 调试建议

1. 打开 unified log 中 `shell.handle_prompt.*` / `shell.turn.*`  
2. 跟 `loop_index` 与 `tool_turn_count`  
3. 持久化 `chat_history` + `updates.jsonl` 做 turn 边界回放  
4. 单测目录 `acp_session_tests/turn/` 覆盖并行 tool、compact 内联、权限等  

相关文档：[04-context-management.md](./04-context-management.md)、[05-prompt-engineering.md](./05-prompt-engineering.md)。
