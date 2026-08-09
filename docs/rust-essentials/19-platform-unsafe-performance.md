# 19. 平台 API、`unsafe` 与性能证据

这是按需专题。`unsafe` 用于编译器无法验证、但调用者能够证明的底层不变量；性能优化用于测量已定位的瓶颈。二者都不是“高级 Rust 风格”，也不应为消除一个报错或猜测更快而引入。

## `unsafe` 没有关闭借用检查器

unsafe context 只允许执行几类额外操作，例如：

- 解引用 raw pointer。
- 调用 unsafe function。
- 访问/修改 `static mut`。
- 读取 union field。
- 实现 unsafe trait。
- 声明/调用需要不变量的 FFI。

普通类型检查、生命周期和借用规则仍然存在。`unsafe` 的含义是：编译器把某些证明义务交给程序员，违反后可能产生 undefined behavior，而不只是返回错误或 panic。

Rust 2024 强调 `unsafe_op_in_unsafe_fn`：即使函数声明为 `unsafe fn`，函数体中的危险操作也应放进明确的 `unsafe { ... }`。这样每个小块都能附着具体 safety argument。

## 每个 unsafe 块的证明模板

不要只写 “SAFETY: required by API”。至少回答：

```text
有效性：pointer/fd/handle 是否有效，允许什么值？
所有权：谁拥有，谁负责释放，是否只释放一次？
别名：同时有哪些引用，读写是否违反 aliasing？
生命周期：底层对象至少活到何时，回调会不会保存 pointer？
线程：API/全局状态是否允许并发调用？
布局/ABI：repr、对齐、长度、字符串编码、calling convention 是否匹配？
错误：null/-1/error code 何时检查，失败后哪些资源已取得？
```

unsafe block 应尽量小：先在安全 Rust 中验证长度、null、tag 和状态，只把无法表达的最后一步放进 block；出来后立即封装成 `OwnedFd`、slice、newtype 等安全类型。

## 安全 abstraction 的责任

一个 public safe function 内部可以使用 unsafe，但它必须对所有合法 safe 输入都保持 sound。若调用者还需满足额外条件，API 应：

- 用类型编码条件，如 `NonNull<T>`、`OwnedFd`、长度受控 slice。
- 在 safe wrapper 中检查并返回 `Result`。
- 或将函数本身标为 unsafe，并写完整 `# Safety` 文档。

“当前调用点碰巧传对”不足以让 wrapper 保持 safe。

## FFI 的 ABI 与所有权

FFI 边界需要同时核对 header/API 文档和 Rust 声明：

```rust
#[repr(C)]
struct Header {
    kind: u32,
    len: usize,
}

unsafe extern "C" {
    fn read_header(out: *mut Header) -> i32;
}
```

关键问题：

- C struct/enum 是否用 `#[repr(C)]` 或明确整数表示。
- Rust `bool`、enum、trait object、String 不能随意假设有 C ABI。
- pointer 可否 null，指向多少元素，读写权限是什么。
- 返回内存由谁分配、用哪个 allocator/free 函数释放。
- callback 是否会在函数返回后保存 Rust 引用。
- C string 是否 NUL 结尾，字节是否有效 UTF-8。
- foreign function 是否线程安全、是否设置 errno。

用 `CString`/`CStr` 处理 NUL 终止字符串；不要从外部 pointer 直接构造 `&'static str`。

## CoreFoundation 的 Create/Copy 规则

仓库 [`macos_managed.rs`](../../crates/codegen/xai-grok-config/src/macos_managed.rs) 读取 macOS managed preference：

```rust
let value_ref = unsafe {
    CFPreferencesCopyAppValue(key, application_id)
};

if value_ref.is_null() {
    return None;
}

let value = unsafe { CFType::wrap_under_create_rule(value_ref) };
value.downcast_into::<CFString>().map(|s| s.to_string())
```

安全证明分段：

1. FFI 声明与 CoreFoundation ABI 匹配。
2. `Copy` 命名规则返回 +1 owned reference，调用者必须 release。
3. null 在包装前检查。
4. `wrap_under_create_rule` 接管恰好这一个 +1 reference，所有出口都由 RAII release。
5. 先动态 downcast 为 `CFString`，不把任意 CFType 当字符串读取。
6. 返回 Rust owned String，之后不再借用 CF 对象。

同一函数先调用 `CFPreferencesAppValueIsForced`，这是安全/信任政策；FFI soundness 通过并不自动证明读取的数据在业务上可信。

## 文件描述符：owned、borrowed、raw

在 Unix：

