# 02 · 设计架构

## 1. 总览

Grok Build 采用 **分层 + Actor** 架构：UI、会话运行时、采样、工具、工作区、压缩引擎彼此解耦，通过 Handle / 协议边界通信。

```
┌─────────────────────────────────────────────────────────────────┐
│  Clients                                                         │
│  · TUI (xai-grok-pager)  · Headless CLI  · IDE via ACP/stdio    │
└───────────────────────────────┬─────────────────────────────────┘
                                │ ACP / Leader socket / 进程内
┌───────────────────────────────▼─────────────────────────────────┐
│  Agent Host — xai-grok-shell                                     │
│  · Leader (可选多客户端)  · SessionActor  · Auth  · Extensions   │
│  · MCP  · Subagent 协调  · Goal harness  · Persistence           │
└───┬─────────────┬─────────────┬─────────────┬───────────────────┘
    │             │             │             │
    ▼             ▼             ▼             ▼
 ChatState    Sampler      ToolBridge     Workspace
 (conversation) (HTTP LLM)  (tools/MCP)   (FS/VCS/perm)
    │             │             │             │
    ▼             ▼             ▼             ▼
 sampling-    API backends   tool-runtime  sandbox /
 types                       tool-protocol  worktree
                ▲
                │
         Compaction engine
         (xai-grok-compaction)
```

---

## 2. 进程与入口模型

### 2.1 三种运行形态

| 形态 | 入口 | 说明 |
|------|------|------|
| **交互 TUI** | `xai-grok-pager-bin` → pager app | UI 进程；常通过 **Leader** 或直连驱动 agent |
| **Headless** | `run_headless` | 单次/脚本式 prompt，无 TUI |
| **stdio Agent** | `run_stdio_agent` | 标准输入输出上的 ACP，供 IDE 嵌入 |

Leader（`xai-grok-shell::leader`）负责：

- 本机 socket 注册与能力协商  
- 多客户端附着同一 workspace  
- 版本偏斜检测（`leader_is_older_than`）  

### 2.2 二进制 composition root

`xai-grok-pager-bin` **只做拼装**：解析 CLI、鉴权/更新、调用 pager 或 shell API。业务逻辑不在 bin 里堆砌。

---

## 3. 分层职责

### L0 — 类型与协议（无 I/O 或仅线协议）

| Crate | 职责 |
|-------|------|
| `xai-grok-sampling-types` | ConversationItem / Request / Response / StopReason / HostedTool |
| `xai-tool-types` | 工具侧共享类型 |
| `xai-tool-protocol` | Computer Hub JSON-RPC 方法、帧、hook、session_event |
| `xai-grok-config-types` / `xai-hooks-plugins-types` | 配置与插件类型 |
| `xai-grok-workspace-types` | 工作区事件与共享类型 |

设计意图：**下游状态/UI 可不拉起完整 shell**。

### L1 — 引擎与运行时

| Crate | 职责 |
|-------|------|
| `xai-grok-sampler` | 三层采样：Client 流 → Stream 事件 → SamplerHandle/Actor |
| `xai-chat-state` | Conversation 权威状态、build_request、usage ledger |
| `xai-grok-compaction` | full-replace / intra / inter 压缩算法与 prompt |
| `xai-tool-runtime` | Tool trait、Dispatch、流式输出、通知 |
| `xai-agent-lifecycle` | Turn/Session 生命周期贡献者（扩展不抢 loop 控制权） |
| `xai-prompt-queue` | 提示队列线类型（shell ↔ pager） |

### L2 — 产品能力

| Crate | 职责 |
|-------|------|
| `xai-grok-agent` | AgentBuilder、prompt 组装、插件/skills 发现 |
| `xai-grok-tools` | 内置工具实现 + ToolBridge + 归一化/版本 |
| `xai-grok-workspace` | FS、VCS、权限、checkpoint、hub server |
| `xai-grok-mcp` | MCP 客户端与 SSE 等传输 |
| `xai-grok-memory` | 跨会话记忆 |
| `xai-grok-sandbox` | OS 级隔离 |
| `xai-grok-hooks` / `xai-grok-plugin-marketplace` | 钩子与市场 |
| `xai-fast-worktree` / `xai-hunk-tracker` | 隔离 worktree 与编辑 hunk 追踪 |

### L3 — 宿主与 UI

| Crate | 职责 |
|-------|------|
| `xai-grok-shell` | SessionActor、ACP 会话、扩展、鉴权、远程同步… |
| `xai-grok-pager` / `pager-render` | TUI 与渲染管线 |
| `xai-grok-pager-bin` | 可执行入口 |

---

## 4. Actor 拓扑（Session 内部）

