# Z&S-AI 项目构建提示词（纯架构版 v2）

## 项目概述

构建一个 AI 角色扮演桌面应用，采用**微内核+插件化架构**，支持**插件热加载**。

架构设计的核心取舍：**功能扩展零成本，协议扩展有代价但可控**。

项目名：ZS-AI

---

## 核心架构原则

1. **内核（core）只做调度，不做业务**
   - 读配置 → 按阶段和顺序执行插件 → 传递 Context
   - 内核不关心插件在做什么，不实现任何业务逻辑

2. **插件（plugins/）是唯一的功能载体**
   - 每个插件独立编译为动态库（`.so`/`.dylib`/`.dll`）
   - 实现 Plugin trait
   - 插件间不直接调用，不相互依赖，只通过 Context 通信
   - **支持运行时热加载**

3. **Context 是唯一的数据通道，但分两层设计**
   - **协议层**（固定字段）：仅包含流水线执行必需的数据——`session_id`、`phase`、`abort`、`messages`、`user_input`、`ai_response`
   - **扩展层**（`custom`）：所有其他数据——角色卡、API 配置、流式通道、token 统计……全部走 `custom`
   - 协议层尽量少改，扩展层随便塞

4. **阶段（Stage）由配置定义，不写死在代码里**
   - 阶段名称是字符串，顺序由 config.toml 的 `[stages].order` 决定
   - 加新阶段只需改配置，不改内核代码，不重编译插件

5. **插件按能力（Capability）暴露功能，非按固定方法签名**
   - Plugin trait 只定义 `metadata()` + `execute()` 两个必须方法
   - 其余功能（健康检查、指标、配置校验……）全部是带默认实现的可选方法
   - 新能力 = 在 trait 里加一个带默认实现的方法，旧插件不受影响

6. **热加载机制**
   - 监听 plugins/ 目录
   - 检测到动态库文件变化时自动重载
   - 重载时保留插件状态（通过 Context.custom 传递）
   - 无需重启整个应用

---

## 目录结构

```
ZS-AI/
├── Cargo.toml                    # 工作区配置
├── config.toml                   # 运行配置（定义阶段、插件、参数）
├── core/                         # 微内核
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs
│       ├── context.rs            # Context（协议层最小化 + 扩展层 custom）
│       ├── plugin.rs             # Plugin trait（metadata + execute + 可选能力）
│       ├── pipeline.rs           # 流水线编排（按配置的阶段顺序执行）
│       ├── config.rs             # config.toml 解析
│       ├── dynamic_loader.rs     # 动态库加载器
│       ├── hot_reload.rs         # 热加载管理器
│       └── error.rs              # 错误类型
├── plugins/                      # 插件（各自独立编译为 cdylib）
│   ├── character_card/           # 角色卡解析
│   ├── prompt_builder/           # 提示词构建
│   ├── api_client/               # LLM API 调用
│   ├── history/                  # 对话历史管理
│   ├── stream_parser/            # SSE 流式解析
│   └── *_/                       # 新插件直接在此创建目录即可
├── server/                       # HTTP/WebSocket 服务
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── routes/               # REST 路由
│       └── ws.rs                 # WebSocket 处理
└── electron-app/                 # Electron 前端（第三阶段）
```

---

## 内核设计要点

### Context：协议层 vs 扩展层

Context 字段分为两类。协议层字段修改需要谨慎（影响所有插件），扩展层字段随时可用。

**协议层（固定，尽量不改）**：

| 字段 | 类型 | 说明 |
|------|------|------|
| session_id | String | 会话唯一标识（支持多会话并行的基础） |
| phase | String | 当前阶段名称（字符串，非枚举） |
| abort | bool | 是否中止流程 |
| user_input | Option\<String\> | 用户本轮输入 |
| ai_response | Option\<String\> | AI 回复 |
| messages | Vec\<Message\> | 完整对话历史（system/user/assistant） |

**扩展层（插件间通信的唯一通道，分两层）**：

| 字段 | 类型 | 说明 |
|------|------|------|
| custom | HashMap\<String, Value\> | **可序列化数据**（JSON 兼容）。角色卡、配置、token 统计等。key 规范：`插件名.字段名` |
| opaque | HashMap\<String, Box\<dyn Any + Send + Sync\>\> | **不可序列化数据**（运行时句柄）。函数指针、channel sender、网络连接等。key 规范：`插件名.字段名` |

