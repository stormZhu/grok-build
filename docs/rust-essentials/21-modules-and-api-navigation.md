# 21. 模块、可见性与 API 导航

在大型 workspace 中，读不动代码往往不是 Rust 语法问题，而是没有分清 package、crate、module、文件和公开 API。它们通常相关，但并非一一对应。

## 五个层级

```text
workspace（根 Cargo.toml）
  -> package（一个成员目录的 Cargo.toml）
       -> crate target（lib、bin、test、example、build script）
            -> module tree（mod / use / pub use）
                 -> item（struct、enum、trait、fn、const...）
```

- package 是 Cargo 的发布和依赖单位，名字可含连字符，如 `xai-tool-runtime`。
- Rust 路径里的 crate 名通常将连字符换成下划线：`xai_tool_runtime`。
- 一个 package 可以同时有 library 和 binary crate。
- module 是 Rust 的命名空间与可见性边界；文件只是承载 module 的常见方式。

## 从 `lib.rs` 读公开表面

[`xai-tool-runtime/src/lib.rs`](../../crates/common/xai-tool-runtime/src/lib.rs) 同时出现：

```rust
pub mod tool;
pub use tool::{Tool, ToolDyn, ToolStream};
```

第一句让调用者可走 `xai_tool_runtime::tool::Tool`。第二句把名字 re-export 到 crate 根，也可写成 `xai_tool_runtime::Tool`。

普通 `use` 只把名字引入当前作用域：

```rust
use crate::tool::Tool;
```

`pub use` 还在构造对外 API：

```rust
pub use crate::tool::Tool;
```

读 `lib.rs` 时先忽略实现，记录三件事：公开 module、crate 根 re-export、crate 级约束（例如 `#![forbid(unsafe_code)]`）。这就是 crate 的“门面”。

## 文件名不一定等于模块路径

常规映射：

```rust
mod prompt;              // prompt.rs 或 prompt/mod.rs
```

本仓库的大型模块也常用显式路径拆分 `impl`：

```rust
#[path = "acp_session_impl/run_loop.rs"]
mod run_loop;
use run_loop::*;
```

