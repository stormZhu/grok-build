# 15 · 运行时调试手册：从症状找到 owner

本页处理“程序能编译，但行为不对”的问题。依赖下载、DotSlash、protoc、首次构建和模型凭据配置仍查 [构建与排障手册](./08-build-troubleshooting.md)；这里聚焦 Prompt、Turn、工具、取消、关闭、持久化和 UI 更新。

调试目标不是收集最多日志，而是尽快回答：**哪个 owner 没有收到输入、没有推进状态，或没有发出终态？**

## 固定的六步流程

1. **冻结症状**：写出最后一个确定成功的事件和第一个缺失事件。
2. **确认宿主模式**：TUI 直连、`grok -p`、relay headless、stdio 还是 Leader；不要跨错进程找状态。
3. **选择关联值**：优先使用 session id、prompt id、tool call id、request id 或 trace id。
4. **定位 owner**：SessionActor、running Turn、Sampler、Tool/Workspace、Persistence 还是 Pager reducer。
5. **画等待边**：谁在等 channel、锁、I/O、子任务或用户批准；谁应唤醒它。
6. **构造最小证据**：focused test、fixture、oneshot/barrier 或结构化日志；修复后用同一证据验证。

不要一开始重跑整个 workspace、增加大量无关联日志或提高 timeout。这些操作会扩大噪声，却没有缩小故障边界。

## 症状路由表

| 症状 | 首先确认 | 主要 owner/边界 | 深入材料 |
|---|---|---|---|
| Prompt 已发送但没有开始 | command 是否到达、是否仍在 `pending_inputs`、已有 `running_task` | `SessionHandle` → `SessionActor` | [`run-session`](./deep-dives/run-session.md) |
| 模型输出中途停止 | Sampler 是否发终态、Turn 是否回传 completion | Sampler stream → `handle_prompt` → completion | [`sampling-lifecycle`](./deep-dives/sampling-lifecycle.md) |
| 工具卡住或没有 Terminal | 是否等待权限、进程、MCP 或 stream 终态 | ToolBridge / Workspace / MCP | [`tool-call-pipeline`](./deep-dives/tool-call-pipeline.md) |
| Cancel 后仍有工作继续 | 取消只停止等待还是也能停止底层操作 | Session → Turn → Sampler/tool child | [`cancellation-and-shutdown`](./deep-dives/cancellation-and-shutdown.md) |
| Shutdown 挂住 | replay、persistence、workflow、feedback 或子任务谁未 ack/join | SessionActor shutdown owner | [`cancellation-and-shutdown`](./deep-dives/cancellation-and-shutdown.md) |
| Turn 完成但 UI 少最后一段 | completion 前 replay buffer 是否 flush | ReplayBuffer → ACP update → Pager | [`persistence-and-replay`](./deep-dives/persistence-and-replay.md) |
| 重启后历史缺失或顺序错 | 通知、JSONL append、chat rebuild 是否混为一层 | persistence actor / storage | [`persistence-and-replay`](./deep-dives/persistence-and-replay.md) |
| 模型或凭据与预期不同 | 最终 `SamplerConfig` 的来源和刷新时点 | ConfigLayers / ModelsManager / auth | [`authentication-and-model-resolution`](./deep-dives/authentication-and-model-resolution.md) |
| 文件已改但 diff/rewind 不对 | Workspace snapshot、hunk、Git checkpoint 各处于什么状态 | Workspace session | [`workspace-state`](./deep-dives/workspace-state-and-worktree-lifecycle.md) |
| MCP 工具消失或反复重连 | client identity、stale close、liveness 状态 | MCP dispatcher | [`mcp-dispatcher`](./deep-dives/mcp-dispatcher.md) |
| 日志有 task panic 但主进程仍活着 | JoinHandle 是否被保存和观察 | task owner/supervisor | [Rust async](./rust-essentials/08-async-runtime-tasks.md) |

## 把“卡住”翻译成等待图

```text
caller
  └─等待 oneshot reply
       └─SessionActor 等待/处理 command
            ├─pending_inputs 等待成为 running_task
            └─running Turn 等待 completion
                 ├─Sampler 等待 HTTP stream
                 ├─Tool 等待权限或子进程
                 └─Persistence/Replay 等待 ack 或 flush
```

对每条边写出：等待的具体 Future、唤醒者、关闭时的返回值、timeout 后底层操作是否仍继续。只写“async 卡住了”无法区分 channel sender 未 drop、锁循环、I/O 无响应和终态丢失。

## Prompt 没有开始

按顺序查四个状态，不要直接跳到模型网络：

1. 调用方是否成功发送 `SessionCommand::Prompt`？send 失败通常意味着 receiver 已关闭。
2. `queue_input` 是否接纳、排队或因 admission/send-now 改变行为？
3. `running_task` 是否已存在？Session 串行执行前台 Turn，排队不等于丢失。
4. `maybe_start_running_task` 是否被 completion/cancel 后再次调用？

导航命令：

```sh
rg -n "SessionCommand::Prompt|queue_input|maybe_start_running_task|running_task" \
  crates/codegen/xai-grok-shell/src/session
```

可运行的缩小模型是 [`mini_session_actor`](./rust-essentials/labs/async-demos/src/bin/mini_session_actor.rs)。它故意让第二条 Prompt 用时更短，但仍断言第二条必须等第一条完成后才能启动。