**为什么分两层**：

`custom` 里的函数指针（`stream_parser.parse_fn`）和 WebSocket sender 句柄（`stream_parser.sender`）无法序列化为 JSON。如果强行用 `Value` 存这些，会在热加载状态保存/恢复时崩溃——`serde_json::Value` 不接受 `fn` 指针或 `tokio::Sender`。

- **`custom`**：可以安全地序列化到磁盘（热加载状态持久化、调试日志）
- **`opaque`**：仅存在于内存中，生命周期绑定到 Context 实例，不参与序列化。热加载时由 server 层重新注入

**两层承载的数据示例**：

`custom`（可序列化）：
- `character_card.data` — 角色卡 JSON
- `api_client.config` — API 配置（api_key、model、base_url 等）
- `api_client.token_usage` — 最后一次请求的 token 用量
- `prompt_builder.output` — 构建好的 system prompt
- `stream_parser.accumulator` — 流式回复完整累积文本
- `history.save_directory` — 历史存储路径
- `session.config` — 会话级配置

`opaque`（不可序列化）：
- `stream_parser.parse_fn` — SSE 解析函数指针（`fn(&[u8]) -> Vec<String>`）
- `stream_parser.sender` — WebSocket 推送通道（`UnboundedSender<String>`）
- （未来）`tts.audio_stream` — TTS 音频输出通道
- （未来）`image_gen.callback` — 图像生成完成回调

**设计意图**：如果将来要加 `temperature`、`top_p`、`user_id`、`conversation_id`……全部走 `custom` 或 `opaque`，Context 结构体本身不需要改。

### Plugin trait：最小必须 + 可选能力

```rust
pub trait Plugin: Send + Sync {
    // ========== 必须实现（2 个） ==========
    
    /// 返回插件元信息
    fn metadata(&self) -> PluginMeta;
    
    /// 核心执行逻辑，读写 ctx
    fn execute(&self, ctx: &mut Context) -> Result<PluginResult>;
    
    // ========== 可选能力（带默认实现，不实现就用默认） ==========
    
    /// 初始化：config.toml 中该插件的配置段
    fn init(&mut self, config: &Value) -> Result<()> { Ok(()) }
    
    /// 关闭：保存状态、释放资源
    fn shutdown(&mut self) -> Result<()> { Ok(()) }
    
    /// 健康检查：返回插件是否正常
    fn health_check(&self) -> Result<HealthStatus> { Ok(HealthStatus::default()) }
    
    /// 暴露指标：供监控用
    fn metrics(&self) -> HashMap<String, f64> { HashMap::new() }
    
    /// 校验配置：init 前调用，检查 config.toml 中配置是否合法
    fn validate_config(&self, config: &Value) -> Result<()> { Ok(()) }
    
    /// 阶段进入/退出钩子：用于日志、计时等横切关注点
    fn on_stage_enter(&self, _stage: &str) -> Result<()> { Ok(()) }
    fn on_stage_exit(&self, _stage: &str) -> Result<()> { Ok(()) }
    
    /// 热重载钩子：重载前/后通知
    fn before_reload(&mut self) -> Result<()> { Ok(()) }
    fn after_reload(&mut self) -> Result<()> { Ok(()) }
}
```

**PluginMeta**：

| 字段 | 类型 | 说明 |
|------|------|------|
| name | String | 插件唯一标识，与 config.toml 中 key 对应 |
| version | (u16, u16, u16) | 语义化版本 |
| stage | String | 所属阶段名称（字符串），如 `"preprocess"` |
| priority | i32 | 阶段内优先级（小值先执行） |
| capabilities | Vec\<String\> | 实现了哪些可选能力，如 `["health_check", "metrics"]` |

**`capabilities` 的定位**：纯声明式，**内核在运行时不会查询它**。因为所有可选 trait 方法都有默认实现，内核直接调用即可，不需要事先检查 capability 列表。它的实际用途是：
- `/health` 端点遍历所有插件时，筛选出"声明了 `health_check` 的插件"来收集健康状态
- 监控 / 日志 / 调试工具读取 `metadata()` 来展示插件能力矩阵
- 插件开发者阅读代码时快速了解某个插件提供了哪些扩展接口

**PluginResult**：包含 `stop_propagation: bool`（跳过同阶段后续插件）。

**对象安全约束**：`Plugin` trait 保持 object-safe——所有方法无泛型参数、不返回 `Self`、不用 `impl Trait`。

