// ====================================================================
// mainwindow.cpp — Z&S-AI 主窗口（纯 UI 层）
//
// 文件结构（自上而下）：
//   1. HTML 转义工具函数
//   2. 构造函数
//   3. 样式表
//   4. 界面构建（setup_ui / setup_menu / create_*_tab）
//   5. 数据同步（控件 ↔ Backend 桥梁）
//   6. 对话显示（append_chat / load_history_to_chat）
//   7. 槽函数（全部委托 Backend 处理业务逻辑）
//   8. Backend 信号响应
//
// 所有的数据持有、API 调用、JSON 构建、文件读写均在 backend.cpp 中。
// ====================================================================

#include "mainwindow.h"
#include "backend.h"

#include <QScrollArea>
#include <QFormLayout>
#include <QVBoxLayout>
#include <QHBoxLayout>
#include <QMenuBar>
#include <QMessageBox>
#include <QFileDialog>
#include <QDialog>
#include <QDialogButtonBox>
#include <QLabel>
#include <QGroupBox>
#include <QStatusBar>
#include <QApplication>

// ====================================================================
// HTML 转义：防止用户输入中的 < > & " 破坏 HTML 标签结构
// Qt6 移除了 QString::toHtmlEscaped()，故手动实现
// ====================================================================
static QString html_escape(const QString &text) {
    QString out;
    out.reserve(text.size());
    for (const QChar &c : text) {
        if      (c == '<')  out += "&lt;";
        else if (c == '>')  out += "&gt;";
        else if (c == '&')  out += "&amp;";
        else if (c == '"')  out += "&quot;";
        else                out += c;
    }
    return out;
}

// ====================================================================
// 构造函数
// ====================================================================
MainWindow::MainWindow(QWidget *parent)
    : QMainWindow(parent)
    , m_backend(new Backend(this))
{
    setWindowTitle("Z&S-AI");
    resize(1100, 720);

    // 全局样式表
    setup_stylesheet();

    // 连接 Backend 信号
    setup_backend_connections();

    // 底部状态栏
    update_status_bar();

    // 界面构建
    setup_ui();
    setup_menu();

    // 将加载的对话历史回显到聊天面板
    load_history_to_chat();
}

