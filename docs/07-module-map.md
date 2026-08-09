# 07 · 模块地图

按**研究问题**索引到源码路径（相对仓库根）。文件行数会变，以目录/模块名为准。

---

## 1. 入口与二进制

| 路径 | 说明 |
|------|------|
| `crates/codegen/xai-grok-pager-bin/src/main.rs` | 主二进制：TUI / headless / leader / stdio 分发 |
| `crates/codegen/xai-grok-pager/src/` | TUI 应用、CLI 参数、滚动与输入 |
| `crates/codegen/xai-grok-pager-render/src/` | 渲染、主题、markdown 显示 |
| `crates/codegen/xai-grok-pager/docs/user-guide/` | 产品用户手册 |

---

## 2. Agent 运行时（Shell）

| 路径 | 说明 |
|------|------|
| `crates/codegen/xai-grok-shell/src/lib.rs` | shell 模块树 |
| `.../agent/app.rs` | `run_headless` / `run_leader` / `run_stdio_agent`；长期宿主与 relay/IPC bridge |
| `crates/codegen/xai-grok-pager/src/headless.rs` | `grok -p` 的单轮 ACP 驱动器与输出 reducer |
| `crates/codegen/xai-grok-shell/src/agent/relay.rs` | headless/Leader 的 WebSocket relay、重连与鉴权恢复 |
| `.../agent/mvp_agent/` | ACP agent、session lifecycle、subagent coordinator |
| `.../session/acp_session.rs` | SessionActor 根与 path 子模块挂载 |
| `.../session/acp_session_impl/run_loop.rs` | **外层** `run_session` |
| `.../session/acp_session_impl/turn.rs` | **中/内层** `handle_prompt` / agentic loop |
| `.../session/acp_session_impl/tool_calls.rs` | 工具执行管道 |
| `.../session/acp_session_impl/sampler_turn.rs` | 采样准备、鉴权重试 |
| `.../session/acp_session_impl/prompt_build.rs` | 用户消息、rules、大 prompt offload |
| `.../session/acp_session_impl/reminders.rs` | 运行时 reminder |
| `.../session/acp_session_impl/spawn.rs` | 子代理 spawn |
| `.../session/acp_session_impl/goal*.rs` | Goal harness |
| `.../session/acp_session_impl/mcp.rs` | MCP 会话集成 |
| `.../session/compaction.rs` | 主机侧 compact 触发与编排 |
| `.../session/persistence.rs` | 会话落盘 |
| `.../session/chat_persistence.rs` | `ChatState` 到 `PersistenceMsg` 的适配 |
| `.../session/storage/mod.rs` | storage trait、提交错误语义、replay/rewind filter |
| `.../session/storage/jsonl/mod.rs` | JSONL session 目录、append、durability、恢复 |
| `.../session/storage/mod.rs` 的 `chat_rebuild` 模块 | 从 `updates.jsonl` 重建 `chat_history.jsonl` |
| `.../session/replay_events.rs` | 客户端通知事件和 replay flush ack |
| `.../agent/update_chunk_merge.rs` | 内存 `ReplayBuffer` 的 chunk 合并与限频 |
| `.../session/plan_mode.rs` | Plan mode 状态 |
| `.../session/slash_commands.rs` | 斜杠命令 |
| `.../auth/` | OAuth / OIDC / API key / refresh |
| `.../leader/` | Leader 控制面 |
| `.../extensions/` | 产品扩展面（memory、hooks、skills、worktree…） |
| `.../tools/` | shell 侧 tool context / bridge 粘合 |

---

## 3. Agent 定义与 Prompt

| 路径 | 说明 |
|------|------|
| `crates/codegen/xai-grok-agent/README.md` | 对外文档级说明 |
| `.../src/agent.rs` | `Agent` 类型 |
| `.../src/builder.rs` | `AgentBuilder` |
| `.../src/config.rs` | `AgentDefinition` frontmatter |
| `.../src/discovery.rs` | 定义发现 |
| `.../src/compaction.rs` | `CompactionPolicy` |
| `.../src/system_reminder.rs` | ReminderPolicy |
| `.../src/prompt/` | 模板、AGENTS、skills、user_message |
| `.../src/prompt/context.rs` | `PromptContext`、audience、extend/full system render |
| `.../src/prompt/agents_md.rs` | AGENTS/Claude/rules 发现、排序、去重 |
| `.../src/prompt/user_message.rs` | workspace/rules/skills/MCP 首轮 user preamble |
| `.../templates/prompt.md` | **主系统提示源** |
| `.../templates/subagent_prompt.md` | 子代理提示源 |
| `.../templates/apply_patch_prompt.md` | patch 工具提示 |
| `.../scripts/encrypt_templates.py` | 模板混淆脚本 |
| `.../src/plugins/` | 插件发现/安装/信任 |

---

## 4. 会话状态与采样