### 阶段系统：字符串 + 配置驱动

**不硬编码阶段枚举**。阶段由 config.toml 定义：

```toml
[stages]
order = ["preprocess", "build_prompt", "api_call", "postprocess"]
```

内核读取 `order` 列表，按序执行。每个插件通过 `metadata().stage` 声明自己属于哪个阶段。

**加新阶段的成本**：
- 修改 config.toml 的 `order` 列表 ✅（不改代码）
- 新插件声明自己属于新阶段 ✅（不改内核）
- 旧插件完全不受影响，无需重编译 ✅

**阶段执行规则**：
1. 遍历 `[stages].order` 中的每个阶段名
2. 收集所有 `stage` 匹配的插件
3. 按 `priority` 排序（值小先执行）
4. 顺序执行，插件可返回 `stop_propagation`
5. `ctx.abort` 则跳转到 `error_stage`

### 插件执行顺序的最终仲裁

顺序由 **config.toml + metadata().stage + metadata().priority** 三者共同决定：

| 维度 | 来源 | 作用 |
|------|------|------|
| 有哪些阶段 | config.toml `[stages].order` | 定义阶段及顺序 |
| 插件属于哪个阶段 | `metadata().stage` | 插件自己声明 |
| 阶段内谁先谁后 | `metadata().priority` | 插件自己声明 |

不在 `[stages].order` 中的阶段不会被执行（即使有插件声明属于它）。`metadata().stage` 和 `order` 列表不匹配的插件（如声明了 `"validation"` 但 order 里没有）不会被加载——内核在加载时校验并跳过。

---

## 动态加载设计要点

### 插件编译配置

- `Cargo.toml` 中 `crate-type = ["cdylib"]`
- 编译输出：`lib插件名.so` / `lib插件名.dylib` / `插件名.dll`
- 导出函数：`#[no_mangle] pub extern "C" fn create_plugin() -> Box<dyn Plugin>`
- 依赖：仅 `core` crate + 第三方库。**插件间禁止编译时依赖**

### 动态加载器

- 使用 `libloading` 加载动态库
- 启动时扫描插件目录，按扩展名（`.so`/`.dylib`/`.dll`）发现
- 加载后立即调用 `create_plugin()` → `metadata()` 获取 name 和 stage
- 持有 `HashMap<String, PluginEntry>`，`PluginEntry = (Library, Box<dyn Plugin>)`，Library 在前
- 提供 `load(path)`、`unload(name)`、`reload(name)`、`get(name)`、`get_by_stage(stage)`
- 卸载时：`shutdown()` → 移入延迟释放队列 → 确认无引用后 drop Library

### 插件发现

- 文件扩展名识别，滤除编译副产品（`.d`、`.rmeta`、`.rlib`）
- 插件名 = 去 `lib` 前缀和扩展名
- 无注册表，文件即配置

---

## 热加载设计要点

### 文件监听

- `notify` crate 监听插件目录
- 事件：`Modify`、`Create`
- 延迟 100ms 防抖
- 忽略临时文件（`.tmp`、`#`、`~` 结尾）

### 热加载流程（与 v1 相同）

1. 检测到动态库文件变化
2. 提取插件名称，匹配已加载的插件
3. 调用 `before_reload()` → `shutdown()` → 卸载
4. 等待文件写入完成
5. 重新 `load_plugin` → 调用 `init()` → `after_reload()`
6. 从 `custom["插件名.state"]` 恢复状态

### 线程模型与内存安全——延迟释放

- `Arc<RwLock<DynamicLoader>>` 共享加载器
- 重载时获取写锁，旧 `(Library, Plugin)` 移入 `deferred_drops` 队列
- 写锁释放后（确认无读锁持有者）再 drop 待释放项
- 正在执行的请求持有读锁，使用旧实例完成，不受影响
- 不用 `Arc<Library>` 方案——`libloading::Library` 不实现 Clone

### 热加载的兼容性边界

| 变更类型 | 热加载 | 说明 |
|---------|--------|------|
| 插件实现细节修改 | ✅ 支持 | 重编译 .so 即可 |
| 新增可选 trait 方法（有默认实现） | ✅ 支持 | 旧插件用默认实现 |
| 修改 `execute()` 签名 | ❌ 不支持 | 需要重编译所有插件 + 重启 |
| 修改 `metadata()` 签名 | ❌ 不支持 | 同上 |
| 修改 Context 协议层字段 | ❌ 不支持 | 影响所有插件的 `execute()` |
| 插件新增依赖（除 core 外） | ⚠️ 依赖方需重编译 | 热加载本身无影响 |
| Windows DLL 替换 | ⚠️ 需临时文件+重命名 | 文件锁限制 |

