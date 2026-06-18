

## 核心架构原则

1. **内核（core）**：
   - 只做三件事：读配置、按顺序执行插件、传递Context
   - 不实现任何业务逻辑
   - 所有业务功能都在插件里

2. **插件（plugins/）**：
   - 每个插件独立目录，独立Cargo.toml
   - 实现 Plugin trait
   - 插件之间不直接调用，通过Context通信
   - GUI也是插件（可以有egui、tauri、cli等多种实现）

3. **Context是唯一的数据传递方式**
   - 包含：user_input、ai_response、messages、character、memories、custom等
   - custom字段是HashMap<String, Value>，插件可以存任何数据

4. **配置文件驱动**
   - config.toml 决定启用哪些插件、执行顺序、插件参数
   - 改配置就能改变程序行为，不需要改代码

