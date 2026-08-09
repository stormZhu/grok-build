# 5. Serde、配置、持久化与 wire 兼容

`Serialize` / `Deserialize` 让 Rust 类型跨越 JSON、TOML、持久化文件和网络协议。进入外部数据边界后，字段名、tag、缺失/null、未知值和默认行为都是契约；derive 生成的是协议代码，不是无关紧要的样板。

读这类代码时分四层：输入语法能否解析、wire shape 如何映射、业务值是否合法、版本/安全策略如何处理未知数据。

## Rust 类型不等于 wire shape

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Active,
    UserPaused,
}
```

Rust variant 是 `UserPaused`，JSON 字符串是 `"user_paused"`。重命名 Rust 标识符未必必须改变 wire；修改 `rename_all` 则一定可能改变协议。

对每个序列化类型写出：

```text
Rust shape：enum/struct/newtype/泛型
wire format：JSON/TOML/JSONL/协议 frame
writer：哪些版本/进程/crate
reader：哪些版本/进程/crate
存活时间：单次请求 / 跨进程 / 长期落盘
兼容策略：严格拒绝 / 默认 / alias / 保留未知值
```

落盘 session 和跨版本网络数据的兼容要求，远高于进程内临时 JSON。

## 字段属性的双向影响

```rust
#[derive(Serialize, Deserialize)]
struct Config {
    #[serde(default)]
    enabled: bool,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,

    #[serde(rename = "toolName", alias = "tool_name")]
    tool_name: String,
}
```

| 属性 | 写出时 | 读入时 | 风险问题 |
| --- | --- | --- | --- |
| `rename` | 使用指定名称 | 只接受指定名称（另有 alias 除外） | 是否破坏旧 writer/reader |
| `alias` | 不影响新写出 | 额外接受旧名称 | alias 何时可以移除 |
| `default` | 无影响 | 缺失时构造默认值 | 默认是否安全、能否区分旧数据 |
| `skip_serializing_if` | 条件成立时省略 | 无直接影响 | 对端是否区分缺失/null/空值 |
| `skip` | 不写出 | 读入使用默认 | round-trip 是否有意丢信息 |
| `skip_serializing` | 不写出 | 仍尝试读入 | 常用于只读兼容字段 |
| `skip_deserializing` | 仍写出 | 忽略输入并默认 | 是否让外部无法注入内部字段 |

`Option<T>` 字段缺失时通常自动得到 `None`；显式 `#[serde(default)]` 可强调兼容意图，且在与 `deserialize_with` 组合时常是必需的，因为缺失字段不会调用自定义 deserializer。

## 缺失、null、false 和空集合不同

```json
{}
{"enabled": null}
{"enabled": false}
```

这三种输入是否等价由契约决定：

- 缺失可能表示“旧 writer 不知道该字段”或“继承上层配置”。
- null 可能表示“显式清除”，也可能不合法。
- false 是明确值。

普通 `Option<bool>` 通常把缺失和 null 都读成 `None`。若业务需要三态，可使用嵌套 Option、自定义 deserializer 或专门 enum，并写测试锁定行为。不要仅凭 Rust 字段类型猜配置 merge 语义。

同理，`[]`、缺失和 null 对列表也可能分别表示“显式清空”“继承”“无值”。

## enum 的四种常见表示

假设：

```rust
enum Event {
    Text { value: String },
    Stop,
}
```

### Externally tagged（默认）

```json
{"Text":{"value":"hi"}}
"Stop"
```

### Internally tagged

```rust
#[serde(tag = "type", rename_all = "snake_case")]
```

```json
{"type":"text","value":"hi"}
{"type":"stop"}
```

### Adjacently tagged

```rust
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
```

```json
{"type":"text","data":{"value":"hi"}}
{"type":"stop"}
```

### Untagged

```rust
#[serde(untagged)]
enum Id { String(String), Number(i64) }
```

Serde 按 variant 顺序尝试匹配。它适合 JSON-RPC id 这类天然由 shape 区分的值，但有风险：新增宽泛 variant 或字段默认后，旧输入可能被更早分支“误成功”解析。修改 untagged enum 时要用歧义输入测试 variant 选择。

仓库 [`JsonRpcId`](../../crates/common/xai-tool-protocol/src/envelope.rs) 接受字符串或数字，就是有明确协议依据的 untagged enum。

## 未知字段与未知 variant

Serde struct 默认忽略未知字段，有利于新 writer → 旧 reader 的 additive compatibility；但安全敏感配置、拼写错误必须尽早暴露时可使用：

```rust
#[serde(deny_unknown_fields)]
```

选择不是全局风格：

| 边界 | 常见选择 |
| --- | --- |
| 长期演进的网络 response | 常允许未知字段 |
| 用户配置且 typo 危险 | 可能拒绝未知字段 |
| 工具参数 | 依据 schema 与是否允许前向字段 |
| 安全/权限策略 | 通常严格或 fail closed |

