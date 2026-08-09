# Grok Build 项目高频英语词组

本页不按字母背单词，而是按工程任务整理可以直接复用的词组。例句尽量描述本仓库中的真实设计。

## 使用方法

每次只选 5 个词组：

1. 先遮住中文，解释英文；
2. 回到源码找到一个真实例子；
3. 不看例句，用当前学习主题造句；
4. 第二天闭卷复述；
5. 一周后仍能正确使用，才算进入主动词汇。

不要只记 `propagate = 传播`。应记完整搭配：`propagate an error to the caller`。

## Repository 与结构

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `repository layout` | 仓库目录布局 | The README summarizes the repository layout. |
| `workspace member` | Cargo workspace 中的成员包 | Each workspace member has its own manifest. |
| `composition root` | 把各组件装配成产品的入口 | The pager binary acts as the composition root. |
| `public API surface` | 对其他模块或 crate 可见的 API 范围 | This change does not expand the public API surface. |
| `implementation detail` | 不应被外部依赖的实现细节 | The channel is an implementation detail of the session layer. |
| `source of truth` | 权威状态来源 | Chat state is the source of truth for the conversation. |
| `call site` | 调用某函数或方法的位置 | Find a call site before reading the full implementation. |
| `entry point` | 执行或阅读入口 | Start from the public entry point. |
| `dependency boundary` | 组件或 crate 之间的依赖边界 | The trait keeps the dependency boundary narrow. |
| `generated file` | 由工具生成、不宜手工维护的文件 | The root manifest is a generated file. |

## Ownership 与状态

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `own the state` | 持有并负责某份状态 | The actor owns the mutable session state. |
| `share a handle` | 共享可克隆的操作句柄 | Multiple tasks can share a cloned handle. |
| `borrow a value` | 借用值而不取得所有权 | The function borrows the configuration. |
| `move into a task` | 将值的所有权移入异步任务 | The sender is moved into the spawned task. |
| `take a snapshot` | 取得某一时刻的状态副本 | The handle stores a snapshot of the MCP configuration. |
| `keep the session resident` | 让 session 保持驻留而不卸载 | On failure, `is_busy` keeps the session resident. |
| `hold a lock` | 持有锁 | Avoid holding the lock across an `.await`. |
| `release the guard` | 释放锁 guard | The code releases the guard before calling external logic. |
| `mutate shared state` | 修改共享状态 | Only the actor mutates the session state directly. |
| `outlive the caller` | 生命周期超过调用者 | A spawned task may outlive the caller. |

## Actor、异步与消息

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `send a command` | 向 Actor 发命令 | The handle sends a command through the MPSC channel. |
| `receive a message` | 从 channel 收到消息 | The actor receives one message at a time. |
| `process messages sequentially` | 串行处理消息 | The actor processes state-changing messages sequentially. |
| `reply through a oneshot channel` | 通过 oneshot 回复一次请求 | The actor replies through a oneshot channel. |
| `wait for completion` | 等待异步工作结束 | The caller waits for completion without blocking the thread. |
| `work in flight` | 已启动但尚未完成的工作 | A session is busy when it has work in flight. |
| `queue an input` | 暂存等待执行的输入 | The session queues the input while another turn is running. |
| `drain the queue` | 依次处理完队列内容 | The actor drains pending inputs after the current turn. |
| `spawn a task` | 启动一个异步 task | The session spawns a task for the active turn. |
| `shut down gracefully` | 完成必要清理后关闭 | The runtime must flush pending updates and shut down gracefully. |
| `drop the sender` | 销毁 channel 的发送端 | Dropping every sender closes the channel. |
| `the channel is closed` | channel 已关闭 | `send` fails when the receiving side is closed. |

## 控制流与生命周期

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `drive the session` | 通过消息推动 session 运转 | `SessionCommand` defines the protocol used to drive the session. |
| `enter the main loop` | 进入主事件循环 | The actor enters the main loop after initialization. |
| `handle an event` | 处理事件 | Each `select!` branch handles a different event source. |
| `reach a terminal state` | 到达不可再继续的终态 | Every tool stream must reach one terminal state. |
| `resume a session` | 恢复已有 session | Durable updates allow the runtime to resume a session. |
| `cancel the active turn` | 取消当前 turn | Cancellation stops the active turn but not necessarily the session. |
| `flush pending updates` | 把尚未发送或落盘的更新排空 | The actor flushes pending updates before shutdown. |
| `preserve ordering` | 保持事件顺序 | The replay buffer preserves client-visible ordering. |
| `trigger a retry` | 触发重试 | A transient network error may trigger a retry. |
| `skip initialization` | 跳过初始化 | The fast path skips initialization when state is already available. |

