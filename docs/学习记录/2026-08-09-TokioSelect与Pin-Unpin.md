# Tokio `select!`、Future 与 `Pin/Unpin` 学习记录

- 日期：2026-08-09
- 主题：`tokio::select!`、Future 的 `poll/Waker` 机制、`Pin/Unpin`、`Sleep` 定时器
- 相关代码：`crates/codegen/xai-grok-shell/src/session/acp_session_impl/run_loop.rs:298-340`

## 1. 项目代码背景

相关代码可以简化为：

```rust
let idle_flush_sleep = match session.idle_flush_timeout {
    Some(timeout) => tokio::time::sleep(timeout),
    None => tokio::time::sleep(std::time::Duration::MAX),
};
tokio::pin!(idle_flush_sleep);

loop {
    tokio::select! {
        biased;

        _ = &mut idle_flush_sleep, if session.idle_flush_timeout.is_some()
            && session.memory.is_enabled()
            && !session.memory.is_flushing.load(Ordering::Relaxed) => {
            // 执行 idle flush

            if let Some(timeout) = session.idle_flush_timeout {
                idle_flush_sleep
                    .as_mut()
                    .reset(tokio::time::Instant::now() + timeout);
            }
        }

        // 其他异步事件分支
    }
}
```

这段代码创建一个可复用的 Tokio 定时器，将其固定，然后在会话事件循环中反复等待和重置。

## 2. `tokio::select!` 是宏 DSL，不是普通 Rust 语法

分支：

```rust
_ = &mut idle_flush_sleep, if condition => {
    // 处理逻辑
}
```

遵循 `tokio::select!` 定义的格式：

```text
模式 = Future 表达式, if 可选前置条件 => 分支处理代码
```

各部分含义：

- `_`：模式匹配，忽略 Future 的输出；`Sleep` 的输出为 `()`。
- `&mut idle_flush_sleep`：可变借用并等待同一个已固定的定时器，而不是把它移入本轮 `select!`。
- `if condition`：分支前置条件；为 `false` 时该分支在本轮被禁用，不会被 poll。
- `=> { ... }`：Future 返回 `Ready` 且模式匹配后执行的处理逻辑。

`_`、`&mut`、`if` 分别是 Rust 元素，但这种整体组合是 `tokio::select!` 宏定义的 DSL，不能脱离该宏单独使用。

### 2.1 `select!` 本身不是循环

一次 `select!` 只选择并执行一个就绪分支，随后结束。项目代码能够持续处理事件，是因为外面有 Rust 的 `loop`：

```rust
loop {
    tokio::select! {
        // 每轮选择一个事件
    }
}
```

### 2.2 `biased;` 的含义

默认情况下，Tokio 会引入一定随机性选择轮询起点，以降低靠前分支总是获胜的风险。

使用：

```rust
biased;
```

后，分支严格按书写顺序检查；多个分支同时就绪时，靠前分支优先。此时公平性由开发者负责：若前面的高频分支持续就绪，后面的分支可能饥饿。

### 2.3 未获胜分支与取消安全

某个分支获胜后，本次 `select!` 结束。直接在宏内创建的其他未完成 Future 通常会被丢弃，因此需要关注 cancellation safety（取消安全性）。

Pin 只保证值不被移动，并不保证 Future 被丢弃后可以无损重建。需要分别判断：

- Future 是否需要跨轮保留进度；
- 未完成时被 drop 是否丢失队列位置、部分输入或中间状态；
- Future 完成后能否再次 poll；
- 是否需要重建、替换或记录完成状态。

## 3. `select!` 为什么不会空转

Future 的核心返回类型是：

```rust
enum Poll<T> {
    Ready(T),
    Pending,
}
```

`select!` 会 poll 所有启用分支：

1. 某个 Future 返回 `Ready(value)`：执行对应分支。
2. 所有 Future 都返回 `Pending`：当前 Tokio task 也挂起，不再占用 CPU。
3. Future 在返回 `Pending` 前会登记当前 task 的 `Waker`。
4. 定时器到期、channel 收到消息或 socket 就绪时，底层资源调用 `wake()`。
5. Tokio 把 task 放回可运行队列，再次 poll。