| 路径 | 说明 |
|------|------|
| `crates/codegen/xai-chat-state/src/lib.rs` | ChatState 总览图 |
| `.../actor/mod.rs` | ChatStateActor |
| `.../actor/request_builder.rs` | **build_request**：prune/image/memory |
| `.../actor/mutations.rs` | 状态变更 |
| `.../usage.rs` | UsageLedger |
| `.../compaction_*.rs` | 与 compact 相关的 transcript 工具 |
| `crates/codegen/xai-grok-sampling-types/src/conversation.rs` | ConversationItem/Request/Response |
| `.../types.rs` / `messages.rs` | 后端相关类型 |
| `crates/codegen/xai-grok-sampler/src/lib.rs` | 三层 API 说明 |
| `.../client.rs` | SamplingClient |
| `.../stream/` | 流解析 completions/responses/messages |
| `.../actor/` | SamplerActor |
| `.../actor/request_task.rs` | 一次请求的 attempt、retry、cancel 与 completion oneshot |
| `.../events.rs` / `retry.rs` | 统一 `SamplingEvent`、纯 retry 决策 |
| `.../retry.rs` / `doom_loop.rs` | 重试与死循环信号 |

---

## 5. 上下文压缩

| 路径 | 说明 |
|------|------|
| `crates/common/xai-grok-compaction/src/lib.rs` | 引擎总览与 re-export |
| `.../code_compaction/` | **Grok Build full-replace** |
| `.../code_compaction/prompt.rs` | 摘要提示 |
| `.../code_compaction/templates/` | 摘要模板文本 |
| `.../code_compaction/assemble.rs` | 历史重建 |
| `.../code_compaction/compact.rs` | 编排 sample→clean→assemble |
| `.../intra_compaction/` / `inter_compaction/` | Grok Chat 策略（对照学习） |
| `.../select.rs` | tail-keep 选择（chat 用） |
| `.../reminder.rs` | compact 后 system-reminder 格式 |

---

## 6. 工具与协议

| 路径 | 说明 |
|------|------|
| `crates/common/xai-tool-runtime/src/` | Tool trait、dispatch、stream |
| `crates/common/xai-tool-protocol/src/` | Hub JSON-RPC、hooks、session_event |
| `crates/common/xai-tool-types/src/` | 共享工具类型 |
| `crates/codegen/xai-grok-tools/src/bridge.rs` | ToolBridge |
| `.../registry/` | 注册与 finalize |
| `.../types/resources.rs` | `Resources`、`Params`/`State`、ephemeral capability 与资源持久化 key |
| `.../implementations/grok_build/` | 主工具实现 |
| `.../implementations/codex/` | Codex 移植 |
| `.../implementations/opencode/` | OpenCode 移植 |
| `.../implementations/skills/` | Skill 工具 |
| `.../reminders/` | 工具结果 reminder |
| `.../types/` | ToolDefinition、ToolKind、compat |
| `crates/codegen/xai-grok-mcp/src/` | MCP 客户端 |

---

## 7. Workspace / 权限 / 沙箱

| 路径 | 说明 |
|------|------|
| `crates/codegen/xai-grok-workspace/src/lib.rs` | 工作区总模块 |
| `crates/codegen/xai-grok-workspace/src/session/` | per-session toolset、文件状态、checkpoint、git/jj 边界 |
| `crates/codegen/xai-grok-workspace/src/worktree/` | Git/JJ worktree 创建、复制、应用、删除与回收 |
| `.../permission/` | 权限决策 |
| `.../file_system/` | FS 抽象与索引 |
| `.../session/` | workspace session、git/jj、file_state |
| `.../worktree/` | worktree 操作 |
| `crates/codegen/xai-fast-worktree/` | 快速 overlay worktree |
| `crates/codegen/xai-hunk-tracker/` | 编辑 hunk 追踪 |
| `crates/codegen/xai-grok-sandbox/` | OS 沙箱 |
| `crates/codegen/xai-codebase-graph/` | 代码图/索引相关 |

---

## 8. 扩展与产品功能

| 路径 | 说明 |
|------|------|
| `crates/codegen/xai-grok-memory/` | 跨会话记忆 |
| `crates/codegen/xai-grok-hooks/` | 用户 hooks 运行时 |
| `crates/codegen/xai-agent-lifecycle/` | 进程内 lifecycle contributors |
| `crates/codegen/xai-prompt-queue/` | 提示队列线类型 |
| `crates/codegen/xai-grok-plugin-marketplace/` | 插件市场 |
| `crates/codegen/xai-grok-config/` | 配置加载 |
| `crates/codegen/xai-grok-shell/src/agent/config.rs` | typed Config、配置 warning、CLI/env/remote runtime resolution |
| `crates/codegen/xai-grok-shell/src/util/config/resolve/` | 按功能拆分的 feature、权限、MCP、compaction、worktree resolver |
| `crates/codegen/xai-grok-models/` | 默认模型表 |
| `crates/codegen/xai-grok-telemetry/` | 遥测与 unified log |
| `crates/codegen/xai-grok-update/` | 自更新 |
| `crates/codegen/xai-interjection-core/` | 插话核心 |

---

## 9. 构建与其它