`flatten` 把子 struct/map 字段并入同一对象，可用于收集扩展字段，但会模糊所有权与冲突规则；Serde 不支持把 `flatten` 与 `deny_unknown_fields` 任意组合。读到 `flatten` 时确认重复 key、未知字段和 schema 生成行为。

未知 enum variant 比未知字段更难兼容，因为 reader 必须选择一个 Rust variant。`#[serde(other)]` 可给特定 enum representation 提供 unit fallback，但会丢掉未知 payload；若需要代理转发，可能要显式 `Unknown { raw: Value }` 或保存原始数据。

Rust 的 `#[non_exhaustive]` 只影响 Rust 调用者 match，不自动解决 Serde wire 的未知 variant。

## alias 是迁移工具，不是无限期垃圾桶

```rust
#[serde(rename = "opencode", alias = "OpenCode", alias = "open_code")]
```

新 writer 始终输出 canonical 名称，reader 临时接受历史拼写。维护时记录：

- 哪个版本开始写新名称。
- 是否有长期落盘数据永远需要旧 alias。
- alias 是否产生两个字段同时出现的冲突。
- telemetry 能否观察旧输入逐渐消失。

删除 alias 前不能只看当前代码；旧 session、配置、客户端和缓存仍可能存在。

## `default` 是业务决策

```rust
#[serde(default)]
pub yolo_mode: Option<bool>,
```

默认值必须回答“旧 writer 没发此字段时最安全行为是什么”。对权限、自动执行和数据删除功能通常 fail closed；对纯展示字段可使用无害默认。

`Default::default()` 方便，但可能把多个字段的兼容政策藏在一个 impl 中。非平凡字段可写具名函数：

```rust
#[serde(default = "default_max_attempts")]
max_attempts: usize,
```

测试不仅断言结果，还应说明为什么这个默认保持旧行为或安全行为。

## 自定义 Deserialize：格式与验证分层

仓库 [`JsonRpcVersion`](../../crates/common/xai-tool-protocol/src/envelope.rs) 是零大小 Rust 类型，wire 上必须是精确字符串 `"2.0"`。它的 Visitor：

```rust
impl<'de> Deserialize<'de> for JsonRpcVersion {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl de::Visitor<'_> for V {
            type Value = JsonRpcVersion;

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "2.0" {
                    Ok(JsonRpcVersion)
                } else {
                    Err(E::custom(format!("expected jsonrpc 2.0, got {value:?}")))
                }
            }
        }
        deserializer.deserialize_str(V)
    }
}
```

这种实现把协议常量编码为类型不变量：成功构造的 `JsonRpcVersion` 不可能表示其他版本。

自定义 deserializer 保持三层：

1. Serde 读取输入 shape。
2. 纯函数转换/验证值。
3. 构造业务类型或返回具体错误。

纯转换函数易于表驱动测试，Visitor 只负责适配 Serde API。

## 宽容解析必须有边界

[`serde_lenient.rs`](../../crates/common/xai-tool-types/src/serde_lenient.rs) 有意接受模型可能生成的布尔形状：

```text
true / "true" / "yes" / "1" / 1
false / "false" / "no" / "0" / 0 / null
```

同时拒绝 `2`、对象、数组和未知字符串。宽容不是“任何东西都猜”：

- 列出有限且测试覆盖的 accepted forms。
- 对未知输入返回错误，不静默 default。
- 区分缺失字段与显式 null。
- 只在 producer 已知不稳定、且歧义风险可接受的边界使用。
- 安全/权限字段的容错必须 fail closed。

对应测试同时覆盖有效、边界和拒绝集合，是自定义反序列化的推荐形状。

## 手写 response 保证 XOR 不变量

JSON-RPC response 要求 `result` 与 `error` 恰好出现一个。简单 struct：

```rust
struct WeakResponse<R> {
    result: Option<R>,
    error: Option<JsonRpcError>,
}
```

能在内存中表示 both/none。仓库 [`JsonRpcResponse`](../../crates/common/xai-tool-protocol/src/envelope.rs) 内部改用：

```rust
enum ResponseOutcome<R> {
    Result(R),
    Error(JsonRpcError),
}
```

手写 Serialize 只写一个 key；Deserialize 先读 flat helper，再 match `(result, error)`，对 both/none 返回协议错误。这是“Rust 内部非法状态不可表示 + 外部非法输入明确拒绝”的完整边界。

helper 中 `result: Option<R>` 缺失可得 None，避免无意引入 `R: Default` bound。自定义泛型 Serde 代码要留意 derive 推导出的 bounds 是否比业务真正需要更强。

## `serde_json::Value` 应停留在动态边界

`Value` 适合：

- 尚不知道 tool id 对应具体 Args 的 registry 边界。
- 原样代理未知扩展数据。
- error details 等开放结构化元数据。