- `OwnedFd` 拥有 fd，Drop 时 close。
- `BorrowedFd<'a>` 临时借用，不能比 owner 活得久。
- `AsFd`/`AsRawFd` 只观察，不转移所有权。
- `FromRawFd::from_raw_fd` 声明接管 raw fd，调用者必须保证唯一 ownership。
- `IntoRawFd` 放弃 RAII ownership，把关闭责任交给调用者。

最危险错误是对同一个 raw fd 调用两次 `from_raw_fd`，产生 double-close；fd number 可能已被 OS 重用，第二次 close 甚至会关闭无关资源。

仓库 [`os_pipe`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs) 直接返回两个 `OwnedFd`：

```rust
fn os_pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    #[cfg(target_os = "linux")]
    {
        nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
            .map_err(|e| std::io::Error::from_raw_os_error(e as i32))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let (read_fd, write_fd) = nix::unistd::pipe()?;
        let _ = set_cloexec(&read_fd);
        let _ = set_cloexec(&write_fd);
        Ok((read_fd, write_fd))
    }
}
```

Linux `pipe2(O_CLOEXEC)` 在创建时原子设置 close-on-exec；fallback 的 `pipe + fcntl` 在多线程 fork/exec 间有 race window。安全类型解决 close ownership，却不自动解决进程继承这一更高层协议问题。

`set_cloexec` 的 unsafe block很小：

```rust
let raw = fd.as_raw_fd();
let flags = unsafe { libc::fcntl(raw, libc::F_GETFD) };
if flags < 0 {
    return Err(std::io::Error::last_os_error());
}
```

这里 `OwnedFd` 保证调用期间 fd 有效且不会由本函数释放，返回值先检查再用于下一次 `fcntl`。

## `cfg` 不是运行时 if

`#[cfg(target_os = "linux")]` 未选中的代码不会进入当前构建的 AST/type checking。macOS 上测试通过不证明 Linux branch 能编译，反之亦然。

检查平台代码时同时看：

- source 的 `cfg` 条件是否互斥且覆盖目标。
- manifest 的 target-specific dependencies。
- CI 是否真的构建/测试目标 triple。
- cross-check 是否只 type-check，还是能够 link/run。
- fallback 是否保留相同错误和资源语义。
- 测试中使用的模拟实现是否遗漏真实 syscall 行为。

平台模块应尽早把 raw OS 差异转成共同的安全类型/trait，让上层不重复 cfg 和 unsafe。Cargo target 选择见 [06 Cargo](./06-cargo-workspace.md)。

## Rust 2024 的环境变量修改

`std::env::set_var`/`remove_var` 在 edition 2024 中是 unsafe，因为许多平台的环境读写是进程全局状态，与其他线程或 C 库并发访问可能不安全。测试中“改完再恢复”只解决值泄漏，不自动解决并发 data race。

可靠方向：

- 生产函数接收显式 config/value，不在深层读取全局 env。
- 必须测试 env 时把 mutation 放进 re-exec child process。
- 或使用全局串行机制，确保所有可能访问者都遵循它；第三方库通常无法保证。
- 用 RAII guard 恢复 unset/旧值，覆盖 panic unwind。
- 不在并行测试中随意调用 unsafe set_var。

[`macos_managed.rs`](../../crates/codegen/xai-grok-config/src/macos_managed.rs) 的测试显式标注 safety 并恢复变量；阅读这类代码时仍要确认测试是否通过串行约束隔离其他线程。

## unsafe 与 panic/cancellation

unsafe 代码必须在错误、panic 和 future 取消时仍保持资源/别名规则：

- ownership 转移前后只能有一个 owner。
- 部分初始化用 `MaybeUninit` 时只 drop 已初始化元素。
- 注册 foreign callback 后，取消不能先 drop callback 捕获的数据。
- pin 相关 pointer 在 destructor 完成前不能移动。
- lock-free 结构的内存回收必须晚于所有 reader。

RAII 能处理普通作用域退出，但 `panic = "abort"`、进程崩溃和 `mem::forget` 不运行 Drop。关键跨进程一致性需要事务、原子 rename、journal 或启动恢复，而不只靠析构。

## 性能先定义问题

“更快”必须具体化：

| 目标 | 典型指标 |
| --- | --- |
| 交互响应 | p50/p95/p99 latency、首 token 时间 |
| 吞吐 | requests/tasks/bytes per second |
| CPU | samples、CPU time、wakeups |
| 内存 | live bytes、peak/RSS、allocation count |
| 并发 | lock wait、queue depth、backpressure |
| 启动 | cold/warm startup、I/O 与解析时间 |