## 错误、恢复与安全

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `propagate an error` | 将错误交给上层处理 | The method propagates the storage error to the caller. |
| `map an error to` | 把内部错误转换成另一层错误 | The ACP layer maps the sampling error to a protocol error. |
| `fall back to` | 主路径失败时采用保守替代值 | `is_busy` falls back to `true` if the actor is unreachable. |
| `fail closed` | 无法确认时拒绝能力或保持关闭 | The content gate fails closed when policy is unavailable. |
| `fail fast` | 发现无效输入后尽早报错 | Configuration validation fails fast. |
| `handle a timeout` | 处理超时 | Shutdown handles a persistence flush timeout explicitly. |
| `recover from failure` | 从失败中恢复 | The session can recover from a transient sampling failure. |
| `surface an error` | 把错误展示给某个受众 | The UI surfaces a useful error to the user. |
| `retain error context` | 保留有助于诊断的上下文 | The conversion should retain the original error context. |
| `avoid leaking` | 避免泄露敏感或内部信息 | Serialization must avoid leaking credentials. |

## Tool、协议与模型请求

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `dispatch a tool call` | 将工具调用路由到实现 | `ToolBridge` dispatches a tool call to the registry. |
| `register a tool` | 将工具加入 registry | Built-in tools are registered during session setup. |
| `execute a command` | 执行 shell 命令或工具命令 | The terminal backend executes the command. |
| `emit progress` | 发出进度项 | A streaming tool may emit multiple progress items. |
| `produce a terminal result` | 产生唯一终态结果 | The stream produces exactly one terminal result. |
| `cross the wire boundary` | 跨越序列化或协议边界 | Typed data becomes JSON at the wire boundary. |
| `serialize into JSON` | 序列化成 JSON | The runtime serializes the typed output into JSON. |
| `build the request` | 组装发给模型的请求 | Chat state builds a request from conversation history. |
| `consume a stream` | 逐项读取 stream | The sampler consumes the backend response stream. |
| `enforce an invariant` | 保证某条不变量 | The helper enforces the terminal-item invariant. |

## 测试与论证

| 英文词组 | 项目语义 | 示例 |
| --- | --- | --- |
| `cover an edge case` | 覆盖边界场景 | This test covers a closed-channel edge case. |
| `reproduce the failure` | 稳定复现失败 | The fixture reproduces the malformed stream. |
| `assert that` | 断言某事实 | The test asserts that the final event is terminal. |
| `verify the behavior` | 验证运行时行为 | A focused test verifies the cancellation behavior. |
| `provide evidence for` | 为某结论提供证据 | Compilation alone does not provide evidence for event ordering. |
| `remain unverified` | 仍然缺少证据 | The Windows-specific path remains unverified. |
| `narrow the scope` | 缩小问题或测试范围 | Narrow the scope to one crate and one behavior. |
| `introduce a regression` | 引入行为退化 | The change must not introduce a protocol regression. |
| `preserve compatibility` | 保持兼容性 | The serializer preserves backward compatibility. |
| `match zero tests` | 过滤器未实际选中测试 | A successful command may still match zero tests. |

## 连接逻辑的句型

下面这些结构比复杂语法更值得优先掌握。

### 定义职责

```text
X owns ...
X is responsible for ...
X provides a way to ...
X acts as the boundary between A and B.
```

### 描述数据流

```text
The caller sends X through Y.
The actor receives X and updates Y.
The result is returned to the caller through Z.
```

### 描述条件和失败

```text
If the receiver is closed, `send` returns an error.
When no turn is running, the actor starts the next queued input.
On failure, the method falls back to a conservative value.
```

### 描述目的

```text
This keeps state changes serialized.
This allows multiple callers to share the same actor.
This prevents the session from being unloaded by mistake.
```

### 区分事实与推断

```text
The type definition shows that ...
The call site confirms that ...
The test verifies that ...
This suggests that ..., but the behavior remains unverified.
```
