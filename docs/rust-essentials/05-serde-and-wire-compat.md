# 5. Serde、配置与线协议兼容性

`Serialize` / `Deserialize` 让 Rust 类型跨越 JSON、TOML、持久化文件和协议边界。对外数据的字段名与缺失字段行为是契约，不能把 derive 当作纯内部实现细节。

```rust
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status { Active, Paused }

#[derive(serde::Deserialize)]
struct Config {
    #[serde(default)]
    enabled: bool,
}
```

## 修改前检查

1. 当前数据由谁写入、由谁读取，是否跨版本保存或发送？
2. 修改 enum 变体或字段名会不会改变 wire format？
3. 新字段在旧数据中缺失时，是否应有 `#[serde(default)]`？
4. 未知字段是应拒绝、忽略，还是保留？按协议和安全要求决定，不要一概宽容。

## 项目中的锚点

- [`GoalStatus`](../../crates/codegen/xai-grok-shell/src/session/goal_tracker.rs#L64) 为了线协议稳定性手写反序列化，并对序列化命名给出注释。
- [`ChatStateSnapshot`](../../crates/codegen/xai-chat-state/src/types.rs#L30) 展示会话状态的派生序列化。
- [`load_toml_file`](../../crates/codegen/xai-grok-config/src/loader.rs#L38) 展示配置文件解析、错误报告和层叠加载。

### 仓库代码摘录：兼容旧状态且默认安全

[`GoalStatus`](../../crates/codegen/xai-grok-shell/src/session/goal_tracker.rs#L62) 新写入使用 snake_case，仍接收历史名称：

```rust
// 源码节选：新写入统一为 snake_case，同时仍接受旧版本的 PascalCase。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[serde(alias = "Active")]
    Active,
    #[serde(alias = "Paused")]
    UserPaused,
}
```

其自定义反序列化将 wire 字符串交给 `from_wire_str`：

```rust
// 源码节选：未知值不会让恢复后的目标自动继续执行。
let s = String::deserialize(deserializer)?;
Ok(Self::from_wire_str(&s))
// from_wire_str 的兜底分支：_ => Self::UserPaused
```

这体现了协议演进时的安全决策，而非单纯的 Serde 技巧。

### 项目关键代码：配置解析后的敏感信息保护

[`load_toml_file`](../../crates/codegen/xai-grok-config/src/loader.rs#L38) 和同文件的错误格式化分开处理加载与安全日志：

```rust
pub fn load_toml_file(path: &Path) -> std::io::Result<toml::Value> {
    let mut v = read_toml_file(path)?;
    // 只在成功解析后展开环境变量，避免把原始敏感配置散播到错误文本。
    expand_env_vars_in_toml(&mut v);
    Ok(v)
}

// 解析错误只保留行、列和消息；不回显可能含密钥的原始 TOML 行。
pub fn toml_error_detail(src: &str, e: &toml::de::Error) -> String { /* ... */ }
```

## 常见属性

- `rename` / `rename_all`：将 Rust 标识符映射到稳定 wire 名称。
- `default`：为旧文件缺失的新字段提供兼容值；默认值必须有明确业务语义。
- `skip_serializing_if`：省略可选字段，但要确认接收端区分“缺失”和“空值”的方式。
- 自定义 `Deserialize`：仅在 derive 无法表达兼容规则或验证条件时使用，并为旧/新输入各写测试。

## 阅读检查点

找一个已持久化的类型，写出“加一个字段、改一个字段名、加一个 enum 变体”分别会影响的读写双方和测试。
