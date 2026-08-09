# 18. 宏、展开代码与 RAII guard

宏在类型检查前接收 token 并生成 Rust 代码；RAII 让资源释放绑定到值的作用域。两者在仓库里经常组合：derive/属性宏生成 trait 或 span，声明式宏生成重复 impl，guard 的 `Drop` 则覆盖成功、`?`、panic unwind 和 future 取消路径。

## 先识别四类宏

| 形式 | 类型 | 典型用途 |
| --- | --- | --- |
| `println!(...)` / `vec![...]` | function-like macro | 表达式或 item 生成 |
| `macro_rules! name { ... }` | 声明式宏 | 按 token pattern 展开 |
| `#[derive(Serialize)]` | derive proc macro | 根据类型定义生成 impl |
| `#[tokio::test]` / `#[tracing::instrument]` | attribute proc macro | 重写被标注 item |

函数只接收已经类型化的值；宏接收语法 token，因此可以生成字段、impl、match arm 或整个函数。宏调用尾部是否需要分号取决于它展开成 item、statement 还是 expression。

读代码时不要停在“这是宏”：

1. 找宏定义或宏 crate 文档。
2. 写出它大致展开成哪些 item/expression。
3. 再按普通 Rust 检查所有权、trait bound、控制流和副作用。
4. 报错位置若落在生成代码，回到调用参数与展开约束对应。

## `macro_rules!` 的匹配模型

```rust
macro_rules! make_getter {
    ($name:ident, $field:ident, $ty:ty) => {
        fn $name(&self) -> &$ty {
            &self.$field
        }
    };
}
```

`$name:ident`、`$ty:ty` 等 fragment specifier 限制输入语法类别。常见类别有 `expr`、`pat`、`path`、`item`、`literal`、`tt`。`tt` 最宽松，但也把更多错误推迟到展开后。

重复语法：

```rust
macro_rules! string_list {
    ($($value:expr),* $(,)?) => {
        vec![$($value.to_string()),*]
    };
}
```

- `$(...),*` 表示逗号分隔的零个或多个。
- `$(,)?` 允许一个可选尾逗号。
- 重复层级中的 metavariable 必须在兼容层级展开。

宏不是文本替换。Rust 有 hygiene：宏内部局部绑定通常不会意外捕获调用点变量。跨 crate 引用自身 item 时使用 `$crate`，不要写死发布后的 crate 名。

## 仓库的 `register_resource!`

[`register_resource!`](../../crates/codegen/xai-grok-tools/src/types/resources.rs) 接受两个 literal 和一个 type：

```rust
#[macro_export]
macro_rules! register_resource {
    ($namespace:literal, $name:literal, $ty:ty) => {
        impl $crate::types::resources::ResourceType for $ty {
            const ID: &'static str = concat!($namespace, ".", $name);
        }
    };
}
```

调用：

```rust
register_resource!("grok_build", "ReadFile", ReadHistory);
```

应在脑中展开为：

```rust
impl xai_grok_tools::types::resources::ResourceType for ReadHistory {
    const ID: &'static str = "grok_build.ReadFile";
}
```

阅读重点：

- `$crate` 指向定义宏的 crate，即使调用者给依赖改名也正确。
- `concat!` 在编译期生成 `&'static str`，没有运行时分配。
- 宏不是运行时 registry 操作，只生成一个 trait impl。
- 若同一 type 重复注册，会因冲突 impl 编译失败。
- `literal` 限制 namespace/name 不能是任意运行时 String。

## derive 宏隐藏的契约

`#[derive(...)]` 看似一行，可能生成大量 impl 和 bounds：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Envelope<T> {
    payload: T,
}
```

通常会要求 `T` 满足相应 trait，但 Serde 等宏可通过属性覆盖 bounds。读 derive 类型时检查：

- 生成哪些 trait，调用方为何能使用相应方法。
- 泛型参数新增了什么 bound。
- Serde rename/default/skip 如何改变 wire。
- Debug 是否会泄露 secret。
- Clone 是否复制大 buffer，还是只 clone Arc。
- proc macro 是否生成注册、inventory 或额外 symbol。

derive 不代表行为“免费”或“显然”。协议 derive 见 [05 Serde](./05-serde-and-wire-compat.md)。

## 属性宏会重写控制流

`#[tokio::test]` 生成 runtime setup 后执行 async body；`#[async_trait]` 把 async method 转成返回 boxed future 的形状；`#[tracing::instrument]` 创建 span 并 instrument 函数调用。

