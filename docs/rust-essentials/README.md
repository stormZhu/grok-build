# 项目必备 Rust 知识速查

> 梳理了阅读 `grok-build` 代码库所需的核心 Rust / Tokio 知识点，每个知识点配有项目中的真实代码示例。

---

## 知识点列表

| # | 文档 | 简介 |
|---|------|------|
| 1 | [tokio::select! 宏](./01-tokio-select.md) | 同时等待多个异步操作，`biased;` 模式、`if` 条件守卫、模式匹配 |
| 2 | [Pin 与异步 Future](./02-pin-and-future.md) | `tokio::pin!`、`Sleep::reset()`、永不触发定时器技巧 |
| 3 | [Atomic 类型与 Ordering](./03-atomic-ordering.md) | `load/store/fetch_add`、`Relaxed` vs `AcqRel` 选择指南 |
| 4 | [spawn_local 与单线程运行时](./04-spawn-local.md) | `spawn` vs `spawn_local`、`current_thread` 运行时 |
| 5 | [watch 通道](./05-watch-channel.md) | `changed()` + `borrow_and_update()` 配合使用 |
| 6 | [Arc / Mutex / RwLock](./06-arc-mutex-rwlock.md) | 异步代码中共享状态、`clone` 模式 |
| 7 | [tracing 日志宏](./07-tracing.md) | 结构化日志、`target` 分流 |

---

## 快速参考卡片

| 概念 | 一句话 | 首次出现 |
|------|--------|----------|
| `select!` | 同时等多个异步操作，谁先完成执行谁 | [run_loop.rs:309](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L309) |
| `biased;` | select! 按书写顺序优先，不再随机 | [run_loop.rs:310](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L310) |
| `if` 守卫 | 条件不满足时跳过 poll，零开销 | [run_loop.rs:312](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L312) |
| `tokio::pin!` | 固定 future 防止被移动 | [run_loop.rs:302](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L302) |
| `Sleep::reset()` | 重置定时器，无需重建 | [run_loop.rs:339](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L339) |
| `Atomic*` | 无锁原子变量，比 Mutex 轻量 | [run_loop.rs:314](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L314) |
| `Ordering::Relaxed` | 只保证原子性，不保证顺序 | [run_loop.rs:314](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L314) |
| `spawn_local` | 单线程运行时专用 spawn | [run_loop.rs:324](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L324) |
| `watch::changed()` | 等待值变化，变化时触发 | [run_loop.rs:363](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L363) |
| `Arc::clone()` | 增加引用计数，共享所有权 | 全项目通用 |
| `tracing::info!` | 结构化日志，支持 target 分流 | [run_loop.rs:320](file:///Users/yuqing/Documents/workspace/grok-study/grok-build/crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs#L320) |

---

*持续更新中，遇到新的知识点随时补充。*