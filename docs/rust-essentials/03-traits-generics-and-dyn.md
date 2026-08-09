# 3. trait、泛型与动态分发

trait 是行为契约，也是 Rust 组织依赖方向的核心工具。先读 trait 的接收者、关联类型、默认方法和 bounds，再读实现；不要从某个具体实现反推整个接口意图。

本章完成后，你应能解释静态分发与动态分发的代价，判断一个 trait 能否写成 `dyn Trait`，读懂 `Tool -> ToolDyn -> ToolDispatch` 的类型擦除路径，并区分 `Send`、`Sync`、`'static` 分别约束什么。

## trait 的组成

```rust
trait Store: Send + Sync {
    type Error;

    fn get(&self, key: &str) -> Result<Option<String>, Self::Error>;

    fn contains(&self, key: &str) -> Result<bool, Self::Error> {
        Ok(self.get(key)?.is_some())
    }
}
```

- `Store: Send + Sync` 是 supertrait：实现 Store 的类型也必须满足它们。
- `type Error` 是关联类型，由每个实现选择。
- `&self` 表示调用不取得 Store 所有权，也不要求独占可变访问。
- `contains` 有默认实现，具体类型可覆盖。
- trait 定义契约；`impl Store for MyStore` 才提供行为。

阅读接收者时先判断所有权：

| 接收者 | 常见语义 |
| --- | --- |
| `self` | 消费当前值，例如 builder 的 `build(self)` |
| `&self` | 共享读取或通过内部可变性修改 |
| `&mut self` | 当前任务独占地修改值 |
| `self: Arc<Self>` | 方法取得一份共享所有权，适合启动长期任务 |

## 泛型参数与关联类型

两种设计表达的量词不同：

```rust
trait Decode<T> {
    fn decode(&self) -> T;
}

trait DecodeAssociated {
    type Output;
    fn decode(&self) -> Self::Output;
}
```

- `Decode<T>`：同一实现类型可以针对多个 T 分别实现，调用点有时需要指定 T。
- 关联类型：一个具体 impl 固定一个 Output，调用点写法更简洁。

仓库的 [`Tool`](../../crates/common/xai-tool-runtime/src/tool.rs) 使用关联类型表达“一种工具固定对应一种 Args 和 Output”：

```rust
pub trait Tool: Send + Sync {
    type Args: for<'de> Deserialize<'de> + JsonSchema + Send + 'static;
    type Output: Serialize + ToolOutput + Send + 'static;

    fn id(&self) -> ToolId;
    fn run(
        &self,
        ctx: ToolCallContext,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, ToolError>> + Send;
}
```

读 `Self::Args` 时，把 `Self` 替换成当前实现类型。若 `ReadFileTool: Tool<Args = ReadArgs, Output = ReadOutput>`，方法签名就可以具体化后再理解。

## bounds 与 where 从句

短约束可写在参数旁：

```rust
fn render<T: Display + Send>(value: T) -> String
```

复杂约束通常放在 `where`：

```rust
fn render<T>(value: T) -> String
where
    T: Display + Send,
```

二者大多语义相同。`where` 还能约束关联类型和引用：

```rust
where
    T::Output: Serialize,
    for<'a> &'a T: IntoIterator,
```

不要把所有 bound 一次记住。逐个标注它服务的边界：序列化、线程转移、共享访问、格式化、克隆还是生命周期。

## 静态分发

```rust
fn load<S: Store>(store: &S, key: &str) -> Result<Option<String>, S::Error> {
    store.get(key)
}

fn load_short(store: &impl Store<Error = MyError>, key: &str) {
    // 参数位置 impl Trait 是匿名泛型参数。
}
```

编译器为实际类型检查和生成调用，通常能内联；代价是每种实例化可能增加编译时间和二进制体积。静态分发允许使用 trait 的完整能力，包括关联类型、泛型方法和返回 `impl Trait` 的方法。

“零成本抽象”不表示完全没有成本，而是通常能编译成与手写具体类型相当的运行时代码。仍应通过 profile 判断热路径。

## 动态分发与 trait object

```rust
fn load_dyn(store: &dyn Store<Error = MyError>, key: &str) {
    // 通过 vtable 调用实际实现。
}

let shared: Arc<dyn Store<Error = MyError>> = Arc::new(ProductionStore::new());
```

`dyn Trait` 是 dynamically sized type，大小在编译期未知，因此通常放在 `&`、`Box` 或 `Arc` 后：

| 容器 | 所有权语义 |
| --- | --- |
| `&dyn Trait` | 临时借用一个实现 |
| `&mut dyn Trait` | 临时独占借用 |
| `Box<dyn Trait>` | 单一所有者，堆分配 |
| `Arc<dyn Trait + Send + Sync>` | 多所有者，可跨线程共享 |

