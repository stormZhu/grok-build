# 19. 平台 API、`unsafe` 与性能边界

这是按需专题。普通业务逻辑不应为“更快”或“更底层”直接进入 `unsafe`；先确认安全抽象和测量无法满足需求。

## `unsafe` 的规则

`unsafe` 不关闭 Rust 的全部检查，只允许少数需要调用者证明正确性的操作。每个 unsafe 块附近应能回答：它依赖哪些不变量？谁建立这些不变量？出错时会怎样？把块压缩到最小，并用安全函数封装其余逻辑。

Rust 2024 中修改进程环境变量也要求 `unsafe`，因为它与其他线程存在全局竞争；测试中应局部化、串行化并恢复环境。

## 文件描述符与 FFI

`OwnedFd` 表示拥有并负责关闭的描述符，`AsRawFd` 只临时借用裸值。把同一个 raw fd 多次交给 `from_raw_fd` 会导致重复关闭，因此从 raw 转 owned 的所有权转移必须唯一。

## 项目中的锚点

- [`os_pipe`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs#L566) 包含 pipe、`OwnedFd`、`fcntl` 和从 raw fd 转换的封装。
- [`cgroup::linux`](../../crates/codegen/xai-grok-tools/src/computer/local/cgroup.rs#L76) 是 Linux 系统调用和 RAII 清理的进阶实例。
- [`read_forced_requirements`](../../crates/codegen/xai-grok-config/src/macos_managed.rs#L76) 展示 macOS FFI 及其测试中的环境变量约束。

### 仓库代码摘录：将 FFI 所有权规则编码回安全类型

`macos_managed.rs` 从 CoreFoundation 取得 `Copy` 规则返回的 +1 引用后，立即交给拥有型包装：

```rust
// 源码节选：CoreFoundation 的 Copy 规则返回 +1 引用，
// 因此随后必须由某个拥有者负责 release。
let value_ref = unsafe {
    CFPreferencesCopyAppValue(key, application_id)
};

// 先检查空指针；把空指针包装为 CFType 会违反封装前提。
if value_ref.is_null() {
    return None;
}

// wrap_under_create_rule 接管这一个 +1 引用的释放责任。
let value = unsafe { CFType::wrap_under_create_rule(value_ref) };
```

此处的安全前提是 API 的 Copy 规则确实给调用者一个待释放的引用；`CFType` 接管释放责任。随后先 downcast 为 `CFString`，避免把非字符串按字符串 API 读取而产生未定义行为。

### 项目关键代码：平台差异封装在安全返回类型之后

[`os_pipe`](../../crates/codegen/xai-grok-tools/src/computer/local/shell_state.rs#L566) 对外返回 `OwnedFd`，调用者无需管理裸文件描述符关闭：

```rust
fn os_pipe() -> std::io::Result<(OwnedFd, OwnedFd)> {
    #[cfg(target_os = "linux")]
    {
        // Linux 用 pipe2 原子设置 O_CLOEXEC，避免 fork 继承 fd。
        nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC)
            .map_err(|e| std::io::Error::from_raw_os_error(e as i32))
    }

    #[cfg(not(target_os = "linux"))]
    {
        let (read_fd, write_fd) = nix::unistd::pipe()
            .map_err(|e| std::io::Error::from_raw_os_error(e as i32))?;
        // macOS 没有可用 pipe2 封装，退化为分别设置 close-on-exec。
        let _ = set_cloexec(&read_fd);
        let _ = set_cloexec(&write_fd);
        Ok((read_fd, write_fd))
    }
}
```

`OwnedFd` 让资源在离开作用域时关闭；`unsafe` 被限制在 `set_cloexec` 的 FFI 调用内，绝大多数上层逻辑不需要接触 raw fd。

## 性能方法

先用 profile、指标或基准定位瓶颈，再修改。优先减少不必要的 I/O、重复解析和过长锁临界区；之后才评估分配、容器和原子优化。优化改动必须保留正确性测试，并对吞吐、延迟、内存或锁争用中的目标指标给出前后证据。

## 阅读检查点

找出一个 unsafe 块，写下其所有权、线程、指针/句柄有效性和错误处理不变量；若无法写出，就不要修改它。
