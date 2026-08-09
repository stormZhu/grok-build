# 6. Cargo workspace、package、target、feature 与验证范围

本仓库是约 80 个 manifest 组成的 Rust workspace。Cargo 决定哪些 crate/target 被编译、依赖版本与 feature 如何解析、build script 何时运行。读源码前若没分清 package、crate、target 和 module，很容易跑错命令或误判影响面。

## 五层概念

```text
workspace
  -> package（一个 Cargo.toml）
       -> target（lib / bin / integration test / example / bench / build script）
            -> crate（每个 target 独立编译成一个 crate）
                 -> module（Rust mod 树）
```

- Package 名可含连字符：`xai-tool-runtime`，用于 `cargo -p`。
- Rust crate 路径通常把连字符换成下划线：`xai_tool_runtime::Tool`。
- 一个 package 可同时有 lib、多个 bin 和多个 integration test target。
- `src/lib.rs`、`src/main.rs`、`tests/foo.rs` 各自是 crate root。
- module 不等于 package；一个 crate 内可以有大量 module。

详见 [21 模块与 API 导航](./21-modules-and-api-navigation.md)。

## workspace 根是 virtual manifest

根 [`Cargo.toml`](../../Cargo.toml) 有 `[workspace]` 而没有普通 `[package]`，用于：

- 列出 members。
- 统一第三方依赖版本与常用 feature。
- 共享 edition/license/lint/profile。
- 提供 crates.io patch。

本仓库根文件标明为自动生成。日常功能改动应从目标 package 的 manifest 开始；需要新增 workspace member、统一依赖或全局 lint 时，应找到上游生成源/维护流程，不能假定直接编辑同步产物会长期保留。

## package manifest 怎样读

先按顺序扫描：

```toml
[package]              # package 名、edition、build script
[lib] / [[bin]]        # 非默认 target、required-features
[dependencies]         # 正常 target 依赖
[dev-dependencies]     # test/example/bench 常用依赖
[build-dependencies]   # build.rs 自己的依赖
[target.'cfg(...)'.dependencies]
[features]
[lints]
```

不要从源码 `use foo::Bar` 就猜依赖来源：它可能是本 package 直接依赖、re-export、target-specific 依赖、生成代码，或只在测试 cfg 下存在。

## workspace dependency 继承

成员 manifest 常写：

```toml
[dependencies]
serde = { workspace = true }
tokio = { workspace = true, features = ["sync", "rt"] }
xai-tool-protocol = { workspace = true }
```

含义：

- 版本/source/path 等主要定义来自 `[workspace.dependencies]`。
- member 可声明自己需要的额外 feature。
- path 依赖仍形成普通 crate 依赖边，Cargo 禁止循环。
- `workspace = true` 不表示该依赖自动出现在所有 package；每个 member 仍须声明直接使用的依赖。

新增第三方依赖前：

```sh
rg -n '^crate_name\s*=|crate_name = ' Cargo.toml crates/**/Cargo.toml
cargo tree -p <package> -i crate_name
```

确认 workspace 是否已有版本、为何当前 package 需要直接依赖，以及能否复用上游 crate 的公开 API。不要为了一个 helper 复制出第二个版本或打开大批默认 feature。

## feature 是编译期能力图

```toml
[features]
default = []
dhat-heap = ["dep:dhat"]
test-support = []
local-workspace = []
```

- `default` 在调用者未关闭默认 feature 时启用。
- `dep:dhat` 激活 optional dependency，而不额外暴露同名隐式 feature。
- `other-crate/feature` 激活依赖的 feature。
- feature 通常应 additive：打开后增加能力，不改变同一 API 的相反语义。

仓库 [`xai-grok-shell/Cargo.toml`](../../crates/codegen/xai-grok-shell/Cargo.toml) 使用 `test-support` 控制测试辅助 target，`dhat-heap` 激活可选 profiler。[`xai-grok-pager/Cargo.toml`](../../crates/codegen/xai-grok-pager/Cargo.toml) 的默认 feature 包含 allocator/sandbox 行为，说明 `--no-default-features` 可能构建出不同产品。

## feature 会在依赖图中统一

同一 package/version 被多个路径依赖时，所需 feature 通常取并集：

```text
app -> library (feature a)
app -> adapter -> library (feature b)
effective library: feature a + b
```

所以“我的 manifest 没开 b”不代表最终没开。检查：

```sh
cargo tree -p <package> -e features
cargo tree -p <package> -i <dependency> -e features
```

根 workspace 使用 `resolver = "2"`，它减少 build/dev/target-specific feature 在不相关场景中的意外统一，但正常依赖图中的 feature 仍是 additive。不要把 resolver 2 理解成“每条依赖边独立编译一份 feature 集”。