流程可概括为：

```text
poll select!
  ├─ poll Sleep   → Pending，并登记 Waker
  ├─ poll channel → Pending，并登记 Waker
  └─ 全部 Pending
          ↓
      task 挂起，不占 CPU
          ↓
   某个资源就绪并 wake
          ↓
   task 被再次调度和 poll
```

### 3.1 哪些情况会空转

如果某个分支每轮都立即返回 `Ready`，外层 `loop` 就可能高速循环。例如：

```rust
loop {
    tokio::select! {
        _ = async {} => {}
    }
}
```

关闭的 channel 也可能持续立即返回 `None`，应在关闭时退出或禁用分支：

```rust
value = rx.recv() => {
    match value {
        Some(value) => handle(value),
        None => break,
    }
}
```

`Sleep` 到期后也是已完成 Future；若在外层循环中复用却不 `reset`，下一轮可能继续立即 `Ready`。当前项目在分支末尾设置新的 deadline，所以不会由该定时器造成空转。

## 4. Future、`poll` 与状态机

`Future` trait 的核心方法大致是：

```rust
trait Future {
    type Output;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output>;
}
```

编译器会把 `async fn` 转换为状态机。每次 `poll` 都可能修改状态：

```text
Start → Waiting → Completed
```

- `&mut Self`：允许原地修改状态，并且只临时借用 Future，返回 `Pending` 后还可继续 poll。
- `Pin<&mut Self>`：除独占可变借用外，还承诺底层 Future 已固定，不会通过安全代码移动。
- `Context`：携带当前 task 的 Waker。

## 5. 什么是 Rust 的“移动”

普通赋值可能转移所有权：

```rust
let sleep1 = tokio::time::sleep(duration);
let sleep2 = sleep1;
```

`Sleep` 没有实现 `Copy`，所以这不是生成两个定时器，而是将同一个值的所有权从 `sleep1` 移给 `sleep2`：

- `sleep1` 变成已移动状态，不能再使用；
- `sleep2` 成为该值的新所有者；
- 最终只析构一次；
- 语言语义允许该值的内存地址发生变化，尽管编译器优化后不一定真的复制字节。

在 Pin 之前可以多次移动：

```rust
let sleep1 = tokio::time::sleep(duration);
let sleep2 = sleep1;
let sleep3 = sleep2;
tokio::pin!(sleep3);
```

Pin 的地址稳定承诺从最后固定的时刻开始。

## 6. `Pin` 的通俗理解

可以把 `Pin` 理解成贴在对象上的“禁止搬家”标签：

> 对象仍可按其 API 原地修改，但不能再通过安全代码把底层值移动到另一个地址。

常见形式：

```rust
Pin<&mut T>
Pin<Box<T>>
```

Pin 不表示：

- 对象完全不可修改；
- 对象不会被正常 drop；
- 对象是 `Send` 或 `Sync`；
- 引用变为 `'static`；
- 内存被操作系统锁定；
- 对象一定存放在堆上。

Pin 只约束 Rust 语言语义上的移动，例如防止通过普通安全 API 对底层值做 `mem::replace`、`swap` 或取出所有权。

## 7. `Unpin` 与 `!Unpin`

准确关系如下：

| 状态                  | 能否移动                       |
| --------------------- | ------------------------------ |
| `T: Unpin`            | 即使处于 `Pin` 中，移动也安全  |
| `T: !Unpin`，尚未 Pin | 可以移动                       |
| `T: !Unpin`，已经 Pin | 不能再通过安全 Rust 移动底层值 |

最重要的记忆方式：

> `!Unpin` 不是“从出生起永远不能移动”，而是“一旦被 Pin，就必须保持固定”。

绝大多数普通数据类型自动实现 `Unpin`。地址敏感、自引用或主动包含 `PhantomPinned` 的类型可能是 `!Unpin`。

### 7.1 移动 Pin 指针不等于移动底层值

对于：

```rust
Pin<Box<T>>
```

移动外层 Box/Pin 变量，只会移动指针；堆上的 `T` 地址不变。类似地，移动一个指向固定对象的 Pin 句柄，不等于移动它所指向的对象。