| 路径 | 说明 |
|------|------|
| `Cargo.toml` | workspace（生成物，只读） |
| `rust-toolchain.toml` | Rust 版本钉扎 |
| `clippy.toml` / `rustfmt.toml` | lint/格式 |
| `crates/build/xai-proto-build/` | protoc 辅助 |
| `bin/protoc` | protoc 启动器 |
| `third_party/` | Mermaid 等 vendored |
| `THIRD-PARTY-NOTICES` | 第三方与移植声明 |

---

## 10. 按学习任务速查

| 我想… | 打开 |
|-------|------|
| 从编译跑起来 | `README.md` + `xai-grok-pager-bin` |
| 看主 system prompt | `xai-grok-agent/templates/prompt.md` |
| 看模型上下文如何装配 | `AgentBuilder` + `PromptContext` + `prompt_build.rs` |
| 跟一次用户消息 | `turn.rs` `handle_prompt` |
| 跟 tool call | `tool_calls.rs` + `bridge.rs` |
| 看发给模型的 JSON 形态 | `sampling-types/conversation.rs` + `request_builder.rs` |
| 跟一次采样、流式 chunk 和 retry | `sampler_turn.rs` + `xai-grok-sampler/{actor,stream,retry}.rs` |
| 看压缩如何换历史 | `code_compaction/*` + `session/compaction.rs` |
| 看 session 如何落盘/恢复 | `session/persistence.rs` + `session/storage/jsonl/` |
| 看 replay 如何处理 rewind 分支 | `session/storage/mod.rs` + `session/acp_session_impl/rewind.rs` |
| 看客户端 chunk 为什么会合并 | `agent/update_chunk_merge.rs` + `session/replay_events.rs` |
| 接 IDE | ACP：`mvp_agent/acp_agent.rs` + user-guide `15-agent-mode.md` |
| 加一种工具 | `xai-tool-runtime` trait + `implementations/grok_build` + registry |
| 加一种 Agent 人格 | `.grok/agents/*.md` 或 `AgentBuilder` |
| 理解权限 | `workspace/permission` + plan gate in `tool_calls.rs` |
| 理解文件恢复 | `workspace/src/session/file_state.rs` + `checkpoint.rs` + `xai-hunk-tracker` |
| 理解 worktree 隔离 | `workspace/src/worktree/mod.rs` + `xai-fast-worktree` + shell `spawn.rs` |
| 理解子代理 | `spawn.rs` + `subagent_prompt.md` + `subagent_coordinator` |

---

## 11. 架构文档交叉引用

- [01 学习指南](./01-learning-guide.md)  
- [02 架构](./02-architecture.md)  
- [03 Agent Loop](./03-agent-loop.md)  
- [04 上下文](./04-context-management.md)  
- [05 提示词](./05-prompt-engineering.md)  
- [06 接口](./06-interfaces.md)  
- [09 开发者实战手册](./09-contributor-playbook.md)
- [10 Workspace Crate 全目录](./10-crate-catalog.md)
- [11 Rust × Agent 源码实验](./11-guided-exercises.md)
- [12 Rust × Agent × 项目术语索引](./12-glossary.md)
- [13 分阶段贡献项目](./13-contribution-projects.md)
- [14 项目学习地图](./14-project-learning-map.md)
- [15 运行时调试手册](./15-runtime-debugging-playbook.md)
- [源码精读：用户消息流](./deep-dives/message-flow.md)
- [源码精读：工具调用](./deep-dives/tool-call-pipeline.md)
- [源码精读：权限与沙箱](./deep-dives/permissions-and-sandbox.md)
- [源码精读：Workspace 状态与 Worktree 生命周期](./deep-dives/workspace-state-and-worktree-lifecycle.md)
- [源码精读：Pager 渲染](./deep-dives/pager-rendering.md)
- [源码精读：MCP 生命周期](./deep-dives/mcp-lifecycle.md)
- [源码精读：MCP Dispatcher](./deep-dives/mcp-dispatcher.md)
- [源码精读：会话持久化与重放](./deep-dives/persistence-and-replay.md)
- [源码精读：采样生命周期](./deep-dives/sampling-lifecycle.md)
- [源码精读：Prompt 装配](./deep-dives/prompt-assembly.md)
- [源码精读：扩展与生命周期](./deep-dives/extensions-and-lifecycle.md)
- [源码精读：Leader 控制面](./deep-dives/leader-control-plane.md)
- [源码精读：认证与模型选择](./deep-dives/authentication-and-model-resolution.md)
- [源码精读：配置加载与运行时解析](./deep-dives/configuration-and-runtime-resolution.md)
- [源码精读：子代理与 Workflow](./deep-dives/subagents-and-workflows.md)
- [源码精读：可观测性与 Trace Timeline](./deep-dives/observability-and-trace-timeline.md)
- [源码精读：贡献者工作流](./deep-dives/contributor-workflow.md)
- [源码精读：资源注入与能力边界](./deep-dives/resources-and-capability-injection.md)
- [源码精读：宿主模式与运行入口](./deep-dives/host-modes-and-entrypoints.md)
- [源码精读：取消、超时与关闭](./deep-dives/cancellation-and-shutdown.md)