---

## 流水线执行要点

### 单次请求流程

```
          HTTP/WS 请求到达
                 │
                 ▼
           创建 Context，设置 session_id、user_input
                 │
                 ▼
    ┌──── 遍历 config.toml [stages].order ────┐
    │                                          │
    │   当前阶段（如 "preprocess"）              │
    │     │                                    │
    │     ├── 收集该阶段所有插件（按 priority 排序）│
    │     ├── 逐个执行插件                       │
    │     │    ├── stop_propagation? → 跳出     │
    │     │    └── ctx.abort? → 跳到 [stages].error_stage  │
    │     └── 阶段结束                           │
    │                                          │
    │   进入下一个阶段（如 "build_prompt"）       │
    │     ...                                  │
    │                                          │
    └──── error_stage 执行完毕 ─────────────────┘
                 │
                 ▼
           返回 Context（ai_response / error）
```

### 中断机制

- `ctx.abort = true`：跳过中间所有阶段，直接进入配置中显式声明的 **`error_stage`**（如 `"postprocess"`，由 config.toml 的 `[stages].error_stage` 指定）
- **为什么不用"最后一个阶段"**：如果有人在 `order` 末尾加了其他阶段（如 `"metrics"`、`"cleanup"`），依赖"最后一个"不可靠。显式配置 `error_stage` 让意图明确、行为可预测
- `PluginResult.stop_propagation = true`：跳过当前阶段剩余插件，进入下一阶段

### 错误处理

- 插件 `execute()` 返回 `Err` → 错误信息写入 `custom["error"]`
- 默认不中断流程（除非同时设置 `abort`）
- `error_stage` 指定的目标阶段的插件负责统一构造错误响应

---

## 流式处理设计

### 问题

`api_client` 发起流式 HTTP 请求后逐 chunk 接收数据，需要 `stream_parser` 解析 SSE，同时逐 token 推送到 WebSocket。但 `execute()` 是同步调用，无法跨插件传递异步回调。

### 方案：api_client 主驱动 + stream_parser 纯函数

**stream_parser 插件**的定位：提供**纯函数库**而非管道参与者。它的 `execute()` 为空（或仅做初始化校验），实际工作通过 `custom` 暴露：

- `custom["stream_parser.parse_fn"]` — SSE 解析函数指针，签名为 `fn(&[u8]) -> Vec<String>`
- 设计为独立插件的原因：不同 API 的 SSE 格式不同，可通过替换 `.so` 文件切换解析器

**api_client 插件**的 `execute()` 逻辑：
1. 检查 `custom["api_client.config"].stream`
2. 非流式：HTTP 请求 → 写入 `ctx.ai_response` → 返回
3. 流式：
   - 从 `custom["stream_parser.parse_fn"]` 获取解析函数
   - 发起流式请求 → 循环读 chunk → 调用解析函数 → 通过 `custom["stream_parser.sender"]` 推送 WebSocket
   - 同时累积到 `custom["stream_parser.accumulator"]`
   - 流结束：写入 `ctx.ai_response`

**WebSocket 推送通道**由 server 层在创建 Context 时注入到 `custom["stream_parser.sender"]`。

**为什么不用 Context 协议层字段**：`stream_sender`、`stream_accumulator` 只有流式场景用，放协议层会污染所有插件的视野。走 `custom` 更干净，非流式插件完全不需要知道它们存在。

---

## 多会话 / 多角色支持

### Session 生命周期

```
POST /api/session         客户端请求创建会话
  │  请求体: {"character": "character.json", "model": "deepseek-v4-flash"}
  ▼
server 层生成 session_id（UUID v7）
  │
  ├── 将 session 元信息写入 `data/sessions/{session_id}/meta.json`
  │   （包含 character 引用、model、创建时间等）
  │
  └── 返回 {"session_id": "019abcd-...", "character_name": "李白"}
  
       客户端后续所有请求带上 session_id:
         POST /api/chat          {"session_id": "...", "message": "..."}
         WS /ws/chat?session_id=...
         GET  /api/history?session_id=...
         GET  /api/card?session_id=...
         PUT  /api/config?session_id=...
         POST /api/history/export?session_id=...

DELETE /api/session?session_id=...
  删除 data/sessions/{session_id}/ 目录，回收资源。
  
长时间未使用的 session（默认 7 天无活动）由后台任务自动清理。
```