## cfg 与 feature 是两层

```rust
#[cfg(feature = "test-support")]
pub mod test_support;

#[cfg(target_os = "linux")]
mod linux;
```

Manifest 也可按 target 声明：

```toml
[target.'cfg(unix)'.dependencies]
nix = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows = { workspace = true }
```

检查一个符号是否存在时要知道：目标 triple、feature set、test cfg 和 build script 输出。IDE 当前分析配置可能与 CI 的 Linux target 或无默认 feature 构建不同。

常用只读查询：

```sh
rustc -vV
cargo metadata --no-deps --format-version 1
cargo tree -p <package> --target all
```

`--target all` 适合查看依赖图，不表示当前主机能成功链接/运行所有平台 target。

## target 决定测试和依赖是否进入构建

默认布局：

```text
src/lib.rs          library target
src/main.rs         binary target
src/bin/name.rs     additional binary
tests/foo.rs        integration test target foo
examples/demo.rs    example target
benches/bench.rs    benchmark target
build.rs            build script target
```

每个 `tests/foo.rs` 是独立 crate：

- 只能通过 public API 使用 library，不能访问 `pub(crate)`/private item。
- `cargo test -p pkg --test foo` 精确选择这个 target。
- 名称过滤器 `cargo test -p pkg foo` 是过滤 test 函数名，可能运行 0 项；不是选择文件。

`cargo check -p pkg` 主要检查默认 lib/bin targets；修改测试辅助代码、examples 或 benches 时使用：

```sh
cargo check -p <package> --all-targets
```

这会引入 dev-dependencies，可能暴露默认 check 看不到的问题。

## dev-dependencies 不进入普通 library 契约

`tempfile`、mock server、pretty assertions 常只在测试需要，放在 `[dev-dependencies]`。但 integration test 使用 library 的 public API，library 生产代码不能反过来依赖 dev-dependency。

若 production API 暴露 test-only 类型，通常说明边界不清。仓库使用 feature `test-support` 的 target 时，明确检查 required-features 和下游测试如何启用它。

## build script 与生成代码

`build.rs` 在编译 package 前运行，是单独的 host crate；它使用 `[build-dependencies]`。常见职责：

- protobuf/code generation。
- 查询环境/平台并发出 `cargo:rustc-cfg`。
- 生成/下载构建资源。
- 发出 `cargo:rerun-if-changed`。

本仓库部分 package 依赖 hermetic `protoc`。即使只测试上层 crate，依赖闭包中的 build script 也会执行；因此构建失败可能发生在你没有编辑的 crate。

排障先看：

```text
failed to run custom build command for ...
--- stdout
--- stderr
```

确认工具、环境、rerun 输入和输出目录，不要把生成文件手工复制进源码绕过。

## Cargo.lock 与版本来源

Manifest 声明允许范围和来源，`Cargo.lock` 记录一次解析的精确版本。这个应用 workspace 应由 Cargo 更新 lockfile，不手工编辑。

依赖可能来自：

- crates.io version。
- workspace path。
- git URL + rev。
- `[patch.crates-io]` 覆盖。

根 manifest 对 `async-openai` 有 git patch，因此 `cargo tree` 显示的实际 source 可能与成员 manifest 表面版本不同。排查 API 差异时查 lock/tree，不只查 `[dependencies]` 一行。

## `-p` 选择顶层 package，不是只编译一个目录

```sh
cargo check -p xai-tool-runtime
```

Cargo 会检查该 package target 和完整依赖闭包，但不会自动检查所有反向依赖者。因此：

- 修改叶子实现：目标 package check/test 通常是第一层。
- 修改 public type/trait：还要检查直接/关键反向依赖者。
- 修改宏、共享协议或 feature：影响面可能横跨 workspace。
- 修改根依赖/lock/build tool：需要更广矩阵。

找反向依赖：

```sh
cargo tree -i xai-tool-runtime --workspace
rg -n 'xai-tool-runtime|xai_tool_runtime' crates/**/Cargo.toml crates --glob '*.rs'
```

`cargo tree -i` 给 Cargo 解析后的依赖图，`rg` 帮助定位具体使用 API；两种证据互补。

## check、build、test、clippy 各证明什么

| 命令 | 主要证明 | 不证明 |
| --- | --- | --- |
| `cargo check` | 类型检查默认 targets/依赖，生成 metadata | 最终链接、测试行为 |
| `cargo build` | 能编译并链接所选 targets | 测试断言、运行环境行为 |
| `cargo test --no-run` | 测试 targets 能编译/链接 | 测试执行通过 |
| `cargo test` | 所选测试实际运行 | 未选择 feature/平台/反向依赖 |
| `cargo clippy` | 所选 targets 满足 lint | 行为正确 |
| `cargo fmt --check` | Rust 格式符合 rustfmt | 编译、Markdown、生成代码正确 |