// ====================================================================
// 全局样式表
// ====================================================================
void MainWindow::setup_stylesheet() {
    setStyleSheet(R"(
        /* ---- 全局 ---- */
        QMainWindow {
            background-color: #f6f8fa;
        }
        QWidget {
            font-family: "Segoe UI", "Noto Sans SC", "Microsoft YaHei", "Helvetica", sans-serif;
            font-size: 13px;
            color: #1e1e1e;
        }

        /* ---- 菜单栏 ---- */
        QMenuBar {
            background-color: #2d2d30;
            color: #cccccc;
            border-bottom: 1px solid #3e3e42;
            padding: 2px 8px;
        }
        QMenuBar::item {
            padding: 6px 12px;
            border-radius: 4px;
            background: transparent;
        }
        QMenuBar::item:selected {
            background-color: #3e3e42;
        }
        QMenu {
            background-color: #2d2d30;
            border: 1px solid #3e3e42;
            border-radius: 6px;
            padding: 4px;
            color: #cccccc;
        }
        QMenu::item {
            padding: 6px 32px 6px 16px;
            border-radius: 4px;
        }
        QMenu::item:selected {
            background-color: #094771;
            color: white;
        }
        QMenu::separator {
            height: 1px;
            background: #3e3e42;
            margin: 4px 8px;
        }

        /* ---- 选项卡 ---- */
        QTabWidget::pane {
            border: none;
            background-color: #ffffff;
        }
        QTabBar::tab {
            background: #f6f8fa;
            border: none;
            border-bottom: 2px solid transparent;
            padding: 10px 28px;
            color: #666;
            font-size: 13px;
        }
        QTabBar::tab:hover {
            color: #1e1e1e;
            background: #eaeef2;
        }
        QTabBar::tab:selected {
            color: #1e1e1e;
            font-weight: 600;
            border-bottom: 2px solid #0969da;
            background: #ffffff;
        }

        /* ---- 分组框（卡片风格） ---- */
        QGroupBox {
            font-weight: 600;
            border: none;
            border-radius: 8px;
            margin-top: 20px;
            padding: 20px 16px 14px 16px;
            background-color: #ffffff;
        }
        QGroupBox::title {
            subcontrol-origin: margin;
            subcontrol-position: top left;
            padding: 4px 0px;
            margin-left: 16px;
            color: #1e1e1e;
            font-size: 13px;
            letter-spacing: 0.5px;
        }

        /* ---- 输入控件 ---- */
        QLineEdit {
            border: 1px solid #d0d7de;
            border-radius: 6px;
            padding: 7px 12px;
            background-color: #f6f8fa;
            font-size: 13px;
            color: #1e1e1e;
            selection-background-color: #0969da;
            selection-color: white;
        }
        QLineEdit:focus {
            border-color: #0969da;
            background-color: #ffffff;
            outline: none;
        }
        QTextEdit {
            border: 1px solid #d0d7de;
            border-radius: 6px;
            padding: 8px 12px;
            background-color: #f6f8fa;
            font-size: 13px;
            color: #1e1e1e;
            selection-background-color: #0969da;
            selection-color: white;
        }
        QTextEdit:focus {
            border-color: #0969da;
            background-color: #ffffff;
            outline: none;
        }
        QComboBox {
            border: 1px solid #d0d7de;
            border-radius: 6px;
            padding: 6px 12px;
            background-color: #f6f8fa;
            font-size: 13px;
        }
        QComboBox:focus {
            border-color: #0969da;
        }
        QComboBox::drop-down {
            subcontrol-origin: padding;
            subcontrol-position: top right;
            width: 24px;
            border-left: 1px solid #d0d7de;
            border-top-right-radius: 6px;
            border-bottom-right-radius: 6px;
        }
        QComboBox QAbstractItemView {
            background-color: #ffffff;
            border: 1px solid #d0d7de;
            border-radius: 6px;
            selection-background-color: #0969da;
            selection-color: white;
            padding: 4px;
        }

        /* ---- 按钮 ---- */
        QPushButton {
            background-color: #f6f8fa;
            color: #24292f;
            border: 1px solid #d0d7de;
            border-radius: 6px;
            padding: 7px 18px;
            font-size: 13px;
            font-weight: 500;
        }
        QPushButton:hover {
            background-color: #f3f4f6;
            border-color: #c0c7cf;
        }
        QPushButton:pressed {
            background-color: #eaeef2;
        }
        QPushButton:disabled {
            background-color: #f6f8fa;
            color: #8c959f;
            border-color: #d0d7de;
        }

        /* ---- 主按钮（蓝底） ---- */
        QPushButton#sendBtn {
            background-color: #0969da;
            color: white;
            border: 1px solid #0969da;
            font-weight: 600;
        }
        QPushButton#sendBtn:hover {
            background-color: #0860ca;
            border-color: #0860ca;
        }
        QPushButton#sendBtn:pressed {
            background-color: #0755b8;
        }
        QPushButton#sendBtn:disabled {
            background-color: #94b8e8;
            border-color: #94b8e8;
            color: #ffffffcc;
        }

        /* ---- 对话框内按钮行 ---- */
        QDialogButtonBox QPushButton {
            min-width: 80px;
        }

        /* ---- 对话显示区 ---- */
        QTextBrowser {
            border: none;
            background-color: #f0f2f5;
            padding: 8px;
            font-size: 13px;
        }

        /* ---- 滚动区域 ---- */
        QScrollArea {
            border: none;
            background-color: transparent;
        }
        QScrollBar:vertical {
            background: transparent;
            width: 8px;
            margin: 0;
        }
        QScrollBar::handle:vertical {
            background: #c0c7cf;
            border-radius: 4px;
            min-height: 30px;
        }
        QScrollBar::handle:vertical:hover {
            background: #8c959f;
        }
        QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {
            height: 0px;
        }
        QScrollBar:horizontal {
            background: transparent;
            height: 8px;
        }
        QScrollBar::handle:horizontal {
            background: #c0c7cf;
            border-radius: 4px;
            min-width: 30px;
        }
        QScrollBar::handle:horizontal:hover {
            background: #8c959f;
        }
        QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {
            width: 0px;
        }

        /* ---- 状态栏 ---- */
        QStatusBar {
            background-color: #2d2d30;
            color: #cccccc;
            border-top: 1px solid #3e3e42;
            font-size: 12px;
        }
        QStatusBar::item {
            border: none;
        }

        /* ---- 提示 & 标签 ---- */
        QLabel {
            color: #1e1e1e;
        }

        /* ---- 分隔线 ---- */
        QFrame[frameShape="4"] {
            color: #d0d7de;
        }
    )");
}