```
                 SessionCommand (prompt/cancel/…)
                          │
                          ▼
┌──────────────────────────────────────────────────────────┐
│                    SessionActor                           │
│  run_session: select! { idle timers | events | cmds… }   │
│                                                          │
│  · agent: Agent (prompt + ToolBridge + policies)         │
│  · chat_state_handle ──► ChatStateActor                  │
│  · sampler_handle    ──► SamplerActor                    │
│  · permissions       ──► Workspace PermissionHandle      │
│  · mcp_state / extensions / goal / plan_mode / memory    │
└──────────────────────────────────────────────────────────┘
          │ session_notification (ACP updates)
          ▼
     Gateway → Client (Pager / IDE)
```

**原则**：

1. **单写者**：conversation 只由 ChatStateActor 突变  
2. **Session 编排**：何时 sample、何时 tool、何时 compact 由 Session 决定  
3. **扩展不抢控制流**：`xai-agent-lifecycle` 的 contributor 只收事件/注 fragment  

---

## 5. 数据面 vs 控制面

### 数据面（一次推理）

```
Conversation + ToolSpec[] + HostedTool[]
        → ConversationRequest
        → Sampler (HTTP stream)
        → ConversationResponse (text + tool_calls + usage)
        → 执行 tool_calls → ToolResult 写回 conversation
        → 循环直到无 tool_calls
```

### 控制面

- 权限决策（YOLO / plan mode / 用户批准）  
- 取消与 interjection（用户中途插话）  
- 模型切换、lazy 策略、goal harness  
- MCP 初始化策略（Blocking / Progressive）  
- 持久化（chat history / updates.jsonl / feedback）  

---

## 6. 关键设计决策

### 6.1 Shell 与 Agent 分离

`xai-grok-agent` 从 shell 抽出：任何宿主（TUI 会话、batch runner）都能 `AgentBuilder::build()` 得到可移植的工具+prompt 包。  
Shell 只负责 **会话生命周期与 I/O**。

### 6.2 Compaction 与 Host 解耦

`xai-grok-compaction` 通过 trait 缝合（`CompactionItem`、`CompactionSampler`、`ItemTokenCounter`、Observer），**不依赖** chat-state 或 sampling-types。  
Grok Build 用 **code_compaction / full-replace**；同 crate 也承载 Grok Chat 的 intra/inter 策略，便于共享。

### 6.3 工具多源统一

```
内置 tools (grok_build / codex / opencode 风格)
    + MCP tools (server__tool 命名)
    + Hosted tools (服务端 web_search 等)
        → 统一 ToolDefinition / ToolSpec 发给模型
        → 执行路径在 ToolBridge / MCP dispatcher / backend
```

### 6.4 Prompt 模板与运行时配置绑定

模板通过 `tools.by_kind.*` 引用 **ToolKind**，而不是写死工具名；换 toolset / 兼容模式时提示词仍正确。  
详见 [05-prompt-engineering.md](./05-prompt-engineering.md)。

### 6.5 兼容生态

- Claude Code：skills / rules / 导入路径  
- Cursor：plan mode 工具与部分 user template  
- Codex / OpenCode：工具实现移植  

兼容开关集中在 `CompatConfig` 与 agent definition。

---

## 7. 目录布局（仓库）

```
crates/
  codegen/     # 产品主闭环（shell/pager/agent/tools/…）
  common/      # 可共享叶依赖（compaction/tool-runtime/…）
  build/       # 构建辅助（proto）
prod/mc/       # 生产侧代理类型等
third_party/   #  vendored Mermaid 等
docs/          # 本分析文档
bin/protoc     # protoc 启动器
```

---

## 8. 与「典型 Agent 框架」对比

| 维度 | 常见 Python Agent 框架 | Grok Build |
|------|------------------------|------------|
| 运行时 | 同步/async 脚本 | 长生命周期 SessionActor + TUI |
| 工具 | 装饰器函数 | 强类型 Tool trait + registry + 版本化 |
| 上下文 | 简单截断 | full-replace 摘要 + pruning + image budget |
| 协议 | 自定义 | ACP 标准 + 内部 Hub 协议 |
| 权限 | 少 | Workspace 权限 + plan mode + sandbox |
| 扩展 | 插件脚本 | Skills / Plugins / Hooks / Lifecycle contributors |

---

## 9. 下一步

- 内层循环细节 → [03-agent-loop.md](./03-agent-loop.md)  
- 上下文与压缩 → [04-context-management.md](./04-context-management.md)  
- 接口清单 → [06-interfaces.md](./06-interfaces.md)  
- 文件地图 → [07-module-map.md](./07-module-map.md)  
