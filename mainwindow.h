#pragma once

// ====================================================================
// mainwindow.h — 角色扮演 AI 的主窗口（纯 UI 层）
//
// 职责：
//   - 管理两个 QTabWidget 标签页（角色卡 / 对话）
//   - 所有 Widget 创建、布局、样式
//   - 通过 Backend 引用读写数据、触发业务操作
//   - 响应 Backend 信号更新界面
//
// 数据与业务逻辑全部在 backend.h/.cpp 中，MainWindow 不持有任何
// ai_content / question_st / QNetworkAccessManager。
// ====================================================================

#include <QMainWindow>
#include <QTabWidget>
#include <QLineEdit>
#include <QTextEdit>
#include <QTextBrowser>
#include <QPushButton>
#include <QComboBox>

class Backend;

class MainWindow : public QMainWindow {
    Q_OBJECT  // 启用 Qt 信号槽机制

public:
    explicit MainWindow(QWidget *parent = nullptr);
    ~MainWindow() override = default;

private slots:
    // ------ 对话 ------
    void on_send();                     // 发送消息（委托 Backend）

    // ------ 文件操作 ------
    void on_save();                     // 保存配置到文件（委托 Backend）
    void on_load();                     // 从文件加载配置（委托 Backend）

    // ------ 工具 ------
    void on_prompt();                   // 显示生成的 system prompt
    void on_settings();                 // 打开 API 设置对话框
    void on_about();                    // 关于对话框

    // ------ 状态栏 ------
    void update_status_bar();           // 更新状态栏显示

    // ------ 聊天历史 ------
    void on_clear_history();            // 清空对话历史（委托 Backend）

    // ------ Backend 信号响应 ------
    void on_backend_response(const QString &content);
    void on_backend_error(const QString &title, const QString &message);
    void on_backend_busy_changed(bool busy);

private:
    // ------ 界面构建 ------
    void setup_ui();                    // 初始化主窗口布局
    void setup_menu();                  // 初始化菜单栏
    void setup_stylesheet();            // 全局样式表
    void setup_backend_connections();   // 连接 Backend 信号
    QWidget* create_character_tab();    // 创建"角色卡"标签页
    QWidget* create_chat_tab();         // 创建"对话"标签页

    // ------ 数据同步（UI ↔ Backend 桥梁） ------
    void sync_fields_to_struct();       // 将控件值写入 Backend::content()
    void sync_struct_to_fields();       // 将 Backend::content() 回写到控件
    void load_history_to_chat();        // 将对话历史显示到聊天面板

    // ------ 对话显示 ------
    void append_chat(const QString &role, const QString &text);
    // 向对话浏览器追加一条气泡风格的消息


    // ========== 控件指针 ==========

    // --- 角色卡标签页 ---
    QLineEdit   *m_name = nullptr;                // 角色名称
    QLineEdit   *m_personality = nullptr;         // 性格
    QTextEdit   *m_background = nullptr;          // 背景故事
    QLineEdit   *m_speaking_style = nullptr;      // 说话风格
    QLineEdit   *m_goals = nullptr;               // 目标
    QLineEdit   *m_scene = nullptr;               // 当前场景
    QLineEdit   *m_time = nullptr;                // 当前时间
    QTextEdit   *m_memory = nullptr;              // 长期记忆
    QTextEdit   *m_example_dialogues = nullptr;   // 示例对话（Few-shot）
    QTextEdit   *m_extra_commend = nullptr;       // 额外指令
    QTextEdit   *m_frankenstein_state = nullptr;  // 环境状态追踪

    // --- 对话标签页 ---
    QTextBrowser *m_chat_display = nullptr;       // 对话内容显示区
    QLineEdit    *m_chat_input = nullptr;         // 消息输入框
    QPushButton  *m_send_btn = nullptr;           // 发送按钮

    // ========== 后端 ==========

    Backend *m_backend = nullptr;       // 纯业务逻辑（数据 + API + 持久化）
};