// ====================================================================
// 连接 Backend 信号 → UI 槽
// ====================================================================
void MainWindow::setup_backend_connections() {
    connect(m_backend, &Backend::responseReady,  this, &MainWindow::on_backend_response);
    connect(m_backend, &Backend::errorOccurred,  this, &MainWindow::on_backend_error);
    connect(m_backend, &Backend::busyChanged,    this, &MainWindow::on_backend_busy_changed);
    connect(m_backend, &Backend::dataChanged,    this, &MainWindow::update_status_bar);
}

// ====================================================================
// 构建主界面
// ====================================================================
void MainWindow::setup_ui() {
    // 中央控件：带内边距的 QTabWidget
    auto *wrapper = new QWidget(this);
    auto *wrapper_layout = new QVBoxLayout(wrapper);
    wrapper_layout->setContentsMargins(6, 6, 6, 6);

    QTabWidget *tabs = new QTabWidget(this);
    tabs->addTab(create_character_tab(), "📋  角色卡");
    tabs->addTab(create_chat_tab(),      "💬  对话");
    wrapper_layout->addWidget(tabs);
    setCentralWidget(wrapper);
}

// ====================================================================
// 构建菜单栏
// ====================================================================
void MainWindow::setup_menu() {
    // ---- 文件菜单 ----
    QMenu *file_menu = menuBar()->addMenu("文件(&F)");
    file_menu->addAction("保存配置(&S)",  QKeySequence::Save, this, &MainWindow::on_save);
    file_menu->addAction("加载配置(&L)",  QKeySequence::Open, this, &MainWindow::on_load);
    file_menu->addSeparator();
    file_menu->addAction("退出(&Q)",      QKeySequence::Quit, qApp, &QApplication::quit);

    // ---- 设置菜单 ----
    QMenu *settings_menu = menuBar()->addMenu("设置(&S)");
    settings_menu->addAction("API 设置...", this, &MainWindow::on_settings);

    // ---- 工具菜单 ----
    QMenu *tools_menu = menuBar()->addMenu("工具(&T)");
    tools_menu->addAction("查看系统提示词(&P)", this, &MainWindow::on_prompt);
    tools_menu->addAction("清空对话历史",        this, &MainWindow::on_clear_history);

    // ---- 帮助菜单 ----
    QMenu *help_menu = menuBar()->addMenu("帮助(&H)");
    help_menu->addAction("关于(&A)", this, &MainWindow::on_about);
}

