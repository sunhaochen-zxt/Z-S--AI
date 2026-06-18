# CLAUDE.md

本文件为 Claude Code（claude.ai/code）在此仓库中工作时提供指导。

## 构建与运行

```sh
mkdir build && cd build
cmake .. && make -j$(nproc)
./role
```

依赖：Qt6（Quick + Network）、CMake ≥ 3.16、GCC ≥ 8（C++17）。构建产物已存在于 `build/` 目录中。

`CMakeLists.txt` 中记录了跨平台编译目标：
- **Windows (MinGW)**：`mingw64-cmake -DCMAKE_BUILD_TYPE=Release .. && mingw64-make -j$(nproc)`
- **Android**：`qt-cmake .. && make -j$(nproc)`（需要 Qt for Android + NDK）

## 架构

这是一个基于 **Qt6 Quick/QML** 的桌面应用，通过 DeepSeek Chat Completions API 实现 AI 角色扮演对话（同时支持 OpenAI 兼容模式）。界面使用 Material Design 3 主题。

### 分层结构

```
main.qml（QML UI — Material Design，两个标签页）
    ↕ Q_PROPERTY / Q_INVOKABLE / 信号
backend.h/.cpp（C++ QObject — 全部业务逻辑，零 UI 代码）
    ↕ 调用
ai_content_creator.h（从 JSON 构建 system prompt）
ai_reciver.h         （API 请求/响应的数据结构）
load_History&config.h（配置文件读写）
character_card.h      （SillyTavern PNG 角色卡导入导出）
```

- **`main.cpp`** — 创建 `QGuiApplication`，强制 Material 风格，将 `Backend` 实例注册为 QML 上下文属性，加载 `main.qml`。
- **`backend.h/.cpp`** — C++ 侧的全部逻辑。持有 `ai_content` + `question_st`，通过 `QNetworkAccessManager` 管理异步 HTTP 请求，暴露属性/信号/槽供 QML 绑定。不使用任何 QWidget/QDialog/QLayout。
- **`main.qml`** — 单文件 UI：两个标签页（角色卡编辑器 + 对话）、设置对话框、导入/导出对话框、系统提示词查看器。使用 `M3Colors.qml` 中的 Material Design 3 调色板。

### 数据流

1. 用户导入 SillyTavern PNG 角色卡（或在角色卡标签页中手动填写字段）
2. 用户在对话标签页输入消息并按 Enter
3. QML 调用 `backend.sendMessage(text)`
4. `Backend::sendMessage()` 将用户消息记录到 `history_communication`，调用 `content_creat()` 从 `character_card_json` 构建 system prompt，构造 JSON 请求体，POST 到 `{base_url}/chat/completions`
5. 解析响应，将助手回复追加到 `history_communication`，自动保存到 `role.conf`，通过 `responseReady()`（非流式）或 `streamPartial()`（流式 / SSE 解析）通知 QML

### 核心数据结构

- **`ai_content`**（`ai_content_creator.h`）— 持有 `character_card_json`（SillyTavern v3 JSON 字符串）和 `history_communication`（原始对话历史字符串）。旧的独立字段（name、personality、background 等）已移除，所有角色数据均从 JSON 中提取。
- **`question_st`**（`ai_reciver.h`）— API 参数：`api_type`（"deepseek" 或 "openai"）、`api_key`、`base_url`、`model`、`stream`、`reasoning_effort`、`extra_body.thinking_type`、`message`（`message_st` 向量）。
- **`content_creat()`** — 纯函数，从角色卡 JSON 中提取字段，组装为 `[System]\n[Character]\n[Personality]\n...` 格式的 prompt 字符串。

### 配置文件格式（`load_History&config.h`）

自定义二进制安全文本格式（`role.conf`）。键值对以换行分隔；值以 `\x01`...`\x02` 定界。`\x03` 为转义字符。分为 `[ai_content]` 和 `[question_st]` 两个区段。由 `save_config()` / `load_config()` 读写。

**安全提醒**：API Key 以明文存储在 `role.conf` 中。建议优先使用环境变量 `DEEPSEEK_API_KEY`。`role.conf` 已加入 `.gitignore`。

### 角色卡导入导出（`character_card.h`）

纯 C++ PNG chunk 解析器（无 libpng 依赖）。支持 SillyTavern v2（`chara` tEXt 键）和 v3（`ccv3` tEXt 键）格式。导入：base64 解码 → JSON → `ai_content.character_card_json`。导出：将 JSON 以 base64 嵌入最小 1x1 PNG 的 tEXt chunk。

### GCC 依赖

`ai_reciver.h` 包含了 `<bits/stdc++.h>`，该头文件仅在 GCC 下可用。除非同时放弃 GCC 编译，否则不要将其替换为标准头文件。

### 已弃用模型

`deepseek-chat` 和 `deepseek-reasoner` 已弃用，2026-07-24 后将不可用。应用默认使用 `deepseek-v4-flash`。

## 项目路线图

详见 `FUTURE.md`。推荐的优先级顺序：
1. OpenAI 兼容 API ✅（已完成）
2. 流式响应 ✅（已完成）
3. SillyTavern 角色卡兼容 ✅（已完成）
4. Token 计数与上下文管理
5. 对话历史管理
6. 世界书 / Lorebook