所以错误可能来自宏生成的要求：

- future 需要 `Send`。
- 参数为了字段记录需要 `Debug`。
- 返回值为了 `ret` 需要 `Debug`/`Display`。
- trait object 方法被 boxed 后有生命周期 bound。
- test flavor 与 LocalSet 不匹配。

遇到难读诊断时使用 rust-analyzer 的 macro expansion。若环境已安装 `cargo-expand`，可运行：

```sh
cargo expand -p <package> module::item
```

不要把展开结果提交回源码；它是诊断视图。stable 编译器错误中的 “this error originates in the macro” 只说明来源，不代表应忽略最内层 cause。

## 宏设计边界

新增宏前先判断普通函数、trait、泛型或 const 是否足够。宏适合必须生成语法结构的重复；不适合隐藏普通业务控制流。

一个可维护宏应：

- 输入 fragment 尽量具体。
- 生成代码使用 `$crate` 和全限定路径，减少调用点 import 假设。
- 错误尽量落在调用参数附近。
- 支持或明确拒绝尾逗号等常见语法。
- 有至少一个跨 module/crate 使用测试。
- proc macro 用 compile-pass/compile-fail fixture 验证诊断。
- 不悄悄执行 I/O 或注册全局状态，除非名称与文档明确说明。

## RAII：资源由值拥有

Resource Acquisition Is Initialization 表示构造值时取得资源，值 drop 时释放：

```rust
{
    let file = File::open(path)?;
    let guard = mutex.lock()?;
    // 使用 file 和 guard
} // guard 解锁，file 关闭
```

常见 RAII 类型：

- `File`、socket、`OwnedFd`：关闭 OS handle。
- `MutexGuard`/`RwLockReadGuard`：释放锁。
- `tempfile::TempDir`：清理临时目录。
- tracing appender `WorkerGuard`：flush/停止后台 writer。
- 自定义状态 guard：恢复环境、标志、文件或注册项。

RAII 的价值是所有权可追踪：不必在每个 return/`?` 分支重复 cleanup。

## Drop 何时运行

正常情况下，局部值在离开作用域时按声明的逆序 drop；struct 自己的 `Drop::drop` 先运行，随后字段按声明顺序 drop。以下路径也会 drop 已构造值：

- 普通 return。
- `?` 提前返回。
- panic 且采用 unwind。
- future 被取消/drop。
- `select!` 未获胜分支的临时 future 被 drop。

以下情况不能依赖 destructor：

- `panic = "abort"` 或显式 `process::abort/exit`。
- 进程崩溃、kill -9、断电。
- `mem::forget` 或泄漏形成的强引用环。
- 程序永久挂起，没有离开作用域。

根 workspace 的部分 profile 使用 panic abort；关键持久化正确性不能只依赖“panic 时 Drop 一定执行”。

## 仓库的取消恢复 guard

[`PlanGuard`](../../crates/codegen/xai-grok-shell/src/session/goal_strategist.rs) 保存 `plan.md` 的原始状态。正常路径可显式恢复；若 runner future 在 await 中被取消，Drop 兜底：

```rust
impl Drop for PlanGuard<'_> {
    fn drop(&mut self) {
        if let Some(reason) = self.restore() {
            tracing::error!(
                reason = reason.as_const_str(),
                "goal strategist: plan.md restore failed during drop",
            );
        }
    }
}
```

要注意：

- `restore()` 设计为幂等；正常路径已恢复时 Drop 不重复破坏状态。
- Drop 不能返回错误，只能记录或更新其他可观察状态。
- 这里执行同步文件操作，可能阻塞；这是恢复契约与 Drop 限制之间的明确取舍。
- 它检查 symlink 等篡改场景，不是简单无条件覆盖。
- abort/进程崩溃时仍不能保证恢复，若要求 crash consistency 需更强的持久化协议。