## 8. 为什么 async Future 会需要 Pin

异步函数会被编译为状态机。局部变量和跨 `.await` 使用的引用都可能成为状态机字段，形成类似的关系：

```text
Future 状态机
┌─────────────────────┐
│ data                │ ◄──┐
│ reference_to_data   │ ───┘
│ current_state       │
└─────────────────────┘
```

如果这种状态机开始执行后被移动，内部地址关系可能失效。`Future::poll` 因此接收 `Pin<&mut Self>`，使 Future 实现者可以依赖“从第一次 poll 开始地址稳定”的契约。

严格顺序是：

```text
创建 Future
  ↓
尚未 Pin，可以移动
  ↓
放到最终位置并 Pin
  ↓
第一次 poll
  ↓
后续多次 poll，底层地址不变
  ↓
完成并在原位置 Drop
```

并不是每个 Future 都真的自引用，但统一的 `poll` 接口允许实现地址敏感的 Future。

## 9. 为什么 `Sleep` 是 `!Unpin`

`tokio::time::Sleep` 不只是保存一个 `Instant`。它还维护与 Tokio 定时器驱动相关的内部状态，例如 deadline、唤醒状态和定时器条目。

`Sleep` 被设计为 `!Unpin`，意味着 Tokio 可以在其被 poll 后依赖固定位置实现内部状态管理。具体内部结构可能随 Tokio 版本变化，不应武断理解为“驱动一定直接保存 Sleep 的裸指针”；稳定的公开契约是：开始 poll 后必须保持固定。

这种设计还避免为了让外层对象随意移动而强制每个定时器都做独立堆分配。调用者可以选择：

- 局部使用 `pin!`，通常无额外堆分配；
- 需要拥有、存储或跨调用传递时使用 `Box::pin`。

## 10. `tokio::pin!(idle_flush_sleep)` 做了什么

项目代码：

```rust
let idle_flush_sleep = tokio::time::sleep(timeout);
tokio::pin!(idle_flush_sleep);
```

概念上发生变量遮蔽：

```text
执行前：idle_flush_sleep: Sleep
执行后：idle_flush_sleep: Pin<&mut Sleep>（概念类型）
```

它将 `Sleep` 固定在当前作用域/async 状态机中的位置，然后提供 pinned 引用，通常不发生堆分配。

如果删掉 `tokio::pin!`，再将 `&mut Sleep` 交给 `select!`，通常会因 `Sleep: !Unpin` 编译失败，错误类似：

```text
the trait `Unpin` is not implemented for `Sleep`
PhantomPinned cannot be unpinned
```

但以下场景通常不需要用户显式 Pin：

```rust
tokio::time::sleep(duration).await;
```

以及：

```rust
tokio::select! {
    _ = tokio::time::sleep(duration) => {}
}
```

此时编译器或宏负责在 poll 前固定 Future。当前项目需要手写 `pin!`，是因为同一个 `Sleep` 要在 `select!` 外保存、跨多轮复用并调用 `reset`。

## 11. 栈 Pin 和堆 Pin 为什么都可以

Pin 关心地址稳定，不关心对象位于栈还是堆。

### 11.1 局部/栈 Pin

```rust
let future = create_future();
tokio::pin!(future);
```

Future 固定在当前作用域或 async 状态机内，通过借用规则保证其有效期内不被安全代码移走。适合局部等待和复用，无需额外堆分配，但 pinned 引用不能逃出所有者生命周期。

### 11.2 堆 Pin

```rust
let mut future = Box::pin(create_future());
```

类型为：

```rust
Pin<Box<T>>
```

外层 Box 指针可以移动，但堆上的 `T` 地址不变。适合：

- 将 Future 存入结构体；
- 异构 Future 类型擦除；
- 跨调用长期保存；
- 需要所有权而非短期局部借用。

两种方式最终都可通过 `as_mut()` 向 `poll` 提供：

```rust
Pin<&mut T>
```

因此 `poll` 无需知道对象具体位于哪里，只要求独占可变访问和地址稳定。

## 12. `as_mut()` 与 `get_mut()`

### 12.1 `as_mut()`：保留 Pin

概念签名：

