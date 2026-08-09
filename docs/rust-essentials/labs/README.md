# Rust 可运行实验

这里有两组互补的微型程序：[`katas.rs`](./katas.rs) 只使用标准库，隔离所有权、模式、迭代器、trait、线程和 channel 语义；[`async-demos/`](./async-demos/) 使用 Tokio，把 `select!`、取消、异步通道、local task、timer 和异步锁缩小到几十行内观察。它们不修改仓库业务代码，独立的 async 包也不会加入根 Cargo workspace。

## 标准库 Katas

不要先运行。每次选一个 `kata_XX` 测试：

1. 遮住 `assert_eq!` 的右侧，预测值和变量是否仍可使用。
2. 标出每次 move、borrow、clone 和类型转换。
3. 运行单个测试验证。
4. 修改一个条件，让测试先失败，再解释为什么。
5. 回到测试注释指向的仓库概念，找一个同形状用法。

```sh
# 列出练习
rustc --edition 2024 --test docs/rust-essentials/labs/katas.rs \
  -o /tmp/grok-rust-katas
/tmp/grok-rust-katas --list

# 只运行第 4 关；--exact 防止误匹配
/tmp/grok-rust-katas --exact tests::kata_04_option_result_transpose --nocapture

# 运行全部正常练习
/tmp/grok-rust-katas
```

## Tokio async demos

每个 demo 都是独立的可执行程序，并包含 `assert!`/`assert_eq!`。不要只看退出码：先预测打印顺序、哪个 future 会被 drop、channel 何时关闭，再对照源码和输出。

```sh
cargo run --locked \
  --manifest-path docs/rust-essentials/labs/async-demos/Cargo.toml \
  --bin select_race
```

将最后的 bin 名替换为表中任意名称即可单独运行：

| bin | 运行前预测 | 观察重点 |
| --- | --- | --- |
| [`select_race`](./async-demos/src/bin/select_race.rs) | 10 ms 和 30 ms timer 谁获胜？另一个 future 的局部 guard 会不会 drop？ | `select!` 在当前 task 竞争；未胜分支离开宏时被 drop |
| [`select_loop`](./async-demos/src/bin/select_loop.rs) | queued message、timeout、delayed message、close 的顺序是什么？ | 循环、`recv() -> None`、`if` guard 禁用已经完成的 timer 分支 |
| [`select_cancel_drop`](./async-demos/src/bin/select_cancel_drop.rs) | timeout 后 `started`、`committed`、cleanup 三个状态各是什么？ | drop 会释放 Future 内字段，但不会自动回滚已经发生的外部副作用 |
| [`watch_latest`](./async-demos/src/bin/watch_latest.rs) | 连发 1、2、3 后慢 receiver 会读几次、读到什么？相同值会不会通知？ | watch 保存最新状态和版本，不保存事件队列；最后一个 sender drop 后关闭 |
| [`actor_request_reply`](./async-demos/src/bin/actor_request_reply.rs) | command 和结果分别走哪条 channel？谁拥有累计状态？ | 有界 mpsc 传命令、oneshot 传一次回复、显式 shutdown 后 join actor |
| [`mini_session_actor`](./async-demos/src/bin/mini_session_actor.rs) | 第二条较快的 Prompt 会不会越过第一条？第一条运行时 Actor 能否回答 Status？ | mailbox、输入队列、单一 running turn、completion 回流、`select!` 和 shutdown flush 如何组成 Session 骨架 |
| [`mini_tool_pipeline`](./async-demos/src/bin/mini_tool_pipeline.rs) | JSON 参数在哪一层变回强类型？拒绝、缺 Terminal 和成功分别怎样呈现？ | prepare、权限、`Progress* -> Terminal`、UI/prompt 两份输出和 ChatState tool result |
| [`mini_replay_order`](./async-demos/src/bin/mini_replay_order.rs) | 两个文本 chunk、一个 tool event、尾部文本和 TurnCompleted 以什么顺序到客户端？ | 流式 chunk 合并、非流式事件强制 flush、completion 前 flush 尾部文本 |
| [`mini_cancel_shutdown`](./async-demos/src/bin/mini_cancel_shutdown.rs) | Cancel 后 Session 能否运行下一 Turn？Shutdown ack 前哪些 cleanup 必须完成？ | Turn 取消、RAII cleanup、Session 继续存活，以及 shutdown 对后台 workflow 的 join/flush 所有权 |
| [`mini_sampler_retry`](./async-demos/src/bin/mini_sampler_retry.rs) | 无输出失败、半截输出、空响应和 401/context error 各会尝试几次？ | logical request/attempt 分离、只在输出前 retry、空响应分类，以及 Session 级恢复边界 |
| [`mini_context_compaction`](./async-demos/src/bin/mini_context_compaction.rs) | pruning 后权威 history 是否变化？窗口缩小会不会触发 compact？ | request clone 与 ChatState、85% 阈值、full-replace 后的摘要/原目标/reminder |
| [`mini_workspace_rewind`](./async-demos/src/bin/mini_workspace_rewind.rs) | 同轮两次写入保留哪个 before？外部修改或恢复写失败后 checkpoint 怎样变化？ | 修改前权限、before/after snapshot、冲突仍写回，以及成功后才 truncate |
| [`mini_config_resolution`](./async-demos/src/bin/mini_config_resolution.rs) | nested object、数组、类型冲突怎样 merge？刷新后旧 Session 会变吗？ | 深度合并、带 `ConfigSource` 的优先级、requirement pin 和 Session snapshot |
| [`mini_auth_model_boundary`](./async-demos/src/bin/mini_auth_model_boundary.rs) | alias、显示名和 wire model 哪个进入请求？第三方 401 会不会刷新 Session token？ | endpoint/BYOK gate、单 Turn 恢复预算、SamplerConfig 重建和 secret 脱敏 |
| [`mini_mcp_singleflight`](./async-demos/src/bin/mini_mcp_singleflight.rs) | 两个 caller 会握手几次？holder 被取消后状态是什么？旧 client 的 close 会删除替代者吗？ | 单一 handshake owner、RAII 恢复 `Pending`、waiter 唤醒和 `client_id` 防陈旧事件 |
| [`mini_workflow_replay`](./async-demos/src/bin/mini_workflow_replay.rs) | 相同 host call 恢复时会不会再次产生副作用？参数变化和预算超限怎样失败？ | 密集 journal sequence、request hash、divergence 和原子 reservation |
| [`mini_trace_timeline`](./async-demos/src/bin/mini_trace_timeline.rs) | 裸 `spawn` 会继承 task-local 吗？哪些 ID 能串起 prompt、request 和 tool？ | 显式上下文传播、traceparent、关联键作用域和字符串字段默认拒绝 |
| [`spawn_local_rc`](./async-demos/src/bin/spawn_local_rc.rs) | `Rc<RefCell<_>>` 为什么不能交给普通 `spawn`，这里却能共享？ | current-thread runtime 仍需 `LocalSet`；不依赖两个 task 的偶然调度顺序 |
| [`timer_reset`](./async-demos/src/bin/timer_reset.rs) | 一个 `Sleep` 完成后怎样再次等待新 deadline？ | 循环外创建、`tokio::pin!`、`.as_mut()` 重借用和 `reset()` |
| [`mutex_snapshot`](./async-demos/src/bin/mutex_snapshot.rs) | reader 停在第二个 `.await` 时，另一个 task 能否立刻取锁？ | 在小作用域内复制 owned snapshot，让 `MutexGuard` 在 await 前释放 |