### Session 状态隔离

每个 session 的数据完全隔离：

```
data/
├── sessions/
│   ├── {session_id_1}/
│   │   ├── meta.json            # 会话元信息（角色卡引用、模型、创建时间）
│   │   ├── history.json         # 对话历史
│   │   └── state.json           # 插件状态快照（可选，用于恢复）
│   └── {session_id_2}/
│       └── ...
├── characters/                  # 角色卡（跨 session 共享）
│   ├── default.json
│   ├── libai.png
│   └── ...
└── config.toml                  # 全局配置
```

### Server 层如何注入 session 信息到 Context

每次请求到达时，server 层：

1. 从请求中提取 `session_id`
2. 从 `data/sessions/{session_id}/meta.json` 读取会话元信息
3. 创建 Context，设置 `session_id`
4. 将会话元信息注入扩展层：
   - `custom["session.config"]` = 会话专属配置（character、model 等）
   - `opaque["session.session_dir"]` = 会话目录路径（`data/sessions/{session_id}/`）
   - `custom["history.path"]` = `data/sessions/{session_id}/history.json`
5. 调用 `run_pipeline(ctx)`

### 多角色支持

角色卡数据在 `custom["character_card.data"]`。当前为单个字符卡对象，多角色场景下：

- `custom["character_card.data"]` = `[{card1}, {card2}, ...]`（数组）
- `prompt_builder` 插件检测到数组格式后，构建多角色 system prompt（每个角色有独立的 `[Character]` 段）
- `history` 插件在消息中记录 `speaker` 字段区分发言者
- Context 协议层不需要任何修改——多角色完全是扩展层的语义约定

### 并发安全

- 同一 `session_id` 的消息**串行处理**——server 层为每个 session 维护一个消息队列
- 不同 session 可以并行处理（独立的 Context + 独立的 pipeline 调用）
- `history` 插件按 session 写入独立文件，无竞争

---

## 配置文件设计

### config.toml 完整结构

```toml
[hot_reload]
enabled = true
plugin_dir = "./target/debug"
delay_ms = 100

# ============================================================
# 阶段定义（顺序即执行顺序。error_stage 为 abort 跳转目标，必须在 order 中存在）
# ============================================================
[stages]
order = ["preprocess", "validate", "build_prompt", "api_call", "postprocess"]
error_stage = "postprocess"         # abort 时跳转的目标阶段

# 每个阶段可以配置阶段级的参数（如超时、重试），插件通过 ctx.custom 读取
[stages.preprocess]
timeout_ms = 5000

[stages.api_call]
timeout_ms = 30000

# ============================================================
# 全局默认配置（非插件专属的通用参数）
# ============================================================
[session]
default_character = "./data/characters/default.json"

[api]
api_type = "deepseek"
api_key = ""                              # 空 = 读环境变量
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
stream = false

[history]
save_directory = "./data/history"
max_context_tokens = 32768
save_on_every_message = true

# ============================================================
# 插件专属配置（键名与 metadata().name 对应）
# ============================================================
[plugins.character_card]
default_card_path = "./data/characters/default.json"

[plugins.api_client]
api_type = "deepseek"
timeout_seconds = 30
max_retries = 2
reasoning_effort = "medium"
thinking_type = ""

[plugins.token_counter]
model_override = ""

[plugins.history]
save_directory = "./data/history"
max_tokens = 32768
save_on_message = true
```

### 配置加载与传递

1. 启动时读取 `config.toml`
2. **校验 `error_stage`**：必须存在于 `order` 列表中，否则启动失败（防止拼写错误导致 abort 行为未定义）
3. `[stages].order` → 构建阶段列表
3. 加载所有 `.so` 插件 → 调用 `metadata()` 获取 stage → 按 stage 分组
4. 对每个插件，将 `[plugins.插件名]` 段传给 `init()`
5. 全局配置（`[api]`、`[history]` 等）由 server 层在创建 Context 时注入 `custom`
6. 热加载新插件时：加载 .so → metadata() → 校验 stage 是否在 order 中 → init(config) → 就绪

### 敏感信息处理