```rust
fn as_mut(&mut self) -> Pin<&mut T>
```

它对 pinned pointer 做临时重新借用：

```text
Pin<Box<T>> 或 Pin<&mut T>
          ↓ as_mut()
      Pin<&mut T>
```

特点：

- 仍保留 Pin 约束；
- 不要求 `T: Unpin`；
- 不移动底层值；
- 生成较短生命周期的 pinned 重借用，调用后原 pinned owner 还能继续使用。

项目中的：

```rust
idle_flush_sleep.as_mut().reset(deadline);
```

就是把一个临时 `Pin<&mut Sleep>` 交给 `reset`。

### 12.2 `get_mut()`：安全去掉 Pin 包装

概念签名：

```rust
fn get_mut(self: Pin<&mut T>) -> &mut T
where
    T: Unpin;
```

它返回普通 `&mut T`。由于普通可变引用能用于替换和移动整个对象，安全版本只允许 `T: Unpin`。

`Sleep: !Unpin`，所以不能安全地对它调用 `get_mut()` 获取 `&mut Sleep`。

### 12.3 `get_unchecked_mut()`

unsafe 版本不要求 `T: Unpin`：

```rust
unsafe fn get_unchecked_mut(self: Pin<&mut T>) -> &mut T
```

调用者必须保证绝不通过该引用移动底层值，也不破坏任何固定字段的不变量。它通常用于底层类型实现和 Pin 投影，不应只为绕过编译错误而使用。

| 方法                  | 返回值        | 保留 Pin | 条件                          |
| --------------------- | ------------- | -------- | ----------------------------- |
| `as_mut()`            | `Pin<&mut T>` | 是       | 不要求 `T: Unpin`             |
| `get_mut()`           | `&mut T`      | 否       | 必须 `T: Unpin`               |
| `get_unchecked_mut()` | `&mut T`      | 否       | `unsafe`，调用者维护 Pin 契约 |

## 13. 接受 Pin 的安全方法是什么意思

具体类型可以定义：

```rust
fn reset(self: Pin<&mut Self>, deadline: Instant)
```

这表示调用者必须先固定对象；方法可以原地修改内部状态，但类型实现者保证不会移动整个对象或破坏固定字段。

“安全方法”指调用方不需要写 `unsafe`：

```rust
sleep.as_mut().reset(deadline);
```

方法内部可能使用经过证明的 unsafe 或 Pin projection，但风险被封装在类型实现中。

Pin 不禁止修改，只禁止移动。例如：

```text
同一地址上的 Sleep { deadline: 10:00 }
                     ↓ reset
同一地址上的 Sleep { deadline: 11:00 }
```

地址没有变化，所以不破坏 Pin。

`Future::poll(self: Pin<&mut Self>, ...)` 自身也是 Pin-aware 方法：它可推进状态机，但不能移动整个 Future。

### 13.1 Pin projection

复杂类型可能需要把：

```rust
Pin<&mut Outer>
```

投影成：

```text
普通、可移动字段：&mut Field
必须固定的字段：Pin<&mut PinnedField>
```

字段是否可以从外层 Pin 中移动，必须由类型设计决定。手写投影容易出错，底层代码通常使用经过验证的实现或 `pin-project` 一类工具。

## 14. 在 Pin 后用 unsafe 强制移动会怎样

如果对已经固定的 `!Unpin` 值使用 `get_unchecked_mut`、`ptr::read` 等手段强制移动，就破坏了 Pin 契约，可能导致未定义行为。

可能表现为：

- 暂时看起来完全正常；
- Future 不再唤醒或错误唤醒；
- 定时器 `reset` 状态异常；
- 悬空指针、use-after-free、double free；
- 只在特定优化级别或执行顺序下崩溃。

“目前能运行”不能证明违反 Pin 契约是安全的。

尤其是：

```rust
let moved = unsafe { std::ptr::read(reference) };
```

`ptr::read` 按位读取，不会自动把原位置标记为普通 Rust 语义上的已移动。若原位置和新值都被析构，还可能产生重复所有权和双重析构问题。即使使用 `ManuallyDrop` 避免重复析构，移动已固定的 `!Unpin` 值本身仍违反 Pin 契约。

