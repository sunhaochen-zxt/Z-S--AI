# Z&S-AI

**一个基于 Qt6 + DeepSeek API 的角色扮演对话桌面工具。**
 > **此项目仅图一乐，安全性稳定性难以保证，请勿用于工作生产。** 
 > ⚠️ **当前版本将 API Key 以明文形式存储在 `role.conf` 文件中。**

Z&S-AI 提供了一个可视化的角色卡编辑器和聊天界面，让你可以创建自定义的 AI 角色，并与它们进行沉浸式的角色扮演对话。

![C++17](https://img.shields.io/badge/C%2B%2B-17-00599C?style=flat&logo=cplusplus)
![Qt6](https://img.shields.io/badge/Qt-6-41CD52?style=flat&logo=qt)
![CMake](https://img.shields.io/badge/CMake-%E2%89%A53.16-064F8C?style=flat&logo=cmake)
![License](https://img.shields.io/github/license/sunhaochen-zxt/Z-S--AI?style=flat&logo=gnu)

---

##  功能

- **角色卡编辑器** — 在图形界面中填写角色名称、性格、背景故事、说话风格、目标等信息
- **场景与时间设定** — 自定义当前场景（如古城废墟、星际飞船）和时间（黄昏、公元3024年）
- **记忆与状态追踪** — 管理角色的长期记忆、好感度、位置等可追踪变量
- **示例对话（Few-shot）** — 通过示例对话引导 AI 角色按照你期望的风格回复
- **额外指令** — 添加高阶叙事规则，精细控制 AI 行为
- **对话历史** — 气泡式聊天界面，支持多轮对话上下文
- **配置持久化** — 自定义二进制格式保存/加载角色卡和 API 设置
- **API 设置面板** — 可视化配置 API Key、模型、Base URL 等参数
- **系统提示词预览** — 随时查看生成的完整 system prompt，方便调试

##  快速开始

### 依赖

| 依赖 | 版本要求 |
|---|---|
| GCC | ≥ 8（需要 `<bits/stdc++.h>` 支持） |
| CMake | ≥ 3.16 |
| Qt6 | Widgets + Network 模块 |
| Linux / macOS / Windows | 理论跨平台，主要在 Linux 下开发 |

### 构建

```bash
git clone <repo-url>
cd Z&S-AI
mkdir build && cd build
cmake ..
make -j$(nproc)
```

### 运行

```bash
# 先设置 API Key（推荐）
export DEEPSEEK_API_KEY="sk-xxxxxxxxxxxxxxxx"

./role
```

也可以在程序内通过 **设置 → API 设置** 手动填入 Key。

### 构建产物

生成的可执行文件名为 `role`，默认配置文件为同目录下的 `role.conf`。

##  使用指南

### 第一步：配置 API

通过 **设置 → API 设置** 填写以下信息：

| 参数 | 说明 | 默认值 |
|---|---|---|
| API Key | DeepSeek API 密钥 | 环境变量 `DEEPSEEK_API_KEY` |
| Base URL | API 端点地址 | `https://api.deepseek.com` |
| 模型 | 使用的模型 | `deepseek-v4-flash` |
| Reasoning Effort | 推理强度 | `medium` |
| Thinking Type | 思维链类型 | 空（可选 `enabled`） |

> ⚠️ `deepseek-chat` 和 `deepseek-reasoner` 已弃用，2026-07-24 后将不可用。

### 第二步：编辑角色卡

在 **角色卡** 标签页中填写角色信息：

- **角色属性** — 名称、性格、说话风格、目标
- **背景与场景** — 背景故事、当前场景、当前时间
- **记忆与状态** — 长期记忆、状态追踪（好感度/位置/变量）、额外指令
- **示例对话** — Few-shot 示例，格式如 `User: ...` / `Assistant: ...`

### 第三步：开始对话

切换到 **对话** 标签页，输入消息，按 Enter 或点击「发送」即可。

对话会自动保存到 `role.conf`，下次启动时恢复。

##  系统提示词结构

程序会自动将角色卡信息组装成结构化的 system prompt：

```
[System]          ← 核心行为规则
[Character]       ← 角色名称
[Personality]     ← 性格描述
[Background]      ← 背景故事
[Speaking Style]  ← 说话风格
[Goals]           ← 角色目标
[Current Scene]   ← 当前场景
[Current Time]    ← 当前时间
[Memory]          ← 长期记忆
[Conversation History]  ← 对话历史
[Frankenstein State]    ← 状态追踪（可选）
[Example Dialogues]     ← 示例对话（可选）
[Additional Instructions]  ← 额外指令（可选）
```

可在 **工具 → 查看系统提示词** 中预览最终生成的 prompt。

## 项目架构

```
├── main.cpp                  # 应用程序入口
├── mainwindow.h / .cpp       # 主窗口：UI 构建、事件处理、API 调用
├── ai_content_creator.h      # 提示词构建器（纯函数）
├── ai_reciver.h              # API 请求/响应数据结构
├── load_History&config.h     # 配置文件的读写
├── CMakeLists.txt            # CMake 构建配置
└── API使用方法.md             # DeepSeek API 使用说明
```

### 数据流

```
角色卡控件 → ai_content 结构体
                  ↓
         content_creat() → system prompt
                  ↓
    JSON 请求 → DeepSeek API → JSON 响应
                  ↓
        对话气泡显示 + auto-save
```

##  配置文件格式

配置文件使用自定义二进制安全文本格式（`role.conf`）：
- 键值对以换行分隔，值用 `\x01` … `\x02` 定界
- `\x03` 为转义字符
- 分为 `[ai_content]` 和 `[question_st]` 两个区段

##  安全警告：API Key 明文存储

> ⚠️ **当前版本将 API Key 以明文形式存储在 `role.conf` 文件中。**

每次对话后程序会自动保存配置（包括 API Key），`role.conf` 文件中 `api_key` 字段的值是未经加密的原始字符串。这意味着：

- 任何能读取该文件的人都能拿到你的 API Key
- 如果将 `role.conf` 提交到 Git 仓库，API Key 会直接暴露
- 如果使用云同步（如网盘）备份该文件，API Key 也会被同步上去

**建议措施：**

1. 已将 `role.conf` 加入 `.gitignore`（如未添加请手动添加）
2. 优先通过环境变量 `DEEPSEEK_API_KEY` 设置 Key，而非在 GUI 中填写
3. 定期检查 `role.conf` 是否被意外泄露
4. 后续计划引入加密存储或系统密钥链支持

##  许可

本项目基于 GPL 协议开源，详见 [LICENSE](LICENSE)。