// ====================================================================
// 创建"角色卡"标签页
// 字段按语义分组放入 QGroupBox，整体放在 QScrollArea 内
// ====================================================================
QWidget* MainWindow::create_character_tab() {
    auto *scroll = new QScrollArea;
    scroll->setWidgetResizable(true);

    auto *panel = new QWidget;
    auto *main_layout = new QVBoxLayout(panel);
    main_layout->setSpacing(4);

    // ---- 分组1：角色属性 ----
    auto *g1 = new QGroupBox(QStringLiteral("\u2003\u8E92\u8272\u5C5E\u6027"));
    auto *f1 = new QFormLayout(g1);
    f1->setLabelAlignment(Qt::AlignRight);
    f1->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);

    m_name = new QLineEdit;
    m_name->setPlaceholderText("角色名称");
    f1->addRow("名称：", m_name);

    m_personality = new QLineEdit;
    m_personality->setPlaceholderText("如：活泼、温柔、冷酷");
    f1->addRow("性格：", m_personality);

    m_speaking_style = new QLineEdit;
    m_speaking_style->setPlaceholderText("如：喜欢用古风句式");
    f1->addRow("说话风格：", m_speaking_style);

    m_goals = new QLineEdit;
    m_goals->setPlaceholderText("如：帮助主角解开谜题");
    f1->addRow("目标：", m_goals);

    main_layout->addWidget(g1);

    // ---- 分组2：背景与场景 ----
    auto *g2 = new QGroupBox(QStringLiteral("\u2003\u80CC\u666F\u4E0E\u573A\u666F"));
    auto *f2 = new QFormLayout(g2);
    f2->setLabelAlignment(Qt::AlignRight);
    f2->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);

    m_background = new QTextEdit;
    m_background->setPlaceholderText("角色的背景故事……");
    m_background->setMaximumHeight(80);
    f2->addRow("背景：", m_background);

    m_scene = new QLineEdit;
    m_scene->setPlaceholderText("如：古城废墟、星际飞船");
    f2->addRow("场景：", m_scene);

    m_time = new QLineEdit;
    m_time->setPlaceholderText("如：黄昏、公元3024年");
    f2->addRow("时间：", m_time);

    main_layout->addWidget(g2);

    // ---- 分组3：记忆与状态 ----
    auto *g3 = new QGroupBox(QStringLiteral("\u2003\u8BB0\u5FC6\u4E0E\u72B6\u6001"));
    auto *f3 = new QFormLayout(g3);
    f3->setLabelAlignment(Qt::AlignRight);
    f3->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);

    m_memory = new QTextEdit;
    m_memory->setPlaceholderText("角色长期记忆 / 已掌握的信息……");
    m_memory->setMaximumHeight(80);
    f3->addRow("长期记忆：", m_memory);

    m_frankenstein_state = new QTextEdit;
    m_frankenstein_state->setPlaceholderText("好感度 / 位置 / 变量等可追踪状态");
    m_frankenstein_state->setMaximumHeight(80);
    f3->addRow("状态追踪：", m_frankenstein_state);

    m_extra_commend = new QTextEdit;
    m_extra_commend->setPlaceholderText("额外指令 / 高阶叙事规则……");
    m_extra_commend->setMaximumHeight(80);
    f3->addRow("额外指令：", m_extra_commend);

    main_layout->addWidget(g3);

    // ---- 分组4：示例对话 ----
    auto *g4 = new QGroupBox(QStringLiteral("\u2003\u793A\u4F8B\u5BF9\u8BDD"));
    auto *f4 = new QFormLayout(g4);
    f4->setLabelAlignment(Qt::AlignRight);
    f4->setFieldGrowthPolicy(QFormLayout::AllNonFixedFieldsGrow);

    m_example_dialogues = new QTextEdit;
    m_example_dialogues->setPlaceholderText(
        "Few-shot 示例对话，格式：User: ...\nAssistant: ...");
    m_example_dialogues->setMaximumHeight(100);
    f4->addRow("示例：", m_example_dialogues);

    main_layout->addWidget(g4);

    // ---- 操作按钮 ----
    auto *btn_layout = new QHBoxLayout;
    btn_layout->setContentsMargins(8, 12, 8, 8);

    auto *save_btn   = new QPushButton("保存配置");
    auto *load_btn   = new QPushButton("加载配置");
    auto *prompt_btn = new QPushButton("查看 Prompt");

    btn_layout->addWidget(save_btn);
    btn_layout->addWidget(load_btn);
    btn_layout->addWidget(prompt_btn);
    btn_layout->addStretch();
    main_layout->addLayout(btn_layout);
    main_layout->addStretch();

    // 将加载的数据回显到控件
    sync_struct_to_fields();

    // 连接按钮信号
    connect(save_btn,   &QPushButton::clicked, this, &MainWindow::on_save);
    connect(load_btn,   &QPushButton::clicked, this, &MainWindow::on_load);
    connect(prompt_btn, &QPushButton::clicked, this, &MainWindow::on_prompt);

    scroll->setWidget(panel);
    return scroll;
}

