# 23. 编译器错误诊断地图

Rust 编译器不是在要求你套用某个固定语法，它是在指出当前程序违反了类型、所有权或并发契约。真正的目标不是“让红线消失”，而是先说清契约冲突，再选择符合业务语义的修复。

本章按 `grok-build` 中最常见的阅读障碍组织。错误码只是入口；同一个 E0277 可以表示完全不同的 trait bound 问题。

## 先学会读一条诊断

典型诊断包含：

```text
error[E0382]: borrow of moved value: `command`  <- 错误类别和最终冲突点
  --> file.rs:10:24                           <- 编译器无法接受的使用
   |
8  | consume(command);                        <- 值在这里 move
   |         ------- value moved here
9  | println!("{command}");
   |            ^^^^^^^ value borrowed here after move
note: consider changing this parameter ...    <- 可能的方向，不是业务结论
```

固定顺序：

1. 读第一条 error，不先修后续级联错误。
2. 找诊断标出的“首次 move/borrow/type choice”，不只看最后爆红行。
3. 写出编译器认为的实际类型和你期望的类型。
4. 判断冲突属于 API 契约、作用域、数据布局还是调度边界。
5. 选择修复后，说明它是否增加 clone、锁、堆分配或行为变化。
6. 重新检查；不要一次同时猜改十处。

辅助命令：

```sh
# 只检查目标 package，短格式适合先看错误集合
cargo check -p xai-tool-runtime --message-format short

# 查看错误码的独立解释
rustc --explain E0382

# 避免输出被 warning 淹没；不要永久隐藏 warning
cargo check -p <package> 2>&1 | sed -n '1,160p'
```

使用最后一条管线时要查看真实命令退出状态；交互 shell 的 pipeline 默认可能只报告末端命令状态。

## 快速索引

| 错误/症状 | 首先怀疑 | 常见正确方向 |
| --- | --- | --- |
| E0382 moved value | 按值传参、`into_iter`、`async move` | 改借用契约、调整所有权边界、必要时 clone |
| E0507 cannot move out of borrowed content | 从 `&T` 模式中取走非 Copy 字段 | 借用字段、clone 明确成本、让调用方交出 T |
| E0502 mutable/immutable borrow conflict | 共享借用跨过 mutation | 缩短借用，先算后改，重排代码 |
| E0499 multiple mutable borrows | 同一容器取两个 `&mut` | `split_at_mut`、entry API、拆字段 |
| E0597 does not live long enough | 被引用所有者先 drop | 延长所有者作用域或返回 owned value |
| E0515 return reference to local | 返回局部值内部引用 | 返回 `String`/`PathBuf` 等 owned value |
| E0716 temporary dropped while borrowed | 对临时值立即 `.as_str()`/`.as_ref()` | 先绑定所有者，再借用 |
| E0308 mismatched types | 分支、嵌套 Result/Option、引用层级 | 从最外层逐层比较实际/期望类型 |
| E0282/E0283 inference | `collect`、`parse`、`into` 目标不明确 | 在语义边界标注目标类型 |
| E0277 trait bound | 缺 trait、`Send`/`Sync`、序列化 bound | 找 bound 从哪个调用边界引入 |
| E0599 method not found | trait 未导入、接收者类型错误、cfg | 查方法定义和 trait bound，不只查拼写 |
| E0038 dyn compatibility | trait 方法无法进入 vtable | 泛型静态分发或建立 object-safe adapter |
| future is not Send | `!Send` 值/guard 跨 `.await` | 缩短存活区间、改调度模型或共享类型 |
| E0733 recursive async fn | Future 大小递归 | 装箱递归边，或改成显式循环 |

## E0382：值已经 move

```rust
let command = String::from("check");
send(command);          // command 所有权进入 send
println!("{command}"); // E0382
```

先问 `send` 是否应拥有值：

- 若它只读取，签名应接受 `&str` 或 `&String`。
- 若它把值存入 actor command，move 往往正是正确语义；发送后不应继续使用。
- 若发送方与接收方都确实需要独立 owned value，clone 是显式成本。
- 若值是 `Arc` 或 channel sender，clone 通常只复制共享句柄；仍要确认关闭语义。

仓库中 `mpsc::Sender::send(command)` 按值转移消息，这是 Actor 独占命令数据的基础。不要为了保留发送方访问而把 command 内所有字段改成引用；异步队列通常比调用栈活得久。

