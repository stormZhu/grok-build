# 05 · 提示词工程

## 1. 总览：提示词不是单文件

Grok Build 的「模型所见」是 **分层组装** 的结果：

```
┌─ System ──────────────────────────────────────────────────┐
│  Base / Subagent / Full 自定义模板 (MiniJinja)            │
│  + Agent definition body (extend 模式追加)                │
│  + role / persona / 可选条件块                            │
└───────────────────────────────────────────────────────────┘
┌─ 前缀 User / Synthetic ───────────────────────────────────┐
│  AGENTS.md / rules / personas system-reminder             │
│  Skills 信息、memory 上下文、MCP/monitor 提醒             │
└───────────────────────────────────────────────────────────┘
┌─ 对话历史 ────────────────────────────────────────────────┐
│  User / Assistant / ToolResult / Hosted …                 │
│  工具结果内嵌 <system-reminder>                           │
└───────────────────────────────────────────────────────────┘
┌─ 每轮动态 ────────────────────────────────────────────────┐
│  TodoGate nudge、goal continuation、interjection、compact 后 reminder │
└───────────────────────────────────────────────────────────┘
```

主实现 crate：`xai-grok-agent`（组装）+ `xai-grok-shell`（运行时注入）。

---

## 2. 模板源文件

路径：`crates/codegen/xai-grok-agent/templates/`

| 文件 | 用途 |
|------|------|
| `prompt.md` | 主 Agent 基础系统提示 |
| `subagent_prompt.md` | 子代理精简系统提示 |
| `apply_patch_prompt.md` | apply_patch / Codex 风格工具约定 |

### 2.1 加密进二进制

- 脚本：`scripts/encrypt_templates.py`  
- 生成：`src/prompt/prompt_encrypted.rs`  
- 运行时：XOR 解密 → `Zeroizing<String>`（drop 清内存）  
- **不是安全边界**，只是降低 `strings` 明文可见度  
- 测试：`test_encrypted_templates_not_stale` 保证与 `.md` 同步  

### 2.2 MiniJinja 与自定义分隔符

使用 **`${{ }}` / `${% %}`**，避免与散文中的 `{{ }}` 冲突。

关键变量：

| 变量 | 含义 |
|------|------|
| `system_prompt_label` | 如 "Grok 4.5" |
| `is_non_interactive` | headless 时改文案（autonomous agent） |
| `os_name` / `shell_path` / `working_directory` / `current_date` | `<user_info>` |
| `tools.by_kind.read` 等 | 按 **ToolKind** 解析出的实际工具名 |
| `tools.read_file` 等 | full 模式按名引用 |
| `memory_enabled` | 是否渲染 `<memory>` |
| `role_instructions` / `persona_instructions` | 子代理角色 |

条件块示例（概念上）：

```jinja
${%- if tools.by_kind.monitor %}
<background_tasks>
Use the `${{ tools.by_kind.monitor }}` tool ...
</background_tasks>
${%- endif %}
```

**设计要点**：提示词绑定 **能力种类**，不绑定死工具字符串；兼容 toolset 换名仍正确。

---

## 3. `prompt.md` 结构（主模板）

典型章节（随模板迭代）：

1. **身份与目标** — 完成 `<user_query>`  
2. **action_safety / executing_actions_with_care** — 可逆性、确认、危险操作清单  
3. **tool_calling** — 优先专用工具而非 bash 读写文件；禁止用 echo 当 UI  
4. **background_tasks** — monitor 类工具  
5. **output_efficiency / formatting** — 文风与 Markdown  
6. **user_guide** — 交互模式下指向 `~/.grok/docs/user-guide/`  
7. 更多运行时拼上的块：skills 约定、代码编辑纪律、并行工具、引用规范等（视加密模板完整内容）

`subagent_prompt.md` 差异：

- 明确「focused worker」  
- 禁止复述 system prompt  
- 更强调并行工具与 hashline 编辑（若启用）  
- 含 **project_instructions_spec**（AGENTS.md 作用域规则）  
- 角色/persona 插槽  

---

## 4. Agent Definition（产品化 Prompt）

Markdown + YAML frontmatter（见 `xai-grok-agent/README.md`）：

```markdown
---
name: code-reviewer
description: Reviews code for quality and security
tools:
  - read_file
  - grep
promptMode: extend   # 或 full
permissionMode: plan
skills: []
agentsMd: true
---

You are a senior code reviewer...
```

### `promptMode`

| 模式 | 行为 |
|------|------|
| `extend`（默认） | Base 模板 + body 原文追加 + AGENTS + skills |
| `full` | body **即** 完整系统提示（MiniJinja 渲染） |

### 其他与提示相关的字段

- `tools` / `disallowedTools` — 影响模板条件与 API tool 列表  
- `toolNameOverrides` / `paramNameOverrides` — 模型侧命名  
- `completionRequirement` — 必须调用的完成工具 + 未调用时的 reminder  
- `outputFormat` — default / concise（可换 concise toolset）  
- `user_message_template` — 用户消息包装（含 Cursor 兼容）  