- `api_key` 为空时从 `DEEPSEEK_API_KEY` 环境变量读取
- `config.toml` 不提交 Git
- 后续可选：系统密钥链（macOS Keychain / Linux Secret Service）

---

## 服务端设计要点

### HTTP API

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/chat` | POST | 非流式对话 |
| `/api/card` | GET/PUT | 角色卡读写 |
| `/api/card/import` | POST | 导入 PNG 角色卡 |
| `/api/card/export` | GET | 导出角色卡为 PNG |
| `/api/prompt/preview` | POST | 预览 system prompt |
| `/api/config` | GET/PUT | 配置管理 |
| `/api/history` | GET/DELETE | 历史管理 |
| `/api/history/export` | GET | 导出历史（JSON/Markdown） |
| `/api/session` | POST | 创建新会话，返回 session_id |
| `/api/session?session_id=...` | DELETE | 销毁会话，删除相关数据 |
| `/api/session?session_id=...` | GET | 获取会话详情（角色、模型、消息数等） |
| `/health` | GET | 健康检查 |

### WebSocket 流式对话

端点：`/ws/chat?session_id=xxx`

客户端 → 服务端：`{"message": "你好！"}`

服务端 → 客户端：
```json
{"type": "partial", "content": "哈"}
{"type": "done", "token_usage": {"prompt": 512, "completion": 128}}
{"type": "error", "title": "...", "message": "..."}
```

### 服务端职责边界

服务端（server crate）的职责**仅限于**：
1. 接收 HTTP/WS 请求
2. 创建 Context，设置 `session_id`、`user_input`，将全局配置和请求元数据注入 `custom`
3. 调用内核的 `run_pipeline(ctx)` 执行完整流水线
4. 将 Context 中的结果（`ai_response`、`custom["error"]`）转为 HTTP 响应 / WS 消息返回

服务端不关心插件有哪些、阶段有几个、数据怎么处理——这些全部由 config.toml + 插件决定。

### 子进程生命周期（被 Electron 管理）

- Electron spawn Rust 二进制，后端监听随机端口，端口号写 stdout
- 崩溃自动重启，最多 3 次
- Electron 退出时 SIGTERM → 等 5s → SIGKILL

---

## 插件开发规范

### 必须遵守

1. 实现完整的 `Plugin` trait——至少 `metadata()` + `execute()`
2. 导出 `#[no_mangle] pub extern "C" fn create_plugin() -> Box<dyn Plugin>`
3. Cargo.toml 仅依赖 `core` + 第三方库，**禁止依赖其他插件**
4. 通过 `custom` 读写所有数据，key 使用 `插件名.字段名` 命名
5. 读取其他插件写入的 custom 数据时，必须处理字段不存在的默认情况

### 建议遵守

1. 需要状态持久化时，实现 `shutdown()`，将状态写入 `custom["插件名.state"]`
2. 可被外部查询的插件实现 `health_check()`
3. 需要在 `init()` 中校验配置合法性的插件实现 `validate_config()`
4. 在 `metadata().capabilities` 中声明实现了哪些可选方法

### 插件间通信

- **唯一方式**：`ctx.custom`
- **命名规范**：`插件名.字段名`
- **约定**：下游插件读上游插件写入的数据
- **禁止**：插件间直接导入彼此模块

### 插件版本管理

| 变更 | 兼容性 | 热加载 |
|------|--------|--------|
| 补丁版本号（实现修改） | ✅ | ✅ |
| 次版本号（新增可选 trait 方法，有默认实现） | ✅ | ✅ |
| 主版本号（改 `execute()` 或 `metadata()` 签名） | ❌ | ❌（需重启） |
| 改 Context 协议层字段 | ❌ | ❌（所有插件重编译） |
| 新增插件阶段（改 config.toml `[stages].order`） | N/A | ✅（仅配置变更） |

---

## 开发阶段划分

### 第一阶段：内核 + 基础插件 + 最小 HTTP（2-3 周）

- 实现 core crate：Context、Plugin trait（含全部可选方法默认实现）、Pipeline（字符串阶段驱动）、Config
- 实现动态加载器（libloading）
- 实现热加载管理器（notify）
- 实现三个基础插件：character_card、prompt_builder、api_client
- 实现 server crate（axum，`/api/chat` + `/health`）
- 验证闭环：HTTP 请求 → 加载角色卡 → 构建 prompt → 调用 LLM → 返回回复