动态分发适用于运行时选择后端、异构集合、插件和测试替身。若调用点总知道具体类型，泛型通常更直接。

## dyn compatibility（object safety）

不是所有 trait 都能形成 `dyn Trait`。判断时重点看：

- 方法能否通过一个只有 data pointer + vtable 的值调用？
- 方法是否返回裸 `Self` 或在参数中使用无法擦除的 `Self`？
- 方法自身是否有泛型参数？vtable 无法提前列出无限实例。
- 接收者是否是允许的 `self` 形式？
- 关联类型是否已在 trait object 路径中指定？

只给 `Self: Sized` 可用的方法不会进入 trait object vtable，因此可以保留在 trait 中：

```rust
trait Factory {
    fn name(&self) -> &str;

    fn create() -> Self
    where
        Self: Sized;
}
```

`Tool` 的方法返回依赖 `Self::Output` 的原生 `impl Future`，项目明确不直接消费 `dyn Tool`。它提供 object-safe 的 [`ToolDyn`](../../crates/common/xai-tool-runtime/src/tool.rs) 适配层。

## 类型擦除不是“丢掉所有类型”

本仓库工具运行时的边界可画成：

```text
具体 Tool 实现
  Args / Output 为具体 Rust 类型
        |
        | blanket impl<T: Tool> ToolDyn for T
        v
dyn ToolDyn
  JSON Value -> 反序列化 T::Args
  T::Output  -> 序列化 Value + model output
        |
        v
dyn ToolDispatch
  只暴露 tool_id、JSON args、context 与统一 stream
```

[`ToolDyn`](../../crates/common/xai-tool-runtime/src/tool.rs) 的 blanket impl 在动态边界恢复具体类型：

```rust
#[async_trait]
impl<T: Tool> ToolDyn for T {
    async fn execute(&self, ctx: ToolCallContext, args: Value)
        -> ToolStream<TypedToolOutput>
    {
        let typed_args: T::Args = match serde_json::from_value(args) {
            Ok(value) => value,
            Err(error) => return terminal_only(Err(
                ToolError::invalid_arguments(error.to_string())
            )),
        };
        // 调用 Tool::execute 后，再把 T::Output 擦除为统一输出。
        // ……
    }
}
```

类型擦除只发生在需要运行时异构性的边界。具体工具内部仍享受强类型参数、穷尽匹配和编译期检查。

## blanket impl

blanket impl 为一整类满足约束的类型提供实现：

```rust
impl<T: Tool> ToolDyn for T { ... }
```

读作：“所有实现 Tool 的 T 自动实现 ToolDyn。”这解释了为何搜索具体类型时找不到显式 `impl ToolDyn for ReadTool`。

[`ToolOutput`](../../crates/common/xai-tool-runtime/src/render.rs) 也有：

```rust
impl<T: ToolOutput + Serialize + ?Sized> ToolOutput for Box<T> {
    fn model_output(&self) -> Vec<ContentBlock> {
        (**self).model_output()
    }
}
```

`?Sized` 放宽泛型参数默认的 `Sized` 约束，因此 T 可以是 trait object 等动态大小类型；`Box<T>` 本身仍有固定大小。

## coherence 与 orphan rule

Rust 要保证任意 trait/type 组合至多有一个实现，避免依赖组合后出现冲突。粗略规则是：实现 trait 时，trait 或目标类型至少一个必须由当前 crate 定义。

```rust
// 当前 crate 定义了 SessionId，因此可以：
impl std::fmt::Display for SessionId { ... }

// String 和 Display 都来自标准库，当前 crate 不能：
// impl std::fmt::Display for String { ... }
```

需要给外部类型实现外部 trait 时，创建本地 newtype 是常见解法。这也是 newtype 不只是“多一个名字”的原因。

## 原生 async trait 方法与 `async_trait`

现代 Rust 可在 trait 中用 `async fn`，但仓库同时存在两种模式：

```rust
fn run(&self, args: Self::Args)
    -> impl Future<Output = Result<Self::Output, ToolError>> + Send;
```

这种 return-position `impl Trait in trait`（RPITIT）保留具体 Future 类型，适合静态分发的 `Tool`。

```rust
#[async_trait::async_trait]
trait ToolDyn {
    async fn execute(&self, args: Value) -> ToolStream<TypedToolOutput>;
}
```

`async_trait` 通常把返回 Future 装箱，使动态分发接口可表达 async 方法，代价是装箱/间接调用和更隐藏的生命周期。读报错时可把 async 方法在脑中展开为“返回一个 Future 的普通方法”。