前三个程序直接对应 [10 `tokio::select!`](../10-tokio-select.md)。使用暂停的 Tokio 时钟的示例不会等待真实的 10/30/50 ms；显式 oneshot 同步点也避免用 sleep 猜 task 已运行到哪里。

## 一键校验

下面的命令会运行全部正常 Katas、确认六个反例按预期编译失败，并逐个运行二十个 async demos：

```sh
docs/rust-essentials/labs/check.sh
```

脚本只在 `mktemp` 创建的临时目录中写编译产物（包括 `CARGO_TARGET_DIR`），结束后自动清理。

## Compile-fail 练习

[`compile_fail/`](./compile_fail/) 中的文件是刻意错误的，不能加入正常构建。先不用编译器回答：哪个值或借用活得太久？最小修复是什么？修复会改变性能还是并发语义吗？

| 文件 | 预期诊断 | 核心问题 | 不应机械采用的修复 |
| --- | --- | --- | --- |
| [`moved_value.rs`](./compile_fail/moved_value.rs) | E0382 | 值已 move 后再次使用 | 到处 `.clone()` |
| [`borrow_conflict.rs`](./compile_fail/borrow_conflict.rs) | E0502 | 共享借用存活时进行可变借用 | 用 `unsafe` 绕开借用检查 |
| [`multiple_mut_borrow.rs`](./compile_fail/multiple_mut_borrow.rs) | E0499 | 同一集合存在重叠可变借用 | 用裸指针绕开别名规则 |
| [`return_local_reference.rs`](./compile_fail/return_local_reference.rs) | E0515 | 返回值引用函数内局部所有者 | 为通过编译而泄漏内存 |
| [`temporary_dropped.rs`](./compile_fail/temporary_dropped.rs) | E0716 | 借用了语句结束即 drop 的临时值 | 随意改成 `'static` |
| [`rc_is_not_send.rs`](./compile_fail/rc_is_not_send.rs) | E0277 | `Rc<T>` 不能跨线程 move | 不分析共享需求就套 `Arc<Mutex<_>>` |

单独查看完整诊断：

```sh
rustc --edition 2024 docs/rust-essentials/labs/compile_fail/moved_value.rs
rustc --explain E0382
```

编译器错误码和措辞可能随工具链变化，`check.sh` 只校验本仓库固定工具链下的核心错误码。

## 建议变体

- 第 1 关：去掉 `.clone()`，观察哪个后续使用导致 E0382。
- 第 2 关：把 `describe(&event)` 改成按值接收，观察 enum 字段何时被移动。
- 第 4 关：把 `transpose()` 展开成完整 `match`，确认两者语义一致。
- 第 5 关：依次替换 `iter()`、`iter_mut()`、`into_iter()`，记录 item 类型。
- 第 7 关：给 trait 增加返回 `Self` 的方法，再尝试通过 `&dyn Render` 调用。
- 第 12 关：把 `Mutex<u32>` 换成 `AtomicU32`，说明需要选择哪种 ordering 以及原因。
- 第 13 关：注释 actor 的回复发送，观察 caller 收到的 channel 错误。

`multiple_mut_borrow.rs` 的正确方向通常是 `split_at_mut`、按顺序缩短借用，或重构数据布局；`return_local_reference.rs` 通常应返回拥有所有权的值；`temporary_dropped.rs` 应先把所有者绑定到局部变量。这些修复分别改变 API、作用域或数据布局，不能只看“错误消失”。

完成变体后，不保留一个“为了通过而碰巧能跑”的版本。用一句话记录你改变了哪个类型或所有权契约。