发现优先级：项目 `.grok/agents/` > `~/.grok/agents/` > 兼容路径 > 内置（`grok-build`、`browser-use`）。

---

## 5. AGENTS.md / Rules 工程

### 5.1 发现

`prompt/agents_md.rs`：从 cwd 向上到 repo root 收集 `AGENTS.md`、`Claude.md`、`.grok/rules/*.md` 等。

### 5.2 注入策略

- **不** 全部塞进 system（易 stale、难去重）  
- 作为 **User 侧 system-reminder**（`agents_md_user_reminder`）  
- `SyntheticReason::ProjectInstructions` 标记，resume/fork **幂等**  
- compact 时去重逻辑识别 legacy 前缀  

### 5.3 作用域（写入 subagent 提示的规范）

- 文件作用域 = 所在目录子树  
- 更深目录优先  
- 用户即时指令最高优先  

Shell 侧 `partition_rules_by_scope`：workspace vs `~/.grok` / `~/.claude` 分桶标签。

---

## 6. Skills

### 6.1 形态

目录含 `SKILL.md`（frontmatter + 指令正文），可捆绑脚本。  
发现：项目 / 用户 / bundled / 插件。

### 6.2 进入上下文的方式

1. **Slash**：`/skill-name args` → 解析为 skill 调用，注入 `skill_information`  
2. **工具 `skill`**：模型主动加载  
3. **Agent frontmatter `skills:`** 预加载  
4. **System 段**：已 announce 的 skill 列表（避免 resume 重复 BaselineChange）  

大 skill 正文受 `SKILL_INLINE_BUDGET` 限制，超出靠 offload/read。

### 6.3 与 Prompt 的关系

Skill 是 **任务级提示包**，可覆盖工作流而不改全局 system；适合可版本化、可分享的流程（code-review、pr-babysit 等）。

---

## 7. 运行时 System Reminder

工具结果与会话状态常附带：

```xml
<system-reminder>
...
</system-reminder>
```

（IDE 兼容可改 tag 为 `system_reminder`。）

来源包括：

- 工具输出后处理（`xai-grok-tools` reminders）  
- Todo 状态、plan mode、MCP 就绪  
- TodoGate / completionRequirement 未满足  
- Goal continuation  
- Compact 后的 active agent state（`xai-grok-compaction::reminder`）  
- Monitor 事件流  

**工程含义**：把「可变状态」放进 **消息流** 而非反复改 system，利于缓存与审计。

---

## 8. User 消息模板

`prompt/user_message.rs` + shell `construct_user_message`：

- 包装真实用户文本到约定结构（含 `<user_query>` 等标签，与 system 呼应）  
- 可挂 images / 文件引用  
- Cursor 等兼容模板路径  

---

## 9. Structured Output

当调用方提供 `json_schema`：

| 后端能力 | 策略 |
|----------|------|
| 原生 schema | API 层约束最终答案 |
| 非原生 | 注入 `StructuredOutput` 伪工具 + system reminder；校验失败最多重试 3 次 |

这是 **协议层提示词工程**：用 tool 形状强迫模型产出可解析 JSON。

---

## 10. Compaction 提示词（二次 Prompt）

压缩本身是一次「元任务」采样：

- 要求 **不调用工具**  
- 结构化保留：用户目标、文件路径、错误与修复、剩余工作  
- 继任助手只见 **原 query + summary**  

详见 [04-context-management.md](./04-context-management.md)。

---

## 11. 提示词工程实践小结（从本仓库抽象）

1. **分层**：稳定 system + 可变 reminder + 历史  
2. **能力驱动模板**：`ToolKind` 而非硬编码名  
3. **项目知识外置**：AGENTS.md / skills，而非写死在 binary  
4. **运行时门闩补纪律**：TodoGate、completionRequirement（单靠 prompt 不够）  
5. **子代理单独模板**：降噪、降成本  
6. **压缩 = 有结构的再提示**，不是无脑截断  
7. **安全与可逆性**写进 system 默认策略，权限系统做硬边界  
8. **模板可测试**：加密一致性测试 + render 单测  

---

## 12. 源码索引

| 主题 | 路径 |
|------|------|
| 模板 MD | `xai-grok-agent/templates/` |
| 解密/常量 | `xai-grok-agent/src/prompt/template.rs` |
| PromptContext | `xai-grok-agent/src/prompt/context.rs` |
| Builder 组装 | `xai-grok-agent/src/builder.rs` |
| 用户消息 | `xai-grok-agent/src/prompt/user_message.rs` |
| Skills | `xai-grok-agent/src/prompt/skills.rs` + tools `implementations/skills` |
| Shell 注入 | `session/acp_session_impl/prompt_build.rs`, `reminders.rs` |
| 文档式 Agent API | `xai-grok-agent/README.md` |
