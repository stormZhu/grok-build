# 08 · 构建与排障手册

本手册只记录本仓库已经实际遇到、且有明确修复方式的问题。学习路径和一般构建要求见 [01-learning-guide.md](./01-learning-guide.md)。

---

## 1. `dotslash` 或 `protoc` 缺失

### 症状

构建 `xai-grok-tools-api` 或其他 protobuf 相关 crate 时，可能看到：

```text
bin/protoc found at `../../../bin/protoc` but failed to execute:
protoc --version failed, likely dotslash is missing
env: dotslash: No such file or directory
...
`protoc` not found
...
protoc command failed
```

错误最后可能表现为 `build-script-build` 的 `exit status: 101` 或 `build.rs` 中的 `unwrap()` panic。它不表示 `xai-grok-tools-api` 的 Rust 逻辑有问题；真正的根因在前面的 `dotslash` / `protoc` 报错。

### 原因

本仓库把 `bin/protoc` 作为 hermetic 启动器。它需要 [DotSlash](https://dotslash-cli.com) 下载并运行匹配平台的 `protoc`。如果 DotSlash 不存在，它才会尝试从 `PATH` 或 `$PROTOC` 找系统安装的 `protoc`。

因此以下两个条件同时不满足时，protobuf 代码生成会失败：

1. `dotslash` 不在 `PATH`；
2. 系统中也没有可用的 `protoc`。

### 修复

推荐使用仓库指定的 DotSlash 方式：

```sh
cargo install dotslash
dotslash --help
bin/protoc --version
```

成功时，最后一条通常输出类似：

```text
libprotoc 29.3
```

随后**重跑原始命令**，而不是只重跑失败 crate 的 build script：

```sh
cargo run -p xai-grok-pager-bin
```

`cargo install` 默认把可执行文件安装到 `~/.cargo/bin`。如果安装成功但仍报 `dotslash: No such file or directory`，检查 shell 的 `PATH`：

```sh
command -v dotslash
printf '%s\n' "$PATH"
```

确保 `~/.cargo/bin` 在输出中；修改 shell 配置后打开一个新终端，或重新加载配置。

### 这次的验证结果

本项目在 macOS 上完成修复后的关键输出是：

```text
DotSlash 0.5.7
libprotoc 29.3
```

之后 `xai-grok-tools-api` 的 protobuf 输出文件已生成，完整 `xai-grok-pager` 二进制也能构建并执行 `--help`。

---

## 2. 最小成功验证

完成环境修复后，按由小到大的顺序验证：

```sh
# 外部 protobuf 工具
bin/protoc --version

# 入口包的编译和链接；--help 不会进入交互 TUI
cargo run -p xai-grok-pager-bin -- --help

# 需要实际使用 TUI 时再运行
cargo run -p xai-grok-pager-bin
```

如果 `--help` 能正常打印 `Grok Build TUI` 和命令列表，就说明可执行文件已成功构建并完成启动前的 CLI 初始化。实际 TUI 运行还可能需要登录、网络和可用模型，这些属于运行时配置问题，与 protobuf 编译问题分开排查。

---

## 3. 启动后的认证与 API key

默认用户数据目录是 `~/.grok`。设置 `GROK_HOME` 后，程序会改用该目录；以下默认路径都应相应理解为 `$GROK_HOME/...`。

| 路径或变量 | 作用 | 是否应手工写入密钥 |
|---|---|---|
| `XAI_API_KEY` | API key 的推荐传入方式 | 是，但应写入 shell 环境或安全的 secret 管理器 |
| `~/.grok/config.toml` | 用户级主配置 | 仅在确有必要时；内容为明文 |
| `~/.grok/auth.json` | `grok login` 后自动保存的 OAuth/OIDC 凭据 | 否，由程序管理 |
| `.grok/config.toml` | 项目级 MCP、插件和权限配置 | 否，绝不能提交 API key |

### 默认连接的域名

使用 xAI 的默认模型时，**不需要手工配置域名**。程序内置以下第一方地址：

| 地址 | 用途 |
|---|---|
| `https://cli-chat-proxy.grok.com/v1` | CLI 默认模型与产品服务的代理端点 |
| `https://api.x.ai/v1` | 直接 xAI API 基址，用于 API key 探测和部分直连 API 操作 |
| `https://grok.com` | 浏览器登录入口 |

因此，只有 `XAI_API_KEY` 时可直接启动；不要为了使用 xAI key 再额外设置 base URL。

如果使用第三方或自建的 OpenAI 兼容网关，才需同时指定模型域名。例如：

```sh
export GROK_MODELS_BASE_URL="https://gateway.example.com/v1"
export XAI_API_KEY="gateway-key"
cargo run -p xai-grok-pager-bin
```

也可在**用户级** `~/.grok/config.toml` 中设置：

```toml
[endpoints]
models_base_url = "https://gateway.example.com/v1"
```

设置 `models_base_url` 后，模型目录请求会发往 `GET https://gateway.example.com/v1/models`，未单独设置 `base_url` 的模型也会把它作为默认推理端点。该地址应是模型 API 的 `/v1` 根路径；不要把 `cli_chat_proxy_base_url` 当作通用第三方模型网关配置。多中转站场景下，优先为每个模型设置自己的 `base_url`，详见下文。

### 中转站模型配置的格式

中转站模型应配置在用户级 `~/.grok/config.toml` 中。一份最小配置由模型选择表 `[models]` 和一个或多个模型表 `[model.<别名>]` 组成：

```toml
[models]
default = "my-model"
web_search = "my-model"

[model.my-model]
model = "provider-model-id"
name = "在 TUI 中显示的名称"
base_url = "https://gateway.example.com/v1"
api_backend = "chat_completions"
context_window = 128000
env_key = "GATEWAY_API_KEY"
```

各字段的含义如下：

| 字段 | 含义 |
|---|---|
| `[models].default` | 默认启用的模型**别名**，对应一个 `[model.<别名>]` 表。 |
| `[models].web_search` | 需要后端搜索能力时使用的模型别名；通常与 `default` 相同。 |
| `[model.<别名>]` | 模型的本地配置。别名只用于本机选择，例如 `/model <别名>`，不直接发送给中转站。含连字符的别名可写成 `[model."my-model"]`。 |
| `model` | 发给中转站 API 的真实模型 ID，必须与 `/v1/models` 的返回值一致。 |
| `name` | TUI 右下角和模型选择界面显示的名称；修改它不会改变实际调用的模型。 |
| `base_url` | 中转站 API 根路径，写到 `/v1` 为止；程序会自行补上 `/responses` 或 `/chat/completions`。 |
| `api_backend` | 协议类型：`responses` 或 `chat_completions`。必须与中转站实际兼容的接口匹配。 |
| `context_window` | 模型上下文窗口大小，用于计算自动压缩阈值；不是单次输出 token 上限。 |
| `supports_backend_search` | 中转站实现了后端搜索时设为 `true`；不确定时设为 `false` 或省略。 |
| `env_key` | 保存 API key 的环境变量名。推荐使用它，避免将密钥明文写进配置文件。 |
| `api_key` | 明文 API key。CC Switch 的 provider 记录可使用该字段，但个人配置和仓库文档不建议使用。 |

### `/model` 的展示与切换

`/model` 展示的是运行时模型目录。目录按以下顺序合并，后者覆盖前者：

```text
程序内置默认模型
        ↓
全局模型列表端点的 /v1/models 结果或本地缓存
        ↓
~/.grok/config.toml 中的 [model.<别名>]
```

下拉框使用 `name` 作为展示文字；当前模型会标为 `(current)`。切换时可输入 `name` 或模型目录的**别名**，例如：

```toml
[model.poly-grok]
model = "grok-4.5"
name = "Poly Grok"
```

```text
/model Poly Grok
/model poly-grok
```

`model = "grok-4.5"` 是请求 API 时发送的真实 ID，不应把 `/model grok-4.5` 当作可靠的切换方式，除非它刚好也是该条目的别名。选中模型后，推理请求使用该条目自己的 `base_url`、`api_backend`、`env_key` 和 `model`。

`disabled_models` 会从目录移除模型；`hidden_models` 会隐藏模型；认证方式不兼容的模型也不会显示。它们适用于需要限制 `/model` 候选项的场景。

### 全局 `models_base_url` 与多个中转站

普通中转站通常只需要配置 `models_base_url`，不需要同时配置两个地址：

```toml
[endpoints]
models_base_url = "https://gateway.example.com/v1"
```

程序会自动请求 `GET https://gateway.example.com/v1/models`。它不会遍历每个 `[model.<别名>].base_url` 分别请求 `/v1/models`；模型推理时仍优先使用具体模型自己的 `base_url`。

只有在“模型列表地址”和“默认推理地址”确实分离时，才额外使用 `models_list_url`：

```toml
[endpoints]
models_base_url = "https://gateway.example.com/v1"
models_list_url = "https://catalog.example.com/v1/models"
```

此时 `models_list_url` 只覆盖模型发现请求的完整 URL；`models_base_url` 仍可作为未单独设置 `base_url` 的模型的默认推理端点。列表地址优先级为 `models_list_url`、`models_base_url + "/models"`、CLI 默认代理端点。多中转站时，不要依赖它自动合并多个远程模型列表；应把需要切换的模型逐个写为 `[model.<别名>]`。

### 本地配置与远程列表冲突

冲突按照模型目录的 key（通常就是 `[model.<别名>]` 中的别名）判断，而不是只比较 `model` 的真实 ID。本地 `[model.<别名>]` 优先级最高：

```toml
# 远程列表已有 id = "gpt-5.6-sol" 时，这份本地配置覆盖它。
[model."gpt-5.6-sol"]
model = "gpt-5.6-sol"
name = "我的 Sol"
base_url = "https://api.polymer-ai.top/v1"
api_backend = "responses"
context_window = 1000000
env_key = "POLY_API_KEY"
```

此时 `/model` 显示 `我的 Sol`，推理走本地写入的 Poly 端点。如果改用不同别名，例如 `[model.poly-sol]`，远程 `gpt-5.6-sol` 与本地 `poly-sol` 会同时出现在目录中，即使它们最终发送的是同一个 `model` 值。为避免歧义，应让别名和 `name` 都保持唯一。

下面两个示例分别对应 CC Switch 中已保存的 `poly grok` 和 `poly gpt-sol` provider。二者均使用 Poly 的 Responses API。请**二选一**完整写入 `~/.grok/config.toml`；不要把两个完整示例直接拼接，否则会重复定义 `[models]`。

### 示例 1：Poly Grok 4.5

```toml
[models]
default = "grok"
web_search = "grok"

[model.grok]
model = "grok-4.5"
name = "Grok 4.5"
base_url = "https://api.polymer-ai.top/v1"
api_backend = "responses"
context_window = 1000000
supports_backend_search = true
env_key = "POLY_API_KEY"
```

### 示例 2：Poly GPT-5.6-Sol

```toml
[models]
default = "gpt-5.6-sol"
web_search = "gpt-5.6-sol"

[model."gpt-5.6-sol"]
model = "gpt-5.6-sol"
name = "poly gpt-sol"
base_url = "https://api.polymer-ai.top/v1"
api_backend = "responses"
context_window = 1000000
supports_backend_search = true
env_key = "POLY_API_KEY"
```

### 同时配置两个模型

若希望在同一份配置中用 `/model` 切换，将两个模型表保留在一起，并且只保留一个 `[models]` 表：

```toml
[models]
default = "grok"
web_search = "grok"

[model.grok]
model = "grok-4.5"
name = "Grok 4.5"
base_url = "https://api.polymer-ai.top/v1"
api_backend = "responses"
context_window = 1000000
env_key = "POLY_API_KEY"

[model."gpt-5.6-sol"]
model = "gpt-5.6-sol"
name = "Poly GPT-5.6-Sol"
base_url = "https://api.polymer-ai.top/v1"
api_backend = "responses"
context_window = 1000000
env_key = "POLY_API_KEY"
```

随后可使用 `/model Grok 4.5` 或 `/model Poly GPT-5.6-Sol` 切换；也可使用别名 `/model grok` 和 `/model gpt-5.6-sol`。`default` 决定下次启动时默认选择的模型。

启动前在当前 shell 或 secret manager 中提供自己的 key：

```sh
export POLY_API_KEY="你的 Poly API key"
cargo run -p xai-grok-pager-bin
```

Poly 的 `grok-4.5` 已完成 `/v1/models` 的鉴权和模型存在性检查。两个示例使用的 `responses` 是其当前 provider 配置；其他中转站若报 `missing field annotations`，说明其 Responses API 实现不完整，应使用它支持的 `chat_completions` 接口，不要照抄 `responses`。

CC Switch 每次切换 provider 都会重写 `~/.grok/config.toml`。可以保存脱敏备份（例如 `~/.grok/config_poly.toml`），但绝不能将包含真实 key 的配置提交到仓库。

### 推荐：环境变量

适用于本地 shell、CI 和无法打开浏览器的环境：

```sh
export XAI_API_KEY="xai-..."
cargo run -p xai-grok-pager-bin
```

需要跨终端保留时，将 `export XAI_API_KEY=...` 放入个人 shell 配置或由 secret manager 注入；不要写入仓库的 `.env`、`.grok/config.toml` 或提交到 Git。

### 浏览器登录

直接启动程序或执行登录命令会使用浏览器认证，并自动将可刷新凭据写入：

```sh
target/debug/xai-grok-pager login
```

凭据路径为 `~/.grok/auth.json`，Unix 上以 owner-only 权限保存。不要复制或分享该文件；需要切换账户或改用环境变量 key 时，优先执行：

```sh
target/debug/xai-grok-pager logout
```

### 可选：在用户配置中设置模型 key

只有需要给特定模型覆盖凭据时，才在**用户级** `~/.grok/config.toml` 中设置：

```toml
[model.grok-build]
api_key = "xai-..."
```

这会把 key 明文保存在磁盘，因此通常不如环境变量合适。项目级 `.grok/config.toml` 只加载 MCP、插件和权限相关内容，不应用于模型 API key，也不应携带任何个人密钥。

### 凭据优先级

从高到低依次为：

1. `[model.<name>]` 中的 `api_key` 或 `env_key`；
2. `~/.grok/auth.json` 中的活跃浏览器/OIDC/外部登录 token；
3. `XAI_API_KEY`（兼容名称：`GROK_CODE_XAI_API_KEY`）。

因此，如果已经使用浏览器登录，单独设置 `XAI_API_KEY` 不会覆盖登录 token。执行 `logout` 清除自动管理的登录凭据后，环境变量 key 才会成为默认认证方式。