命令绿色只能支持其覆盖范围内的结论。记录 test 数量和 filtered count，避免“0 tests passed”。

## feature 验证矩阵

修改 gated 代码时至少考虑：

```sh
# 默认产品组合
cargo check -p <package> --all-targets

# 最小组合
cargo check -p <package> --all-targets --no-default-features

# 修改的特定能力
cargo check -p <package> --all-targets --no-default-features --features feature_name
```

不是每个 package 都支持任意 feature 组合；以 manifest 和 CI 矩阵为准。测试 target 有 `required-features` 时，未启用会被跳过而非失败，必须查看 Cargo 输出。

## profile、panic 与行为差异

根 manifest 定义 dev/release/bench profiles。`panic = "abort"` 与 unwind 会改变 panic 清理、二进制和测试/生产差异；优化、LTO、debug assertions 也可能影响行为与性能。

不要用 dev 性能推断 release，也不要依赖 debug-only overflow panic 作为业务验证。性能问题见 [19 平台、unsafe 与性能](./19-platform-unsafe-performance.md)。

## lint 是 workspace 契约

成员通常写：

```toml
[lints]
workspace = true
```

根 `[workspace.lints]` 统一 rustc/clippy policy。局部 allow 应说明生成代码、平台差异或已知上游原因；不要为让 CI 绿而给整个 crate 添加宽泛 allow。

## 日常反馈循环

```sh
# 1. 定位 package 名和 features
sed -n '1,180p' path/to/Cargo.toml

# 2. 最快类型反馈
cargo check -p <package>

# 3. 精确行为测试（优先 target + test name）
cargo test -p <package> --test <integration-target> -- <exact_name> --exact

# 4. 目标 package 全测试
cargo test -p <package>

# 5. 修改 public API 时检查关键下游
cargo check -p <direct-consumer>

# 6. 提交前按仓库约定扩大 fmt/clippy/feature/platform 范围
```

集成测试的完整测试名可能包含模块路径，先 `--list`：

```sh
cargo test -p <package> --test <target> -- --list
```

## 按改动类型选择范围

| 改动 | 第一层 | 扩大条件 |
| --- | --- | --- |
| 私有纯函数 | 同 module unit test | 公共行为变化 |
| public struct/enum/trait | 定义 crate + compile/tests | 直接反向依赖、协议/derive 变化 |
| Serde/wire 类型 | exact/round-trip/invalid tests | producer/consumer、旧 fixture |
| feature/cfg | 改动组合 check | 默认、最小、平台 CI |
| build.rs/proto | package build/check | 所有消费生成 API 的 crate |
| workspace dependency/lock | 受影响 packages | CI workspace matrix |
| proc macro | macro crate tests | 多个代表性展开调用者 |

“全 workspace”不是第一反应，也不能替代 targeted test；它适合共享面改动后的最后一层证据。

## 阅读一个陌生 package 的十分钟流程

1. 看 `[package]` 名、edition、build。
2. 看 lib/bin/test targets 和 required-features。
3. 看直接本地依赖，判断它位于哪一层。
4. 看 features/default 和 target-specific deps。
5. 打开 `src/lib.rs` 的 public modules/re-exports。
6. 用 `cargo tree -p pkg --depth 1` 核对依赖。
7. 用 `cargo tree -i pkg --workspace` 看关键使用者。
8. 列测试 target：`cargo test -p pkg -- --list`（会构建）或先 `rg --files`。
9. 写下最小 check、focused test 和扩大范围。

## 动手练习

选择 `xai-tool-runtime`：

```sh
cargo metadata --no-deps --format-version 1
cargo tree -p xai-tool-runtime --depth 1
cargo tree -i xai-tool-runtime --workspace
cargo test -p xai-tool-runtime --test trait_object_safety -- --list
```

回答：package 名与 Rust crate 名为何不同？有哪些 integration test target？`serde_json` 是 direct dependency 还是 transitive？哪些 crate 反向依赖它？运行 `cargo test -p ... trait_object_safety` 为什么可能是 0 tests，而 `--test trait_object_safety` 会选中目标？

再选择一个有 `[features]` 和 target-specific dependencies 的 package，例如 [`xai-grok-shell`](../../crates/codegen/xai-grok-shell/Cargo.toml)，为默认、`test-support`、Linux/Windows 四种场景列出哪些 target/dependency 会变化。

完成标准：改代码前能准确写出 package、target、feature set、依赖闭包、反向依赖和验证命令，而不是只说“在这个目录跑 cargo test”。