### 第二阶段：完善插件 + 流式 + 多会话（1-2 周）

- 实现 history 插件（对话历史读写、上下文裁剪、按 session 隔离存储）
- 实现 stream_parser 插件（SSE 纯函数 + WebSocket 推送）
- 实现 session 管理（POST/DELETE/GET 端点、session 目录结构、元信息维护、超时自动清理）
- 补齐其余 REST API 端点
- 完善错误处理与日志
- 验证多会话场景：创建两个 session，分别加载不同角色卡，交替发送消息，确认历史隔离正确

### 第三阶段：Electron 前端（2 周）

- Electron + React + TypeScript
- 后端子进程管理（spawn → 读端口 → 监控重启）
- 对话界面（气泡、流式逐字显示）
- 角色卡编辑器 + 导入导出
- 设置面板
- Material Design 3 主题

---

## 关键技术决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| 阶段定义方式 | 字符串 + config.toml | 加阶段不改代码，微内核的本质要求 |
| Plugin trait | metadata + execute 必须，其余可选默认实现 | 加能力不改签名，旧插件不受影响 |
| Context 数据分层 | 协议层 6 字段 + 扩展层 custom | 核心稳定，扩展自由 |
| 动态库格式 | cdylib（C ABI） | 跨平台稳定，libloading 成熟 |
| 插件发现 | 扩展名扫描 | 复制 .so 即安装 |
| 热加载旧实例安全 | 延迟释放队列 | 不用 Arc，实现简单，无内存风险 |
| 插件隔离 | Context 纯数据，禁直接导入 | 最大解耦，独立编译和热加载 |
| 流式架构 | api_client 主驱动，stream_parser 纯函数 | 避免跨插件传递异步回调，ownership 清晰 |
| HTTP 框架 | axum | tokio 生态，类型安全 |

---

## 与 v1 的关键差异

| 方面 | v1 | v2 |
|------|-----|-----|
| 阶段定义 | `PipelinePhase` enum 硬编码 | config.toml `[stages].order` 字符串列表 |
| Plugin trait | 6 个必须方法 | 2 必须（metadata + execute）+ 7 可选默认实现 |
| Context | 12 个固定字段（含 api_key、model 等） | 6 个协议字段 + custom（其余全部走扩展层） |
| 加新阶段 | 改 enum → 重编译内核 + 所有插件 | 改 config.toml 即可 |
| 加新 trait 方法 | 所有插件重编译 | 旧插件用默认实现，不受影响 |
| 加新配置字段 | 改 Context 结构体 → 所有插件重编译 | 写入 custom，零影响 |
| 多会话支持 | 不支持 | session_id 预留，按 session 隔离 |

---

## 注意事项

1. **Context 协议层是最后防线**：加新字段前务必确认是否可以通过 `custom` 解决。只有流水线执行本身需要的数据（如 `abort`、`phase`）才值得放在协议层
2. **trait 可选方法只能追加，不能删除或改名**：删除就是破坏性变更，所有插件需重编译
3. **Windows DLL 锁定**：使用临时文件 + 重命名方案规避文件锁
4. **生产环境关闭热加载**：`hot_reload.enabled = false`，消除文件监听开销
5. **API Key**：优先环境变量，不提交 config.toml
6. **插件不加 `core` 以外的编译时依赖**：否则热加载替换插件时需要同步替换依赖库

---

## 扩展指南

### 添加新插件

1. `plugins/` 下建目录 + `Cargo.toml`（`crate-type = ["cdylib"]`，仅依赖 `core`）
2. 实现 `Plugin` trait（至少 `metadata()` + `execute()`），导出 `create_plugin`
3. 在 workspace 中注册
4. 在 `config.toml` 的 `[stages].order` 对应阶段下自动被发现
5. 编译插件即可，内核无需改动

### 添加新阶段

1. 在 `config.toml` 的 `[stages].order` 中添加阶段名 ✅
2. 不需要改任何代码 ✅

### 添加新 trait 可选能力

1. 在 `Plugin` trait 中添加带默认实现的新方法 ✅
2. 旧插件自动用默认实现，不影响 ✅
3. 新插件可选择重写 ✅

### 修改 Context 扩展层数据

- 只需插件间约定新的 `custom` key，无需改 Context 结构体 ✅

### 修改 Context 协议层

- 需要在极少情况下才做，影响所有插件，需整体重编译 ⚠️