进入已知业务逻辑后尽早转成具体类型，避免到处 `get("name")`、字符串比较和运行时类型错误。离开业务层前再序列化。

仓库 `Tool -> ToolDyn` 先把 JSON Value 反序列化成 `T::Args`，执行强类型工具，再擦除输出；见 [03 trait](./03-traits-generics-and-dyn.md)。

## 序列化与 schema 必须同步

工具参数通常同时实现 `Deserialize` 和 `schemars::JsonSchema`。自定义 `deserialize_with` 可以接受比 schema 更多的 shape；这可能是有意容错，也可能导致“模型按 schema 生成”和“运行时实际接受”漂移。

修改时检查：

- Serde rename/tag/default。
- JsonSchema 字段名、required、enum 和 default。
- prompt/tool description 是否描述相同约束。
- 旧客户端实际发送的 shape。
- validation error 是否定位字段。

不要只让 serde_json 测试通过。

## 敏感数据与错误文本

Deserialize error 可能包含 offending value、路径或输入片段。配置中含 token/密钥时：

- 不把整份原始 TOML/JSON放进 anyhow context。
- 错误尽量包含文件、行列、字段路径和安全消息。
- Debug/trace 日志避免直接记录完整配置结构。
- `#[serde(skip)]` 只控制序列化，不自动控制 Debug。
- secret 类型应有 redacted Debug/Display 或专门日志策略。

仓库配置 loader 将 TOML 解析与环境变量展开、错误格式化分离；这类边界不能用 `format!("{config:?}")` 排障。

## 兼容矩阵

新增字段至少分析四格：

| writer / reader | 旧 reader | 新 reader |
| --- | --- | --- |
| 旧 writer | 基线行为 | 新字段缺失时 default/None |
| 新 writer | 未知字段是忽略还是拒绝 | 新行为 |

修改 enum/tag/字段名还要考虑：

- 历史落盘 fixture。
- rolling upgrade 中新旧进程并存。
- 请求与响应方向是否对称。
- 数据会不会被旧版本读后再写，导致新字段丢失。
- downgrade 是否支持。

“新版本能读旧数据”只是 backward compatibility；“旧版本能读新数据”是 forward compatibility，两者不要混称。

## 测试不能只做 round-trip

Round-trip `T -> JSON -> T` 可能让 writer 和 reader 一起犯同一个错。协议测试至少包括：

1. **Exact output**：断言 key/tag/省略行为的 JSON shape。
2. **Known input**：从手写 JSON/历史 fixture 读取。
3. **Round-trip**：确保信息在预期范围内保留。
4. **Invalid input**：both/none、未知 tag、错误类型、越界值应拒绝。
5. **Compatibility**：旧 alias/缺失新字段仍按政策工作。
6. **No leak**：敏感/source 字段不出现在 wire。

仓库 [`notification_serde.rs`](../../crates/common/xai-tool-runtime/tests/notification_serde.rs) 同时 round-trip 并断言 `type` tag 与关键字段；integration target [`jsonrpc_envelope.rs`](../../crates/common/xai-tool-protocol/tests/jsonrpc_envelope.rs) 覆盖 JSON-RPC exact shape、合法 round-trip 与 both/neither 拒绝行为。

## 修改前检查表

1. 类型是否跨 crate、进程、版本或长期落盘？
2. canonical wire shape 是什么，有无规范/fixture？
3. missing、null、空值、未知字段、未知 variant 分别如何处理？
4. 新旧 writer/reader 四格是否都分析？
5. 默认值是兼容决策还是碰巧来自 `Default`？
6. custom deserializer 是否接受有限明确的 shape？
7. schema、文档和 Serde 是否同步？
8. 错误和 Debug 是否可能泄露敏感输入？
9. 测试是否包含 exact shape、旧输入和 invalid input？

## 动手练习

1. 阅读 [`JsonRpcResponse`](../../crates/common/xai-tool-protocol/src/envelope.rs)，手写 four-case 表：result only、error only、both、neither。
2. 阅读 [`TemplateOverride`](../../crates/codegen/xai-grok-agent/src/prompt/context.rs)，列出新格式与 legacy raw string 怎样映射。
3. 阅读 [`serde_lenient.rs`](../../crates/common/xai-tool-types/src/serde_lenient.rs)，为一个未覆盖的拒绝输入先预测结果，再确认现有测试。
4. 运行协议相关测试：

```sh
cargo test -p xai-tool-runtime --test notification_serde
cargo test -p xai-tool-protocol --test jsonrpc_envelope
```

5. 选择一个带 `#[serde(default)]` 的安全/权限字段，回答缺失为何采用该默认，而不是仅说明 Rust 默认值是什么。

完成标准：修改一个 Serde 类型前，能画出 wire shape 和新旧兼容矩阵，并写出至少一个 round-trip 无法发现的负向测试。