同时写出 workload、数据规模、feature、target、profile、硬件和噪声控制。一次 debug build 的 wall-clock 不能证明 release 性能。

## 从 profile 到 benchmark

推荐顺序：

1. 用生产症状/metrics 确认值得优化。
2. profile 定位时间、分配或争用热点。
3. 提出可证伪假设，例如“重复 JSON parse 占 CPU 18%”。
4. 建代表性 benchmark 或回放 workload。
5. 先保存 correctness test。
6. 修改一个主要变量。
7. 多次测量并报告分布/置信区间，而非最好一次。
8. 检查端到端指标和内存/CPU等副作用。

Microbenchmark 只能证明微型 workload。优化 parser 5 倍不代表 session latency 5 倍；Amdahl 定律限制总体收益。

## 仓库中的性能工具

仓库已有多类证据入口：

- [`session_list.rs`](../../crates/codegen/xai-grok-shell/benches/session_list.rs)：Criterion 风格的具体操作 benchmark。
- [`fork_copy.rs`](../../crates/codegen/xai-grok-shell/benches/fork_copy.rs)：带 `required-features = ["test-support"]` 的 bench target。
- [`skills_watcher_startup.rs`](../../crates/codegen/xai-grok-shell/benches/skills_watcher_startup.rs)：启动路径测量。
- `dhat-heap` feature 和相关 soak/integration tests：heap allocation/liveness 证据。
- runtime CPU/heap profile 模块：真实进程诊断。

运行前先读对应 `Cargo.toml` 的 bench target、harness 和 required-features；不要假设 `cargo bench -p package` 会选择正确场景。命令范围见 [06 Cargo](./06-cargo-workspace.md)。

## 常见安全优化优先级

在 unsafe 前通常有更高收益的安全改动：

- 避免重复 I/O、网络 round trip、解压和解析。
- 修复无界 channel/队列与背压。
- 缩短高争用锁临界区。
- 把不变数据预计算并缓存，明确失效策略。
- 传 slice/borrow 避免不必要大对象 clone。
- clone `Arc`/`Bytes` 等共享 handle，而不是复制 payload。
- 批量系统调用或持久化写入。
- 选择适合访问模式的数据结构。
- 删除 debug/trace 热循环中的高成本格式化。

但 clone 也不能机械删除：有时 owned snapshot 是释放锁、跨 await 或简化生命周期的正确成本。用 profile 决定。

## 为什么 unsafe 不自动更快

bounds check、iterator、slice copy 和 enum match 常会被优化掉。手写 raw pointer 可能：

- 阻止优化器理解 aliasing。
- 引入 UB，使 benchmark 结果没有意义。
- 增加维护成本并限制重构。
- 优化错误层级，实际瓶颈仍是 I/O/锁。
- 让跨平台 fallback 不一致。

只有 profile 指向热点、safe 实现无法达到目标、benchmark 证明改善且不变量可审查时，才考虑局部 unsafe。保留 safe reference implementation 做差分/property test 很有价值。

## 性能改动的正确性门

优化不能只看 benchmark 绿：

- 输出与错误语义等价。
- 并发取消、shutdown、backpressure 不退化。
- wire/持久化兼容不变。
- feature 和平台 branch 都验证。
- 内存峰值没有换取不可接受的增长。
- 不依赖 HashMap 偶然顺序或计时 race。
- benchmark 实际运行非 0 iterations/cases。
- 结果能在 clean/release 条件复现。

unsafe 优化还需 Miri/sanitizer（适用时）、边界/对齐/空输入测试和至少一位能复述 safety proof 的 reviewer。工具没覆盖的 FFI/OS 行为仍需目标平台测试。

## 阅读练习

1. 对 [`set_cloexec`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs) 的两个 unsafe call 各写一份有效性、所有权、线程和错误证明。
2. 对 [`read_forced_requirements`](../../crates/codegen/xai-grok-config/src/macos_managed.rs) 从 FFI 返回到 String 画出 +1 reference 的唯一 owner 和所有 early return。
3. 找一个 target-specific module，比较 cfg、manifest dependency 和 CI 证据；指出当前机器无法证明的部分。
4. 选择一个 bench 文件，先写 workload、metric、feature/profile 和它不代表什么，再运行 `--help`/`--list` 或实际 benchmark。
5. 找一处大 clone，判断它是浪费、锁外 snapshot 还是跨 task ownership；没有 profile 证据时不修改。

完成标准：能为 unsafe block 写出可审查的完整证明，并能用 profile、代表性 benchmark 和 correctness matrix 支持性能改动，而不是凭“更底层”或单次耗时判断。