## 测试状态恢复 guard

[`with_dark`](../../crates/codegen/xai-grok-config/src/signed_policy.rs) 临时替换 thread-local override：

```rust
let previous = cell.replace(Some(Some(Vec::new())));

struct Restore(Option<KeyOverride>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        LOCAL_OVERRIDE.with(|cell| {
            *cell.borrow_mut() = previous;
        });
    }
}

let _restore = Restore(previous);
f()
```

`Option::take` 让 restore value 只消费一次。guard 变量以下划线开头只是抑制“未读取”警告，并不提早 drop；它仍活到作用域结尾。不要写成 `let _ = Restore(...)`，后者可能在该 statement 结束就 drop，失去保护区间。

## Drop 不能 async

Rust 的 `Drop::drop` 是同步函数。需要 flush 网络、等待 child、发送协议 shutdown 或 join task 时，提供显式异步生命周期：

```rust
impl Worker {
    async fn shutdown(mut self) -> Result<(), Error> {
        self.cancel.cancel();
        if let Some(task) = self.task.take() {
            task.await??;
        }
        self.closed = true;
        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if !self.closed {
            self.cancel.cancel();
            // 只能做同步、best-effort 的兜底，不能 await。
        }
    }
}
```

调用者在正常路径 await `shutdown`，Drop 只防遗忘或取消。不要在 Drop 内启动 fire-and-forget async cleanup 并声称资源已安全释放；runtime 可能已经结束。

## Drop 中不要 panic

析构时 panic 若发生在另一个 panic unwind 过程中，进程会 abort。Drop 应尽量：

- 只做不可失败的内存状态恢复。
- 对可失败 cleanup 记录错误或保存状态。
- 把需要调用者处理的错误放入显式 `close/shutdown`。
- 保持幂等，防止显式 cleanup 后再次 drop。
- 不获取可能形成锁顺序环的复杂锁。
- 不调用未知用户 callback。

安全敏感恢复失败不能只静默忽略；需要日志、指标、下一次启动修复或事务式设计。

## 锁 guard、borrow guard 与 await

RAII 让 guard 自动释放，但精确释放点仍是行为：

```rust
let snapshot = {
    let state = state.lock().await;
    state.snapshot()
}; // guard 在这里 drop

send(snapshot).await;
```

用 `drop(guard)` 可显式释放，但小作用域通常更清楚，也更容易让编译器判断 future 的 `Send`。内部可变性与锁见 [15](./15-interior-mutability-and-locks.md)。

## 测试宏与 guard

宏测试关注展开后的契约：

- 代表性合法输入能编译并运行。
- 边界 token、尾逗号、泛型 type 能处理。
- 非法输入产生可理解 compile-fail 诊断。
- 跨 crate 调用时 `$crate` 路径正确。
- 生成 impl 不引入意外 bound。

guard 测试关注每个退出路径：

- 正常返回恢复。
- `Result::Err`/`?` 恢复。
- `catch_unwind` 下恢复，仅适用于 unwind profile。
- future abort/cancel 后恢复。
- 显式 cleanup + Drop 不重复执行。
- cleanup 失败可观察。

## 阅读练习

1. 搜索 `register_resource!` 的调用点，选一个手写展开后的 impl，再从 `ResourceType::ID` 找到消费方。
2. 对 [`spawn_session_actor`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/spawn.rs) 的 `#[tracing::instrument]` 写出宏大致包裹出的 span/instrument 结构，并列出它避免记录的参数。
3. 阅读 [`PlanGuard`](../../crates/codegen/xai-grok-shell/src/session/goal_strategist.rs) 的构造、显式 restore 与 Drop，画出成功、错误、取消、symlink tamper 四条路径。
4. 把 [`with_dark`](../../crates/codegen/xai-grok-config/src/signed_policy.rs) 中 `let _restore` 假设改成 `let _`，预测保护区间怎样变化。

完成标准：看到宏能还原其生成的普通 Rust 约束；看到 guard 能说明它拥有的资源、精确 Drop 时机、取消/abort 差异，以及为何还需要显式异步 shutdown。