// ====================================================================
// 创建"对话"标签页
// 上方 QTextBrowser 显示历史消息，下方 QLineEdit + [发送] 按钮
// ====================================================================
QWidget* MainWindow::create_chat_tab() {
    QWidget *panel = new QWidget;
    QVBoxLayout *vbox = new QVBoxLayout(panel);
    vbox->setContentsMargins(4, 4, 4, 4);

    // 对话显示区（只读，支持 HTML）
    m_chat_display = new QTextBrowser;
    m_chat_display->setReadOnly(true);
    m_chat_display->setOpenExternalLinks(false);
    vbox->addWidget(m_chat_display, 1);  // stretch = 1，占据所有剩余空间

    // 底部输入行
    QHBoxLayout *hbox = new QHBoxLayout;
    m_chat_input = new QLineEdit;
    m_chat_input->setPlaceholderText("输入消息……（Enter 发送）");
    m_send_btn = new QPushButton("发送");
    m_send_btn->setObjectName("sendBtn");
    m_send_btn->setEnabled(false);  // 初始禁用，API Key 设置后启用
    m_send_btn->setFixedHeight(36);

    m_chat_input->setFixedHeight(36);

    hbox->addWidget(m_chat_input, 1);
    hbox->addWidget(m_send_btn);
    vbox->addLayout(hbox);

    // 信号连接
    connect(m_chat_input, &QLineEdit::returnPressed, this, &MainWindow::on_send);
    connect(m_send_btn,   &QPushButton::clicked,     this, &MainWindow::on_send);

    return panel;
}

// ====================================================================
// 将控件上的值写入 Backend::content()
// ====================================================================
void MainWindow::sync_fields_to_struct() {
    auto &c = m_backend->content();
    c.name               = m_name->text().toStdString();
    c.personality        = m_personality->text().toStdString();
    c.background         = m_background->toPlainText().toStdString();
    c.speaking_style     = m_speaking_style->text().toStdString();
    c.goals              = m_goals->text().toStdString();
    c.scene              = m_scene->text().toStdString();
    c.time               = m_time->text().toStdString();
    c.memory             = m_memory->toPlainText().toStdString();
    c.example_dialogues  = m_example_dialogues->toPlainText().toStdString();
    c.extra_commend      = m_extra_commend->toPlainText().toStdString();
    c.frankenstein_state = m_frankenstein_state->toPlainText().toStdString();
}

// ====================================================================
// 将 Backend::content() 的值写回到控件
// ====================================================================
void MainWindow::sync_struct_to_fields() {
    const auto &c = m_backend->content();
    m_name->setText(QString::fromStdString(c.name));
    m_personality->setText(QString::fromStdString(c.personality));
    m_background->setPlainText(QString::fromStdString(c.background));
    m_speaking_style->setText(QString::fromStdString(c.speaking_style));
    m_goals->setText(QString::fromStdString(c.goals));
    m_scene->setText(QString::fromStdString(c.scene));
    m_time->setText(QString::fromStdString(c.time));
    m_memory->setPlainText(QString::fromStdString(c.memory));
    m_example_dialogues->setPlainText(QString::fromStdString(c.example_dialogues));
    m_extra_commend->setPlainText(QString::fromStdString(c.extra_commend));
    m_frankenstein_state->setPlainText(QString::fromStdString(c.frankenstein_state));
}

