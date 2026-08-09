# 6. Cargo workspace、feature 与验证范围

本仓库是一个多 crate workspace。把 package 当成独立构建与依赖边界，而不是把 `crates/` 当作普通源码子目录。

## 日常循环

```sh
cargo check -p xai-grok-tools
cargo test -p xai-grok-tools <test-filter>
cargo clippy -p xai-grok-tools
cargo fmt --all -- --check
```

`-p` 只选择顶层 package，不会跳过其依赖闭包；它避免了检查无关反向依赖。修改共享类型、feature 或宏后，再扩大到直接使用者或 CI 约定的范围。

## manifest 规则

- 根 `Cargo.toml` 标记为自动生成，日常功能改动不要手工修改它。
- 目标 crate 的 `Cargo.toml` 负责直接依赖；已有统一版本的依赖用 `workspace = true`。
- feature 是编译期能力开关，修改 feature 前检查默认 feature、可选依赖和所有受支持组合。
- 不以“先让它编过”为理由新增重复依赖或打开不必要的默认 feature。

## 项目中的锚点

- 根 [`Cargo.toml`](../../Cargo.toml#L6) 定义 workspace 成员、统一依赖与生成限制。
- [`xai-grok-tools/Cargo.toml`](../../crates/codegen/xai-grok-tools/Cargo.toml#L8) 是一个包含工具运行时依赖的实际 crate manifest。
- [`rust-toolchain.toml`](../../rust-toolchain.toml#L11) 固定 Rust 1.94、rustfmt、clippy 和目标平台。

### 项目关键配置：版本由 workspace 统一

目标 crate 的 manifest 使用 workspace 依赖时，版本不在此处重复声明：

```toml
# crates/codegen/xai-grok-tools/Cargo.toml 的真实依赖声明节选。
[dependencies]
serde = { workspace = true }
tokio = { workspace = true, features = [
    "io-std", "process", "rt", "macros", "rt-multi-thread", "net", "sync",
] }
```

`workspace = true` 表示版本来自根 workspace；`tokio` 的 feature 则由这个 crate 显式收窄到所需能力。新增依赖前先查根 manifest 是否已有同一 crate；本公开同步树的根 manifest 是生成物，不能把它当作普通编辑入口。

根 manifest 则提供 workspace 级边界：

```toml
# Cargo.toml 源码节选：根文件只声明 workspace 结构和统一依赖版本。
[workspace]
resolver = "2"
members = [
    "crates/codegen/xai-grok-shell",
    "crates/codegen/xai-grok-tools",
    # ... 其余 member
]

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
```

这解释了为什么目标 crate 写 `serde = { workspace = true }`：它继承版本，但仍在自己的 manifest 决定是否需要额外 feature。

## 阅读检查点

改动前列出：所在 package、直接依赖者、需要的 feature、最快的 check 与最能证明行为的 test。只有这些答案明确后再扩大构建范围。