因此 [`acp_session_impl/run_loop.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs) 中可以写 `impl SessionActor`，即使 `SessionActor` 主定义不在该文件。模块系统按 `#[path]` 和 `mod` 形成树，不按编辑器目录视图自动形成树。

当一个文件开头是 `use super::*;` 时：

1. 先找谁通过 `mod` 声明了这个文件。
2. 回到父模块看引入了哪些名字。
3. 再定位目标 struct 的真正定义。

不要假定当前文件包含理解它所需的全部 import。

## 可见性从哪里能访问

| 写法 | 可访问范围 |
| --- | --- |
| 无 `pub` | 当前 module 及其子 module 的隐私规则内 |
| `pub` | 只要所有父 module 也可达，就可从 crate 外访问 |
| `pub(crate)` | 当前 crate 内 |
| `pub(super)` | 直接父 module 内 |
| `pub(in crate::path)` | 指定祖先 module 内 |

“item 写了 `pub`”不保证外部一定能访问；通往它的 module 路径也必须公开，或者它被公开 re-export。

本仓库中 `pub(crate)` 很有信息量：它通常表示类型是 crate 内部协作契约，不应被其他 crate 绑定。修改前先搜索所有 crate 内调用点，不要因为它不是 public API 就认为影响小。

## `crate`、`self`、`super` 和外部 crate

```rust
use crate::context::ToolCallContext; // 当前 crate 根
use super::commands::SessionCommand; // 当前 module 的父 module
use self::types::State;              // 当前 module
use serde_json::Value;               // 依赖 crate
```

路径中的 `self` 与方法参数 `self` 拼写相同但语境不同。路径开头的 `self::` 是当前 module；方法中的 `&self` 是当前值。

## 找一个名字的可靠顺序

以 `ToolStream` 为例：

```sh
# 1. 精确找定义
rg -n '^(pub )?type ToolStream\b|^(pub )?(struct|enum|trait) ToolStream\b' crates

# 2. 看 crate 门面是否 re-export
rg -n 'pub use .*ToolStream|ToolStream,' crates/common/xai-tool-runtime/src/lib.rs

# 3. 看使用点；先限目标区域
rg -n '\bToolStream\b' crates/common/xai-tool-runtime crates/codegen/xai-grok-tools
```

如果名字是方法，搜索顺序是：

```text
fn method_name(
trait 中的 fn method_name(
impl Type / impl Trait for Type
调用点 .method_name(
```

方法可能来自 trait，而非类型的固有 `impl`。编辑器“Go to Definition”很有用，但你仍要看 trait 是否在作用域中，以及实际接收者类型是什么。

## 从调用点反推 API

当定义过于抽象时，先看调用方：

```rust
let mut stream = dispatch.call(tool_id, args, ctx).await;
while let Some(item) = stream.next().await { ... }
```

调用方告诉你：

- `call` 本身是 async，第一次 `.await` 得到 Stream。
- Stream 是可变的，因为 `next()` 需要推进内部状态。
- 每个 item 还需要按 Progress/Terminal 分支。
- API 的重要不变量是“终态恰好一次”，不只是返回类型能编译。

定义告诉你“允许什么”，调用方和测试告诉你“项目依赖什么”。两者都要读。

## 从 `Cargo.toml` 判断依赖方向

打开目标 package 的 manifest：

```toml
[dependencies]
xai-tool-protocol = { path = "../xai-tool-protocol" }
serde = { workspace = true }
```

这表示当前 crate 可以引用 `xai_tool_protocol`，反方向不成立，除非对方也声明依赖；若双方互相依赖，Cargo 会拒绝 cycle。

阅读分层时用下面的问题：

- 这是底层类型 crate、运行时契约、具体实现，还是应用编排？
- 一个类型为何放在这个 crate，而不是调用者里？
- 为避免依赖环，哪个边界把强类型转换成协议值或 trait？
- feature 是否让某个 module 或依赖只在特定构建出现？

可用 Cargo 自带命令确认：

```sh
cargo tree -p xai-tool-runtime --depth 1
cargo tree -p xai-grok-tools -i xai-tool-runtime
cargo metadata --no-deps --format-version 1
```

第一条看直接依赖，第二条看指定 crate 的反向依赖者，第三条看 package/target/feature 元数据。

## feature 和 `cfg` 会改变你正在读的程序

```rust
#[cfg(test)]
mod tests;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(feature = "foo")]
pub mod foo;
```

某个符号“明明存在却找不到”时检查：

1. 当前 target OS。
2. 当前构建是否是 test。
3. package 的默认 feature。
4. 上层依赖是否用 `features = [...]` 或关闭 default features。

`cargo test`、`cargo check` 和 IDE 默认分析的 cfg 集合不一定完全相同。

## 宏生成的名字怎样找

搜索不到 `impl SomeTrait for Type` 时，检查：

- `#[derive(SomeTrait)]` 是否生成实现。
- `macro_rules!` 或过程宏调用是否接收了该类型。
- `include!`、build script、protobuf 生成文件是否参与构建。
- trait 是否通过 blanket impl 提供，例如 `impl<T: X> Y for T`。

先搜索 trait 名和类型名，不要只搜索完整 `impl` 文本：

```sh
rg -n '\bSerialize\b|\bMyType\b' path/to/crate
```

确需看展开时可使用 rust-analyzer 的 macro expansion，或安装 `cargo-expand` 后运行 `cargo expand -p <package> module::path`。展开代码用于理解，不应直接编辑。

## 大文件的三遍阅读法

[`acp_session.rs`](../../crates/codegen/xai-grok-shell/src/session/acp_session.rs) 和其拆分实现很大，不适合从第一行顺读到末尾。

第一遍只建地图：

```text
module 声明 -> 核心 struct -> handle -> command enum -> run loop
```

第二遍只追一个行为：

```text
SessionHandle 发送 Prompt
  -> SessionCommand::Prompt
  -> run_session match 分支
  -> handle_prompt
  -> completion oneshot / notification
```

第三遍才查横切约束：取消、持久化、tracing、权限和错误转换。一次混读所有横切关注点，会让控制流看起来比实际更复杂。

## 建议保留的源码笔记模板

```text
符号：ToolDispatch::call_terminal
定义：crates/common/xai-tool-runtime/src/dispatch.rs
公开路径：xai_tool_runtime::ToolDispatch
输入所有权：ToolId、JSON args、ToolCallContext 都按值传入
输出：Future -> Result<TypedToolOutput, ToolError>
下游依赖：需要 Progress* + Terminal 的流不变量
错误边界：无 Terminal 会转为 stream_no_terminal
验证：xai-tool-runtime 中 tool streaming / dyn tests
仍不确定：多 Terminal 时是否忽略第二个（当前在第一个处 return）
```

“仍不确定”不是失败，它能阻止你把推测写成事实，并自然导向下一次搜索或测试。

## 导航练习

1. 从 [`xai-grok-agent/src/lib.rs`](../../crates/codegen/xai-grok-agent/src/lib.rs) 找到 `AgentBuilder` 的真实定义，并列出它直接使用的三个下游 crate。
2. 从 [`xai-tool-runtime/src/lib.rs`](../../crates/common/xai-tool-runtime/src/lib.rs) 找到 `ToolError`，解释为什么 `source` 不会进入序列化结果。
3. 从 [`SessionHandle`](../../crates/codegen/xai-grok-shell/src/session/handle.rs) 找到 `SessionCommand`，再找 actor 主循环；说明 handle/actor 分离解决了什么所有权问题。
4. 搜索 `ToolDispatch` 的实现。区分 trait 定义、具体 impl、`Arc<dyn ToolDispatch>` 使用点和测试替身。

完成标准不是“找到了文件”，而是能写出公开路径、真实定义、依赖方向和一个行为不变量。