// ====================================================================
// 将 Backend::content().history_communication 回显到聊天面板
// ====================================================================
void MainWindow::load_history_to_chat() {
    m_chat_display->clear();

    std::stringstream ss(m_backend->content().history_communication);
    std::string line;
    std::string pending_role;
    std::string pending_text;

    auto flush_pending = [&]() {
        if (!pending_role.empty() && !pending_text.empty()) {
            while (!pending_text.empty() && pending_text.back() == '\n')
                pending_text.pop_back();
            append_chat(QString::fromStdString(pending_role),
                        QString::fromStdString(pending_text));
        }
        pending_role.clear();
        pending_text.clear();
    };

    while (std::getline(ss, line)) {
        if (line.rfind("User: ", 0) == 0) {
            flush_pending();
            pending_role = "User";
            pending_text = line.substr(6);
        } else if (line.rfind("Assistant: ", 0) == 0) {
            flush_pending();
            pending_role = "AI";
            pending_text = line.substr(11);
        } else {
            if (!pending_text.empty()) pending_text += '\n';
            pending_text += line;
        }
    }
    flush_pending();

    m_send_btn->setEnabled(m_backend->hasApiKey());
}

// ====================================================================
// 向对话浏览器追加一条气泡风格的消息
//
// QTextBrowser 基于 QTextDocument（HTML4 / CSS2 子集），大量现代 CSS 不支持。
// 采用 <table align=right|left> + bgcolor + width 百分比实现气泡。
// ====================================================================
void MainWindow::append_chat(const QString &role, const QString &text) {
    QTextCursor cursor = m_chat_display->textCursor();
    cursor.movePosition(QTextCursor::End);

    // HTML 转义 + \n → <br> 保证多段落正确换行
    QString esc = html_escape(text);
    esc.replace("\n", "<br>");

    // 在 QTextDocument 中 <table> 是块级元素，自动上下堆叠不会重叠
    if (role == "User") {
        // 右浮动蓝色气泡，width=72% 限制宽度防止过长文本太难阅读
        cursor.insertHtml(QStringLiteral(
            "<table align='right' width='72%' cellpadding='0' cellspacing='0' border='0'"
            " bgcolor='#0969da' style='margin:5px 0 2px 0;'>"
            "<tr><td style='padding:10px 16px;'>"
            "<span style='color:white; font-size:13px;'>%1</span>"
            "</td></tr></table>"
            "<br>"
        ).arg(esc));
    } else if (role == "AI") {
        // 左浮动白色气泡
        cursor.insertHtml(QStringLiteral(
            "<table align='left' width='72%' cellpadding='0' cellspacing='0' border='0'"
            " bgcolor='#ffffff'"
            " style='margin:5px 0 2px 0; border:1px solid #d0d7de;'>"
            "<tr><td style='padding:10px 16px;'>"
            "<span style='font-size:13px; color:#1e1e1e;'>%1</span>"
            "</td></tr></table>"
            "<br>"
        ).arg(esc));
    } else {
        // 居中黄色警告
        cursor.insertHtml(QStringLiteral(
            "<table align='center' width='85%' cellpadding='0' cellspacing='0' border='0'"
            " bgcolor='#fff3cd'"
            " style='margin:5px 0; border:1px solid #ffe69c;'>"
            "<tr><td style='padding:6px 14px;'>"
            "<span style='font-size:12px; color:#664d03;'>%1: %2</span>"
            "</td></tr></table>"
            "<br>"
        ).arg(html_escape(role), esc));
    }

    m_chat_display->setTextCursor(cursor);
    m_chat_display->ensureCursorVisible();
}

// ====================================================================
// 更新状态栏显示
// ====================================================================
void MainWindow::update_status_bar() {
    QString api_status = m_backend->hasApiKey() ? "🔑 API 已就绪" : "🔒 API Key 未配置";
    statusBar()->showMessage(
        QString("  %1  |  📦 %2  |  Z&S-AI")
            .arg(api_status, m_backend->modelDisplayName()));
}