Pin 后仍可合法地：

- 移动 Pin 指针或 Box 指针，前提是底层值不动；
- 通过 `Pin<&mut Self>` 的公开 API 原地修改；
- 生命周期结束时在原位置执行 Drop。

## 15. 什么时候主动控制 `Unpin`

普通业务代码通常不需要改变类型的 `Unpin` 属性。遇到 `!Unpin` Future 时，优先使用 `pin!` 或 `Box::pin`，而不是强行实现 `Unpin`。

### 15.1 主动让类型成为 `!Unpin`

常见于类型作者实现：

- 自引用结构；
- 地址敏感的 Future/Stream；
- 侵入式链表节点；
- 外部系统长期保存对象地址的 FFI 结构；
- 其他必须原地存在的底层状态。

常用 `PhantomPinned` 阻止自动实现 `Unpin`：

```rust
struct AddressSensitive {
    data: String,
    pointer: *const String,
    _pin: std::marker::PhantomPinned,
}
```

仅仅传给 FFI 不一定需要 `!Unpin`；关键在于外部是否会跨调用长期保存该地址。

### 15.2 主动实现 `Unpin`

只有能证明移动不会使任何内部引用、指针或状态失效时，才考虑：

```rust
impl Unpin for MyType {}
```

`Unpin` 虽然不是 unsafe trait，但错误实现可能使依赖 Pin 的 unsafe 代码失去前提，间接造成未定义行为。不能仅为消除编译错误而实现它。

泛型类型有时会条件实现：

```rust
impl<T: Unpin> Unpin for Wrapper<T> {}
```

不过很多普通结构体由编译器自动推导，不需要手写。

### 15.3 外层可移动、内层保持固定

常见封装：

```rust
struct Holder<T> {
    inner: Pin<Box<T>>,
}
```

即使 `T: !Unpin`，移动 `Holder<T>` 也只移动 Box 指针，堆中的 `T` 不动。因此外层容器仍可能是 `Unpin`。这不是把 `T` 变成 `Unpin`，而是让外层移动不影响内层固定值。

## 16. 当前代码的完整心智模型

```text
创建 Sleep
  │ 尚未 Pin，可以移动
  ▼
tokio::pin!(idle_flush_sleep)
  │ 固定在当前 async 状态机/作用域
  ▼
loop + tokio::select!
  │
  ├─ 条件为 false：该分支本轮不 poll
  │
  ├─ 条件为 true且未到期：Sleep 返回 Pending，登记 Waker
  │                        所有分支 Pending 后 task 挂起
  │
  └─ 定时器到期：Waker 唤醒 task
                   Sleep 返回 Ready(())
                   执行 idle flush 分支
                   as_mut().reset(新 deadline)
                   回到下一轮 loop
```

关键点：

1. `select!` 是一次选择，不是循环；持续运行来自外层 `loop`。
2. 所有分支 `Pending` 时 task 挂起，因此不会轮询空转。
3. `Sleep` 在循环外创建，所以其他分支获胜时仍保留同一个定时器及其进度。
4. `Sleep: !Unpin`，通过 `tokio::pin!` 在第一次 poll 前固定。
5. `&mut idle_flush_sleep` 让 `select!` 借用同一个 Future，而不是取得所有权。
6. `as_mut()` 保持 Pin 并创建临时重借用。
7. `reset()` 原地修改 deadline，不移动 `Sleep`。
8. reset 后下一轮重新返回 `Pending`，避免已完成定时器导致外层循环空转。
9. Pin 与取消安全、线程安全、生命周期是不同问题，不能混为一谈。

## 17. 最简记忆口诀

```text
select!：多个 Future 等一次，谁先 Ready 执行谁。
loop：让 select! 一轮又一轮继续等待。
Pending + Waker：没事件就挂起，有事件再唤醒，不空转。
Pin：可以改，不能搬。
Unpin：搬了也安全。
!Unpin：Pin 前可搬，Pin 后底层值不能搬。
as_mut：仍然 Pin，只做重借用。
get_mut：去掉 Pin，只允许 Unpin 类型。
Sleep：循环外保存、poll 前 Pin、触发后 reset。
```