仓库另一个清晰例子是 [`AsyncFileSystem`](../../crates/codegen/xai-grok-tools/src/computer/types.rs)：生产文件系统与 [`MockFs`](../../crates/codegen/xai-grok-tools/src/computer/local/mock_fs.rs) 共用一个异步 I/O 契约。

## Send、Sync 和 'static 分别约束什么

| bound | 准确直觉 |
| --- | --- |
| `T: Send` | T 的所有权可安全转移到另一线程 |
| `T: Sync` | `&T` 可安全在线程间共享 |
| `T: 'static` | T 不含必须在更短时间失效的借用 |
| `&'static T` | 这个引用指向整个程序期间都有效的数据 |

`T: 'static` 不表示值永远不会 drop。一个拥有全部字段的 `String` 满足 `'static`，仍可在下一行被销毁。

`tokio::spawn` 通常要求 Future 为 `Send + 'static`，因为任务可能换线程且比创建它的函数活得久。`spawn_local` 放宽 `Send`，但仍需要 Future 被放入正确的 `LocalSet` 生命周期。

常见反例：

```rust
let state = Rc::new(...);
tokio::spawn(async move { use_state(state).await });
```

问题不是“async 不支持 Rc”，而是该调度边界允许跨线程；选择 `spawn_local` 还是 `Arc` 必须服从真实执行模型，而不是只为消除错误。

## 常见标准 trait 的阅读含义

| trait | 看到实现时应想到 |
| --- | --- |
| `From<T>` / `Into<T>` | 无失败的所有权转换；`?` 可借它转换错误 |
| `TryFrom<T>` / `TryInto<T>` | 可能失败的转换 |
| `AsRef<T>` | 低成本借用视图，常用于泛型参数 |
| `Borrow<T>` | 与集合查找/哈希相容的借用语义，比 AsRef 契约更强 |
| `Default` | 缺省构造，不等同于业务上总是安全的默认值 |
| `Display` | 面向用户的文本 |
| `Debug` | 面向开发诊断的结构 |
| `Iterator` | 由关联类型 Item 定义逐项产出 |
| `Future` | 被 poll 后最终产出关联类型 Output |
| `FromIterator` | 决定 `collect()` 能构造什么 |

遇到 `.into()` 推断失败，优先在接收位置写明确目标类型；遇到 `?` 类型不匹配，检查源错误到函数返回错误是否有 `From` 路径。

## 修改 trait 的影响面

给 trait 新增必需方法会破坏所有实现者；新增有默认实现的方法通常兼容实现者，但仍可能影响 object safety、命名冲突和语义。修改关联类型或 bounds 会影响所有泛型调用者。

改动前搜索：

```sh
# trait 定义、具体实现、trait object 和泛型边界
rg -n 'trait Tool\b|impl.*Tool for|dyn Tool|T: Tool|impl<T: Tool>' crates
```

然后区分：生产实现、adapter/blanket impl、测试 fake、`Arc<dyn ...>` 持有点以及 public re-export。只跑 trait 定义 crate 的测试，未必覆盖下游实现者。

## 项目阅读路径

1. 读 [`Tool`](../../crates/common/xai-tool-runtime/src/tool.rs) 的 Args/Output 和默认方法，只写契约。
2. 读同文件的 `ToolDyn` 及 blanket impl，标出 JSON 解码、强类型执行、输出擦除三个阶段。
3. 读 [`ToolDispatch`](../../crates/common/xai-tool-runtime/src/dispatch.rs)，解释它为什么能放入 `Arc<dyn ToolDispatch>`。
4. 读 [`ToolOutput`](../../crates/common/xai-tool-runtime/src/render.rs) 对 `Box<T>` 的 blanket impl，解释两次解引用。
5. 查看 [`trait_object_safety.rs`](../../crates/common/xai-tool-runtime/tests/trait_object_safety.rs) 和 [`tool_dyn.rs`](../../crates/common/xai-tool-runtime/tests/tool_dyn.rs)，确认测试依赖哪些行为不变量。

## 动手练习

运行 [Rust 阅读 Katas](./labs/README.md) 的第 7--10、13 关和 `rc_is_not_send.rs`：

```sh
docs/rust-essentials/labs/check.sh
rustc --explain E0277
```

最后闭卷画出 `Tool -> ToolDyn -> ToolDispatch`，每层写清：具体类型由谁知道、是否能形成 trait object、Args/Output 是 Rust 类型还是 JSON、Future 是否装箱。能准确画出这四项，比背诵“泛型快、dyn 灵活”更接近真正读懂本仓库。
