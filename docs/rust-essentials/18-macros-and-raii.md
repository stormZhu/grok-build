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

### 仓库代码摘录：取消时仍恢复计划文件

[`PlanGuard::drop`](../../crates/codegen/xai-grok-shell/src/session/goal_strategist.rs#L536) 的析构实现负责兜底：

```rust
impl Drop for PlanGuard<'_> {
    fn drop(&mut self) {
        if let Some(reason) = self.restore() {
            tracing::error!(reason = reason.as_const_str(), "plan restore failed");
        }
    }
}
```

future 在 `.await` 间被取消也会 drop 局部值，因此该 guard 把“计划文件必须恢复”的不变量放进所有提前退出路径。`Drop` 中只做同步恢复和记录，不尝试 async I/O。

## 阅读检查点

对一个 guard 说明它保护的资源、构造后哪些路径会触发 Drop、以及为什么没有显式调用者也能保证恢复。