// ====================================================================
// on_send — 发送按钮 / Enter 键响应
// 只负责 UI 层：取文本 → 显示气泡 → 清空输入 → 委托 Backend
// AI 回复和错误由 Backend 信号异步驱动
// ====================================================================
void MainWindow::on_send() {
    if (m_backend->isBusy()) return;

    QString text = m_chat_input->text().trimmed();
    if (text.isEmpty()) return;

    if (!m_backend->hasApiKey()) {
        QMessageBox::warning(this, "API Key 未设置",
            "请先通过「设置 → API 设置」配置 API Key，\n"
            "或设置环境变量 DEEPSEEK_API_KEY。");
        return;
    }

    // 控件值 → Backend 数据（同步角色卡最新修改）
    sync_fields_to_struct();

    // 显示用户气泡
    append_chat("User", text);

    // 清空输入框
    m_chat_input->clear();

    // 委托 Backend 发送（异步，结果通过信号返回）
    m_backend->sendMessage(text);
}

// ====================================================================
// 保存配置
// ====================================================================
void MainWindow::on_save() {
    sync_fields_to_struct();

    QString filename = QFileDialog::getSaveFileName(this,
        "保存配置", "role.conf",
        "Config files (*.conf);;All files (*)");
    if (filename.isEmpty()) return;

    if (m_backend->saveConfig(filename))
        QMessageBox::information(this, "保存成功", "配置已保存到 " + filename);
    else
        QMessageBox::warning(this, "保存失败", "无法写入文件 " + filename);
}

// ====================================================================
// 加载配置
// ====================================================================
void MainWindow::on_load() {
    QString filename = QFileDialog::getOpenFileName(this,
        "加载配置", "role.conf",
        "Config files (*.conf);;All files (*)");
    if (filename.isEmpty()) return;

    if (m_backend->loadConfig(filename)) {
        sync_struct_to_fields();
        load_history_to_chat();
        QMessageBox::information(this, "加载成功", "配置已从 " + filename + " 加载");
    } else {
        QMessageBox::warning(this, "加载失败", "无法读取文件 " + filename);
    }
}

// ====================================================================
// 生成并显示 system prompt（仅供调试 / 预览）
// ====================================================================
void MainWindow::on_prompt() {
    sync_fields_to_struct();

    QString prompt = m_backend->buildSystemPrompt();

    QDialog dlg(this);
    dlg.setWindowTitle("系统提示词（System Prompt）");
    dlg.resize(700, 500);

    QVBoxLayout *lay = new QVBoxLayout(&dlg);
    QTextEdit *view = new QTextEdit;
    view->setPlainText(prompt);
    view->setReadOnly(true);
    lay->addWidget(view);

    QPushButton *close_btn = new QPushButton("关闭");
    lay->addWidget(close_btn);
    connect(close_btn, &QPushButton::clicked, &dlg, &QDialog::accept);

    dlg.exec();
}