练习：[`moved_value.rs`](./labs/compile_fail/moved_value.rs)。

## E0507：不能从借用中 move 字段

```rust
fn name(event: &Event) -> String {
    match event {
        Event::Started { prompt_id } => *prompt_id, // 尝试从 &Event 取走 String
    }
}
```

匹配 `&Event` 时字段绑定通常也是引用。选择取决于 API：

```rust
prompt_id.as_str() // 返回借用视图
prompt_id.clone()  // 返回独立所有权，有分配成本
```

或者把函数改成接收 `Event`，明确消费调用者的值。错误本质是调用契约不明确，不是少了一个星号。

## E0502：共享借用与可变借用重叠

```rust
let first = &tools[0];
tools.push(new_tool);       // Vec 可能重分配，旧引用会失效
println!("{first}");
```

常见修复：

```rust
let first_name = tools[0].name.clone(); // 真正需要 owned snapshot 时
tools.push(new_tool);
println!("{first_name}");
```

或先完成对 `first` 的最后一次使用，让 non-lexical lifetimes 缩短借用，再 mutation。对 map 可考虑 `entry` API，避免先 `get` 后 `insert` 形成重叠借用。

练习：[`borrow_conflict.rs`](./labs/compile_fail/borrow_conflict.rs)。

## E0499：同时存在多个可变借用

```rust
let first = &mut values[0];
let second = &mut values[1]; // 编译器无法仅凭索引证明不同
```

切分 API 能把“不重叠”编码进类型：

```rust
let (left, right) = values.split_at_mut(1);
let first = &mut left[0];
let second = &mut right[0];
```

若 struct 的两个字段明显不同，编译器通常能分别借用；若所有状态塞在一个容器中，频繁冲突可能说明数据布局需要拆分。

练习：[`multiple_mut_borrow.rs`](./labs/compile_fail/multiple_mut_borrow.rs)。

## E0597 / E0515 / E0716：所有者活得不够久

三者都从 owner/reference 关系入手：引用绝不能比所有者更长寿。

```rust
fn bad() -> &str {
    let text = String::from("local");
    &text // E0515：函数返回时 text drop
}
```

通常应返回 `String`，把所有权交给调用者。若字符串来自输入参数，才可能用生命周期把返回引用绑定到输入。

```rust
let view = String::from("temporary").as_str(); // E0716
```

修复为：

```rust
let owner = String::from("temporary");
let view = owner.as_str();
```

不要用 `Box::leak` 或伪造 `'static` 作为普通修复；那会改变内存生命周期，而不是表达本来就存在的长期所有者。

练习：[`return_local_reference.rs`](./labs/compile_fail/return_local_reference.rs) 与 [`temporary_dropped.rs`](./labs/compile_fail/temporary_dropped.rs)。

## E0308：类型不匹配

对复杂类型从外向内比较：

```text
expected Result<Option<T>, E>
   found Option<Result<T, E>>
```

这不是 T 的问题，最外层容器就不同；可能需要 `transpose()`。

```text
expected &Path
   found PathBuf
```

调用边界需要借用，可传 `path.as_path()` 或 `&path`，不应为了匹配而复制。

异步代码常见三层：

```rust
timeout(duration, handle).await
// Result<Result<Result<T, E>, JoinError>, Elapsed>（形状依实际组合而定）
```

先为每层命名：超时、task join、业务操作。不要用连续 `unwrap()` 抹平来源。

## E0282 / E0283：类型推断不够

高频触发点：

```rust
let values = iter.collect();
let port = "8080".parse()?;
let path = raw.into();
```

给最终语义位置标类型：

```rust
let values: Vec<_> = iter.collect();
let port: u16 = "8080".parse()?;
let path: PathBuf = raw.into();
```

不要在链中每一步都写冗余类型；在 API 边界、集合目标和转换终点提供一次信息通常足够。

## E0277：某个 trait bound 不满足

完整诊断通常会告诉你 bound 从哪一层引入：

```text
required because it appears within the type ...
required by a bound in tokio::spawn
```

按调用链反向读：`tokio::spawn` 要 Future: Send；Future 捕获了 `Rc<State>`；`Rc<State>` 不实现 Send。因此错误不是“State 缺 derive Send”，而是调度模型与共享类型不相容。

