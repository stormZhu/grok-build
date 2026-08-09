# 18. 宏、属性与 RAII

宏在调用点生成代码。阅读时先识别它属于 derive、属性宏还是声明式宏，再用展开后的“普通 Rust”理解其行为。

| 形式 | 用途 |
| --- | --- |
| `#[derive(...)]` | 为类型生成 trait 实现，例如 Serde、Debug、Parser |
| `#[attribute]` | 改变 item 的生成或注册方式，例如 `#[tokio::test]`、`#[async_trait]` |
| `macro_rules!` | 在本 crate 或跨 crate 复用语法模板 |

不要把宏隐藏的 I/O、分配、panic 或 trait bound 当作零成本；遇到报错可借助 IDE 展开或从宏文档确认生成的约束。

## RAII 与 `Drop`

资源应由拥有它的值在离开作用域时释放。guard 可以确保临时目录、锁、环境恢复或任务清理在早返回和 `?` 路径上仍会执行。

```rust
struct Restore<'a> { target: &'a mut bool, old: bool }
impl Drop for Restore<'_> {
    fn drop(&mut self) { *self.target = self.old; }
}
```

`Drop` 不能 async，因此异步关闭必须有显式 `shutdown`/`flush`/`join` 阶段；不要指望析构完成网络或磁盘写入。

## 项目中的锚点

- [`register_resource!`](../../crates/codegen/xai-grok-tools/src/types/resources.rs#L60) 含声明式资源注册宏和 async trait。
- [`PlanGuard::drop`](../../crates/codegen/xai-grok-shell/src/session/goal_strategist.rs#L536) 展示作用域清理。
- [`Restore::drop`](../../crates/codegen/xai-grok-config/src/signed_policy.rs#L113) 的测试辅助 guard 展示状态恢复。

### 项目关键代码：宏生成 trait 实现

[`register_resource!`](../../crates/codegen/xai-grok-tools/src/types/resources.rs#L60) 不是运行时注册，而是在编译时为某个类型补齐实现：

```rust
// 源码节选：调用 register_resource!("ns", "name", MyType) 后，
// 编译器看到的核心结果是下列 impl。
macro_rules! register_resource {
    ($namespace:literal, $name:literal, $ty:ty) => {
        impl $crate::types::resources::ResourceType for $ty {
            // concat! 在编译期拼接静态资源 ID，无运行时字符串分配。
            const ID: &'static str = concat!($namespace, ".", $name);
        }
    };
}
```

阅读宏调用时，应先把它展开为这个 `impl`，再检查被实现类型是否满足 `ResourceType` 的其余约束。

### 仓库代码摘录：取消时仍恢复计划文件

[`PlanGuard::drop`](../../crates/codegen/xai-grok-shell/src/session/goal_strategist.rs#L536) 的析构实现负责兜底：

```rust
// 源码节选：Drop 是取消与提前返回时的最后一道同步恢复保障。
impl Drop for PlanGuard<'_> {
    fn drop(&mut self) {
        // restore 已成功执行过时返回 None，避免重复恢复。
        if let Some(reason) = self.restore() {
            // Drop 不能 await，因此这里只做同步恢复和错误记录。
            tracing::error!(reason = reason.as_const_str(), "plan restore failed");
        }
    }
}
```

future 在 `.await` 间被取消也会 drop 局部值，因此该 guard 把“计划文件必须恢复”的不变量放进所有提前退出路径。`Drop` 中只做同步恢复和记录，不尝试 async I/O。

### 项目关键代码：测试中的线程局部状态也由 guard 恢复

[`with_dark`](../../crates/codegen/xai-grok-config/src/signed_policy.rs#L109) 在闭包退出时恢复此前的 override：

```rust
LOCAL_OVERRIDE.with(|cell| {
    let prev = cell.replace(Some(Some(Vec::new())));

    struct Restore(Option<KeyOverride>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0.take();
            LOCAL_OVERRIDE.with(|cell| *cell.borrow_mut() = prev);
        }
    }

    let _restore = Restore(prev); // 即使 f() panic 或提前返回也会恢复。
    f()
})
```

这类局部 guard 比在每个分支手写“恢复旧值”可靠，尤其适合测试和临时环境覆盖。

## 阅读检查点

对一个 guard 说明它保护的资源、构造后哪些路径会触发 Drop、以及为什么没有显式调用者也能保证恢复。