// ====================================================================
// API 设置对话框
// ====================================================================
void MainWindow::on_settings() {
    QDialog dlg(this);
    dlg.setWindowTitle("API 设置");
    dlg.resize(500, 250);

    QFormLayout *form = new QFormLayout(&dlg);

    // API Key（密码模式，不显示明文）
    QLineEdit *api_key = new QLineEdit(QString::fromStdString(m_backend->question().api_key));
    api_key->setEchoMode(QLineEdit::Password);
    api_key->setPlaceholderText("sk-xxxxxxxxxxxxxxxx");
    form->addRow("API Key：", api_key);

    // Base URL
    QLineEdit *base_url = new QLineEdit(QString::fromStdString(m_backend->question().base_url));
    base_url->setPlaceholderText("https://api.deepseek.com");
    form->addRow("Base URL：", base_url);

    // 模型选择
    QComboBox *model = new QComboBox;
    model->addItem("deepseek-v4-flash");
    model->addItem("deepseek-v4-pro");
    model->addItem("deepseek-chat (已弃用)");
    model->addItem("deepseek-reasoner (已弃用)");
    int idx = model->findText(QString::fromStdString(m_backend->question().model),
                              Qt::MatchContains);
    if (idx >= 0) model->setCurrentIndex(idx);
    form->addRow("模型：", model);

    // Reasoning Effort
    QComboBox *reasoning = new QComboBox;
    reasoning->addItems({"", "low", "medium", "high"});
    reasoning->setCurrentText(QString::fromStdString(m_backend->question().reasoning_effort));
    form->addRow("Reasoning Effort：", reasoning);

    // Thinking Type
    QLineEdit *thinking = new QLineEdit(
        QString::fromStdString(m_backend->question().extra_body.thinking_type));
    thinking->setPlaceholderText("enabled");
    form->addRow("Thinking Type：", thinking);

    // 对话框按钮
    QDialogButtonBox *buttons = new QDialogButtonBox(
        QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
    form->addRow(buttons);

    connect(buttons, &QDialogButtonBox::accepted, &dlg, &QDialog::accept);
    connect(buttons, &QDialogButtonBox::rejected, &dlg, &QDialog::reject);

    if (dlg.exec() == QDialog::Accepted) {
        QString model_text = model->currentText();
        if (model_text.contains("已弃用")) {
            QMessageBox::warning(this, "模型已弃用",
                "deepseek-chat 和 deepseek-reasoner 将于 2026-07-24 后不可用。\n"
                "建议使用 deepseek-v4-flash 或 deepseek-v4-pro。");
            model_text = model_text.section(" ", 0, 0);
        }

        auto &q = m_backend->question();
        q.api_key                = api_key->text().toStdString();
        q.base_url               = base_url->text().toStdString();
        q.model                  = model_text.toStdString();
        q.reasoning_effort        = reasoning->currentText().toStdString();
        q.extra_body.thinking_type = thinking->text().toStdString();

        m_send_btn->setEnabled(m_backend->hasApiKey());

        m_backend->saveConfig("role.conf");
        update_status_bar();
    }
}

// ====================================================================
// 清空对话历史
// ====================================================================
void MainWindow::on_clear_history() {
    QMessageBox::StandardButton ans = QMessageBox::question(this,
        "清空历史", "确定要清空所有对话历史吗？\n（角色卡设置不受影响）",
        QMessageBox::Yes | QMessageBox::No, QMessageBox::No);

    if (ans == QMessageBox::Yes) {
        m_backend->clearHistory();
        m_chat_display->clear();
    }
}

// ====================================================================
// 关于对话框
// ====================================================================
void MainWindow::on_about() {
    QMessageBox::about(this, "关于 Z&S-AI",
        "Z&S-AI v1.0\n\n"
        "基于 DeepSeek API 的角色扮演对话工具。\n"
        "使用 Qt6 + C++17 构建。\n\n"
        "模型：deepseek-v4-flash / deepseek-v4-pro\n"
        "API：https://api.deepseek.com");
}

// ====================================================================
// Backend 信号响应：AI 回复就绪
// ====================================================================
void MainWindow::on_backend_response(const QString &content) {
    append_chat("AI", content);
}

// ====================================================================
// Backend 信号响应：错误发生
// ====================================================================
void MainWindow::on_backend_error(const QString &title, const QString &message) {
    append_chat(title, message);
}

// ====================================================================
// Backend 信号响应：busy 状态变化
// ====================================================================
void MainWindow::on_backend_busy_changed(bool busy) {
    m_chat_input->setEnabled(!busy);
    if (busy) {
        m_send_btn->setEnabled(false);
        m_send_btn->setText("发送中…");
        statusBar()->showMessage("  ⏳ 等待 AI 回复中……");
    } else {
        m_send_btn->setText("发送");
        m_send_btn->setEnabled(m_backend->hasApiKey());
        update_status_bar();
    }
}