## Turn 或流式输出停住

把三种“完成”分开：

- HTTP/SSE stream 结束；
- `handle_prompt` 产出 `PromptTurnResult`；
- SessionActor 消费 completion，并在终态前 flush 最后一个客户端 delta。

任何一层缺失都会表现为“没有最终回复”。先追一个 request/prompt id，不要按时间相近把多个并发 session 的日志拼在一起。若客户端只缺最后一段，优先查 ReplayBuffer 和 completion 顺序；若连首个 chunk 都没有，优先查请求构建、认证和 Sampler stream。

## 工具调用卡住

一个工具调用至少包含五个可能等待点：

```text
模型 tool call
  -> 参数解析/registry
  -> plan gate / permission
  -> ToolBridge / MCP / builtin execution
  -> Progress* / Terminal stream
  -> ToolResult 写回 ChatState，继续采样
```

记录最后看见的是 permission request、Progress 还是进程退出。Stream 结束不自动等于收到合法 Terminal；timeout 结束等待也不保证外部子进程已经停止。涉及文件或命令时，还要确认 Workspace permission 和 OS sandbox 是两个不同边界。

## Cancel 与 Shutdown

Cancel 是一次 Turn 的协作式停止，Shutdown 是 Session 资源的最终所有者执行完整收尾。不要用同一个“取消成功”断言覆盖两者。

Cancel 至少验证：

- 当前 Turn 不再继续写入普通完成结果；
- replay buffer 的已有输出按契约 flush；
- 工具/采样收到对应取消信号；
- 新 Prompt 是否可以继续启动。

Shutdown 至少验证：

- command 入口关闭或显式 `Shutdown` 被处理；
- hooks、memory/persistence、replay 和 feedback 有明确顺序；
- 子任务被 cancel 后还会被 join/drain；
- 最终 ack 不会早于必须完成的持久化边界。

`JoinHandle::abort`、drop Future、关闭 channel 和 kill OS process 是四种不同动作。发现残留工作时先确认系统实际采用哪一种。

## UI 与持久化不一致

区分三份数据：

| 数据 | 目的 | 典型 owner |
|---|---|---|
| ChatState conversation | 模型下一轮请求的权威上下文 | ChatStateActor |
| JSONL/session storage | 重启、恢复和审计 | persistence/storage |
| ReplayBuffer / ACP updates | 当前客户端的增量显示 | SessionActor + client reducer |

“UI 看见了”不能证明已经持久化，“JSONL 有记录”也不能证明 Pager reducer 已消费。选择与症状同层的证据，再检查跨层提交顺序。

## 证据收集命令

```sh
# 找 owner、消息变体和收发两端
rg -n "enum SessionCommand|SessionCommand::Cancel|SessionCommand::Shutdown" \
  crates/codegen/xai-grok-shell/src/session

# 先列出可能的 focused tests
cargo test -p xai-grok-shell -- --list | rg -i "cancel|shutdown|replay|completion"

# 调试一个具体测试时显示日志/输出，并确认实际运行数量
cargo test -p xai-grok-shell <exact-filter> -- --nocapture
```

完整的 span、unified log、debug firehose 和 OTLP 路径见 [`observability-and-trace-timeline`](./deep-dives/observability-and-trace-timeline.md)。收集日志时保留关联 ID 和事件顺序，并删除 token、Authorization header、prompt 私密内容和本地敏感路径。

## 异步故障测试的最低标准

- 用 oneshot/barrier 证明 task 已到达目标阶段，不用真实 sleep 猜调度。
- 用 timeout 限制失败测试，但断言超时发生在哪个等待边。
- 断言消息内容、状态和终态顺序，不只看进程退出码。
- channel 关闭测试必须明确 drop 哪些 sender/receiver。
- 取消测试同时检查 RAII cleanup 与不可回滚的外部副作用。
- focused filter 运行后检查实际 test 数量，零测试不是通过证据。

可在不改生产代码的前提下，对四个项目缩小模型做故障注入：

1. 暂时去掉 completion 分支中的 `maybe_start_running_task`，观察第二条 reply 永远不来。
2. 在 `mini_tool_pipeline` 中去掉 Terminal，确认 Progress 不能冒充成功结果。
3. 在 `mini_replay_order` 中把 `TurnCompleted` 放到 flush 前，确认顺序断言失败。
4. 在 `mini_cancel_shutdown` 中让 Shutdown 不 await workflow，确认 cleanup 证据为何不足。

实验结束后恢复能通过断言的版本，不保留偶然依赖调度或无限等待的代码。

## 调试结论模板

```text
症状：最后成功事件 / 第一个缺失事件
模式：TUI direct / grok -p / relay / stdio / Leader
关联值：session / prompt / tool call / request / trace id
owner：权威状态属于谁
等待边：谁等待什么，由谁唤醒
根因：哪个契约被违反
证据：源码定义 + 收发端 + 可重复测试/日志
修复：最小行为变化
验证：实际运行的测试及覆盖范围
残余风险：未覆盖的平台、竞态、协议或外部系统
```

一个好的调试结论必须能被反驳：它明确指出什么新证据会推翻当前判断，而不是用“可能是异步问题”结束调查。