选择：

- 状态只应在 LocalSet：使用 `spawn_local` 并保持 `Rc/RefCell`。
- 状态确需跨线程只读共享：可能使用 `Arc<T>`。
- 状态确需跨线程修改：评估 actor/channel、`Arc<Mutex<T>>`、并发 map 或原子类型。
- 根本无需 spawn：在当前 task 直接 `.await`。

练习：[`rc_is_not_send.rs`](./labs/compile_fail/rc_is_not_send.rs)。

## future cannot be sent between threads safely

这种诊断常指出某个值跨 `.await` 存活：

```rust
let guard = state.lock().unwrap();
network_call().await;
use_guard(guard);
```

即使 guard 的最后“业务用途”看似在 await 前，词法作用域或显式 drop 位置也可能让它仍成为 Future 状态机字段。优先创建短作用域：

```rust
let snapshot = {
    let guard = state.lock().unwrap();
    guard.clone_needed_fields()
};
network_call(snapshot).await;
```

这同时避免长时间持锁。换成 Tokio Mutex 只能让“等待锁”异步化，并不会自动证明跨网络等待持锁是好设计。

## E0599：找不到方法

按顺序检查：

1. 接收者实际类型是不是你以为的类型，是否多了一层 `Option`/`Result`/引用？
2. 方法来自 trait 吗？trait 是否在作用域中？例如 `StreamExt::next`。
3. 泛型参数是否满足提供该方法的 bound？
4. 方法是否被 `cfg(feature/target/test)` 排除？
5. 是否发生名字遮蔽或使用了不同版本 crate 的同名类型？

用 `rg -n 'fn method_name\b|trait .*'` 找定义，再看 `impl` 条件。不要直接假设依赖版本错了。

## E0038：trait 不能形成 trait object

编译器通常指出具体不兼容方法：泛型方法、返回 `Self`、不合适的接收者或其他无法进入 vtable 的签名。解决方向：

- 调用者知道具体类型：用泛型/`impl Trait`。
- 只有少数方法需要动态调用：为其他方法加 `where Self: Sized`。
- 确需动态边界：建立 object-safe adapter，擦除关联类型或泛型参数。

本仓库 [`Tool`](../../crates/common/xai-tool-runtime/src/tool.rs) 与 `ToolDyn` 正是第三种设计；详见 [03 trait](./03-traits-generics-and-dyn.md)。

## E0733：递归 async fn

`async fn` 返回匿名状态机，直接递归会让 Future 类型包含自身而大小无限：

```rust
async fn walk(node: Node) {
    walk(node.child).await;
}
```

可在递归边装箱，使大小固定；也可改为显式 Vec/队列循环，通常更容易加入深度限制、取消和并发上限。仓库处理目录树、任务图或 agent 层级时，限制深度往往也是业务安全要求。

## 不要机械采用的“万能修复”

| 机械动作 | 隐藏的代价/风险 |
| --- | --- |
| 到处 `.clone()` | 分配、复制大对象、延长 sender/Arc 生命周期 |
| 到处 `Arc<Mutex<_>>` | 改变线程模型，引入争用、死锁与跨 await 持锁风险 |
| 全部改成 `'static` | 往往不可能，或通过泄漏掩盖错误所有权 |
| 全部 `.unwrap()` | 把可恢复失败变成进程/task panic |
| 把错误 `.ok()` | 丢失解析、I/O、权限等真实失败 |
| 加 `move` | 会改变捕获所有权，不保证 Future 因此 Send |
| 使用 `unsafe` | 借用检查不变量变成程序员必须人工证明的责任 |

编译器 suggestion 是语法上可能的局部改法，不知道产品不变量。先判断 API 是否应拥有、借用、共享或复制。

## 仓库排障工作表

每次遇到难错误，记录：

```text
第一条错误码/症状：
最终冲突行：
首次 move/borrow/bound 来源：
实际类型：
期望类型：
跨越的边界：函数 / task / channel / lock / wire
候选修复 A 及语义成本：
候选修复 B 及语义成本：
选择与验证：
```

先在 [Rust 阅读 Katas](./labs/README.md) 运行全部 compile-fail 练习，再从仓库历史或当前编译输出选一条真实诊断填写工作表。能解释错误为何发生、修复改变了什么，才算掌握；仅得到绿色编译不算。
