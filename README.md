# Z&S-AI

**微内核+插件化 AI 角色扮演桌面应用。**

> v2 重构版：Rust 后端 + Electron 前端，放弃旧版 Qt6/QML 实现。

![Rust](https://img.shields.io/badge/Rust-1.96-000000?style=flat&logo=rust)
![Electron](https://img.shields.io/badge/Electron-42-47848F?style=flat&logo=electron)
![React](https://img.shields.io/badge/React-19-61DAFB?style=flat&logo=react)
![License](https://img.shields.io/badge/License-Non--Commercial-blue?style=flat)

---

## 架构

```
┌──────────────────────────────────┐
│        Electron + React          │  前端 (TypeScript + M3)
│   对话页 · 角色卡编辑器 · 设置     │
└──────────┬───────────────────────┘
           │ HTTP + WebSocket
┌──────────┴───────────────────────┐
│         Server (axum)            │  服务端 (Rust)
│  14 API 端点 · Session 管理       │
└──────────┬───────────────────────┘
           │
┌──────────┴───────────────────────┐
│        Core (zsai-core)          │  微内核
│  Context · Plugin · Pipeline     │
│  DynamicLoader · HotReload       │
└──────────┬───────────────────────┘
           │
┌──────────┴───────────────────────┐
│      Plugins (7 × .so)           │  插件 (动态库)
│  character_card · prompt_builder │
│  api_client · stream_parser      │
│  history · token_counter · stub  │
└──────────────────────────────────┘
```

## 快速开始

### 依赖

| 依赖 | 版本 |
|------|------|
| Rust | ≥ 1.85 |
| Node.js | ≥ 20 |
| Linux | x86_64 (macOS/Windows 理论支持) |

### 构建 & 运行

```bash
git clone https://github.com/sunhaochen-zxt/Z-S--AI.git
cd Z-S--AI

# 1. 编译后端
cargo build --workspace

# 2. 设置 API Key
export DEEPSEEK_API_KEY="sk-xxxxxxxx"

# 3. 启动后端
./target/debug/server

# 4. 启动前端（另一个终端）
cd electron-app
npm install
npx tsc -p tsconfig.main.json
npx vite build
npx electron dist-electron/main/index.js
```

### 一键启动

```bash
# 后端
cargo build --workspace && ./target/debug/server

# 前端（开发模式）
cd electron-app && npx vite build && npx electron dist-electron/main/index.js
```

## 配置

编辑 `config.example.toml`（复制为 `config.toml`）：

```toml
[api]
api_type = "deepseek"       # deepseek | openai
api_key = ""                # 留空则读 $DEEPSEEK_API_KEY
base_url = "https://api.deepseek.com"
model = "deepseek-v4-flash"
stream = false

[stages]
order = ["preprocess", "validate", "build_prompt", "api_call", "postprocess"]
error_stage = "postprocess"
```

## 插件

插件编译为独立 `.so` 动态库，支持运行时热加载。

| 插件 | 阶段 | 职责 |
|------|------|------|
| character_card | preprocess | SillyTavern v3 JSON 角色卡加载 |
| prompt_builder | build_prompt | System Prompt 组装 |
| api_client | api_call | DeepSeek / OpenAI API 调用 + SSE 流式 |
| stream_parser | api_call | SSE 事件流解析 |
| history | postprocess | 对话历史 JSON 读写 + 上下文裁剪 |
| token_counter | postprocess | Token 用量估算 + 超限警告 |
| test_stub | postprocess | 插件系统验证桩 |

### 开发新插件

```bash
# 1. 创建目录
mkdir plugins/my_plugin/src

# 2. 编写 Cargo.toml
cat > plugins/my_plugin/Cargo.toml << EOF
[package]
name = "plugin-my-plugin"
version.workspace = true
edition.workspace = true
[lib]
crate-type = ["cdylib"]
[dependencies]
zsai-core = { path = "../../core" }
EOF

# 3. 实现 Plugin trait，导出 create_plugin
# 4. 注册到 Cargo.toml workspace members
# 5. cargo build --workspace
```

## API

| 端点 | 方法 | 说明 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/api/chat` | POST | 非流式对话 |
| `/ws/chat` | WebSocket | 流式对话 |
| `/api/session` | POST/DELETE | 会话管理 |
| `/api/card` | GET/PUT | 角色卡读写 |
| `/api/card/import` | POST | 导入角色卡 |
| `/api/config` | GET/PUT | 配置管理 |
| `/api/history` | GET/DELETE | 历史管理 |
| `/api/history/export` | GET | 导出历史 (JSON/Markdown) |
| `/api/prompt/preview` | POST | 预览 System Prompt |

## 项目结构

```
Z-S--AI/
├── core/                  zsai-core 微内核
├── plugins/               7 个插件
│   ├── character_card/
│   ├── prompt_builder/
│   ├── api_client/
│   ├── stream_parser/
│   ├── history/
│   ├── token_counter/
│   └── test_stub/
├── server/                HTTP/WS 服务端
├── electron-app/          Electron 前端
├── config.example.toml    配置模板
└── data/characters/       示例角色卡
```

## 许可

Non-Commercial Free Use License 1.0 © Z&S-AI

---

> ⚠️ 此项目仅图一乐，安全性稳定性难以保证，请勿用于工作生产。
> API Key 通过环境变量或 GUI 设置，config.toml 不要提交到 Git。
