#pragma once

// ====================================================================
// mainwindow.h — 角色扮演 AI 的主窗口声明
//
// 职责：
//   - 管理两个 QTabWidget 标签页（角色卡 / 对话）
//   - 维护 ai_content 和 question_st 数据
//   - 通过 QNetworkAccessManager 调用 DeepSeek API
// ====================================================================

#include <QMainWindow>
#include <QTabWidget>
#include <QLineEdit>
#include <QTextEdit>
#include <QTextBrowser>
#include <QPushButton>
#include <QComboBox>
#include <QNetworkAccessManager>
#include <QNetworkReply>

#include "ai_content_creator.h"
#include "ai_reciver.h"

class MainWindow : public QMainWindow {
    Q_OBJECT  // 启用 Qt 信号槽机制

public:
    explicit MainWindow(QWidget *parent = nullptr);
    ~MainWindow() override = default;

private slots:
    // ------ 对话 ------
    void on_send();                     // 发送消息给 API

    // ------ 文件操作 ------
    void on_save();                     // 保存配置到文件
    void on_load();                     // 从文件加载配置

    // ------ 工具 ------
    void on_prompt();                   // 显示生成的 system prompt
    void on_settings();                 // 打开 API 设置对话框
    void on_about();                    // 关于对话框

    // ------ 状态栏 ------
    void update_status_bar();           // 更新状态栏显示

    // ------ 聊天历史 ------
    void on_clear_history();            // 清空对话历史

private:
    // ------ 界面构建 ------
    void setup_ui();                    // 初始化主窗口布局
    void setup_menu();                  // 初始化菜单栏
    QWidget* create_character_tab();    // 创建"角色卡"标签页
    QWidget* create_chat_tab();         // 创建"对话"标签页

    // ------ 数据同步 ------
    void sync_fields_to_struct();       // 将控件值写入 ai_content
    void sync_struct_to_fields();       // 将 ai_content 回写到控件
    void load_history_to_chat();        // 将 hitory_communication 显示到对话面板

    // ------ 对话显示 ------
    void append_chat(const QString &role, const QString &text);
    // 向对话浏览器追加一条气泡风格的消息


    // ========== 控件指针 ==========

    // --- 角色卡标签页 ---
    QLineEdit   *m_name;                // 角色名称
    QLineEdit   *m_personality;         // 性格
    QTextEdit   *m_background;          // 背景故事
    QLineEdit   *m_speaking_style;      // 说话风格
    QLineEdit   *m_goals;               // 目标
    QLineEdit   *m_scene;               // 当前场景
    QLineEdit   *m_time;                // 当前时间
    QTextEdit   *m_memory;              // 长期记忆
    QTextEdit   *m_example_dialogues;   // 示例对话（Few-shot）
    QTextEdit   *m_extra_commend;       // 额外指令
    QTextEdit   *m_frankenstein_state;  // 环境状态追踪

    // --- 对话标签页 ---
    QTextBrowser *m_chat_display;       // 对话内容显示区
    QLineEdit    *m_chat_input;         // 消息输入框
    QPushButton  *m_send_btn;           // 发送按钮

    // ========== 数据 ==========

    ai_content  m_content;              // 角色卡 + 场景 + 记忆
    question_st m_question;             // API 请求参数

    QNetworkAccessManager *m_network;   // HTTP 客户端（异步非阻塞）

    bool m_is_sending = false;          // 发送中标志，防重复提交
};
