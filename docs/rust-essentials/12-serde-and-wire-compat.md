# 12. Serde、配置与线协议兼容性

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

- [`GoalStatus`](../../crates/codegen/xai-grok-shell/src/session/goal_tracker.rs) 为了线协议稳定性手写反序列化，并对序列化命名给出注释。
- [`xai-chat-state` 类型](../../crates/codegen/xai-chat-state/src/types.rs) 展示会话状态的派生序列化。
- [`xai-grok-config` loader](../../crates/codegen/xai-grok-config/src/loader.rs) 展示配置文件解析、错误报告和层叠加载。

### 仓库代码摘录：兼容旧状态且默认安全

[`GoalStatus`](../../crates/codegen/xai-grok-shell/src/session/goal_tracker.rs) 新写入使用 snake_case，仍接收历史名称：

```rust
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    #[serde(alias = "Active")]
    Active,
    #[serde(alias = "Paused")]
    UserPaused,
}
```

其自定义反序列化把未知状态恢复为可暂停状态而不是 `Active`。这体现了协议演进时的安全决策，而非单纯的 Serde 技巧。

## 常见属性

- `rename` / `rename_all`：将 Rust 标识符映射到稳定 wire 名称。
- `default`：为旧文件缺失的新字段提供兼容值；默认值必须有明确业务语义。
- `skip_serializing_if`：省略可选字段，但要确认接收端区分“缺失”和“空值”的方式。
- 自定义 `Deserialize`：仅在 derive 无法表达兼容规则或验证条件时使用，并为旧/新输入各写测试。

## 阅读检查点

找一个已持久化的类型，写出“加一个字段、改一个字段名、加一个 enum 变体”分别会影响的读写双方和测试。
