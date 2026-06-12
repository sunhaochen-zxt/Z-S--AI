// ====================================================================
// backend.cpp — Z&S-AI 后端核心实现
//
// 所有业务逻辑从 MainWindow 中提取到这里：
//   - 数据初始化（环境变量、默认值、config 加载）
//   - DeepSeek API 请求构建与异步发送
//   - JSON 响应解析
//   - 配置文件的读写
//   - 对话历史管理
// ====================================================================

#include "backend.h"
#include "load_History&config.h"

#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonArray>
#include <QUrl>
#include <cstdlib>

// ====================================================================
// 构造：初始化默认值 → 读环境变量 → 加载 role.conf
// ====================================================================
Backend::Backend(QObject *parent)
    : QObject(parent)
    , m_network(new QNetworkAccessManager(this))
{
    initDefaults();
}

void Backend::initDefaults() {
    // 从环境变量读取 API Key（如果存在）
    const char *env_key = std::getenv("DEEPSEEK_API_KEY");
    if (env_key)
        m_question.api_key = env_key;

    // 默认值
    m_question.base_url          = "https://api.deepseek.com";
    m_question.model             = "deepseek-v4-flash";
    m_question.reasoning_effort  = "medium";

    // 尝试从默认配置文件恢复上次工作状态
    loadConfig("role.conf");
}

// ====================================================================
// 自动保存到 role.conf
// ====================================================================
void Backend::autoSave() {
    saveConfig("role.conf");
}

// ====================================================================
// 保存 / 加载包装
// ====================================================================
bool Backend::saveConfig(const QString &filePath) {
    return ::save_config(filePath.toStdString(), m_content, m_question);
}

bool Backend::loadConfig(const QString &filePath) {
    return ::load_config(filePath.toStdString(), m_content, m_question);
}

// ====================================================================
// 查询
// ====================================================================
bool Backend::hasApiKey() const {
    return !m_question.api_key.empty();
}

bool Backend::isBusy() const {
    return m_busy;
}

QString Backend::modelDisplayName() const {
    return QString::fromStdString(m_question.model);
}

// ====================================================================
// 构建 system prompt
// ====================================================================
QString Backend::buildSystemPrompt() const {
    return QString::fromStdString(content_creat(m_content));
}

// ====================================================================
// 清空对话历史
// ====================================================================
void Backend::clearHistory() {
    m_content.history_communication.clear();
    m_question.message.clear();
    emit historyCleared();
    emit dataChanged();
}

// ====================================================================
// sendMessage — 核心 API 调用流程
// 1. 记录 User 消息到 history_communication
// 2. 构建 system prompt
// 3. 构建 JSON 请求体
// 4. 异步 POST 到 DeepSeek
// 5. 回调中解析响应 → 通知 UI
// ====================================================================
void Backend::sendMessage(const QString &userText) {
    if (m_busy) return;
    if (userText.trimmed().isEmpty()) return;

    if (!hasApiKey()) {
        emit errorOccurred("API Key 未设置",
            "请先通过「设置 → API 设置」配置 API Key，\n"
            "或设置环境变量 DEEPSEEK_API_KEY。");
        return;
    }

    // ---- 记录 User 消息 ----
    m_content.history_communication += "User: " + userText.toStdString() + "\n";

    // ---- 构建 system prompt ----
    std::string system_prompt = content_creat(m_content);

    // ---- 构建 JSON 请求体 ----
    QJsonObject root;
    root["model"] = QString::fromStdString(m_question.model);

    QJsonArray messages;
    {
        QJsonObject sys;
        sys["role"]    = "system";
        sys["content"] = QString::fromStdString(system_prompt);
        messages.append(sys);
    }
    {
        QJsonObject usr;
        usr["role"]    = "user";
        usr["content"] = userText;
        messages.append(usr);
    }
    root["messages"] = messages;
    root["stream"]   = false;

    if (!m_question.reasoning_effort.empty())
        root["reasoning_effort"] = QString::fromStdString(m_question.reasoning_effort);

    if (!m_question.extra_body.thinking_type.empty()) {
        QJsonObject thinking;
        thinking["type"] = QString::fromStdString(m_question.extra_body.thinking_type);
        QJsonObject extra;
        extra["thinking"] = thinking;
        root["extra_body"] = extra;
    }

    QByteArray json_data = QJsonDocument(root).toJson(QJsonDocument::Compact);

    // ---- 构建 HTTP 请求 ----
    QString url = QString::fromStdString(m_question.base_url);
    if (!url.endsWith('/')) url += '/';
    url += "chat/completions";

    QNetworkRequest req{QUrl(url)};
    req.setHeader(QNetworkRequest::ContentTypeHeader, "application/json");
    req.setRawHeader("Authorization", ("Bearer " + m_question.api_key).c_str());

    // ---- 标记 busy ----
    m_busy = true;
    emit busyChanged(true);

    // ---- 异步 POST ----
    QNetworkReply *reply = m_network->post(req, json_data);
    connect(reply, &QNetworkReply::finished, this,
            [this, reply, userText]() { onReplyFinished(reply, userText); });
}

// ====================================================================
// onReplyFinished — 处理 API 响应
// ====================================================================
void Backend::onReplyFinished(QNetworkReply *reply, const QString &/*userText*/) {
    reply->deleteLater();

    m_busy = false;
    emit busyChanged(false);

    // ---- 网络错误 ----
    if (reply->error() != QNetworkReply::NoError) {
        emit errorOccurred("网络错误", reply->errorString());
        emit dataChanged();
        return;
    }

    // ---- 解析 JSON ----
    QByteArray resp_data = reply->readAll();
    QJsonParseError parse_err;
    QJsonDocument doc = QJsonDocument::fromJson(resp_data, &parse_err);

    if (parse_err.error != QJsonParseError::NoError) {
        emit errorOccurred("解析错误",
            QString("JSON 解析失败：%1").arg(parse_err.errorString()));
        emit dataChanged();
        return;
    }

    QJsonObject obj = doc.object();

    // ---- API 层错误 ----
    if (obj.contains("error")) {
        QJsonObject err_obj = obj["error"].toObject();
        QString msg = err_obj["message"].toString();
        emit errorOccurred("API 错误", msg.isEmpty() ? "(未知错误)" : msg);
        emit dataChanged();
        return;
    }

    // ---- 提取 assistant 回复 ----
    QJsonArray choices = obj["choices"].toArray();
    if (choices.isEmpty()) {
        emit errorOccurred("响应错误", "API 返回了空的 choices 数组");
        emit dataChanged();
        return;
    }

    QString content = choices[0].toObject()["message"].toObject()["content"].toString();

    // ---- 记录 Assistant 消息 ----
    m_content.history_communication += "Assistant: " + content.toStdString() + "\n";

    // ---- 通知 UI ----
    emit responseReady(content);

    // ---- 自动保存 ----
    autoSave();
    emit dataChanged();
}

// ====================================================================
// 角色卡属性 getter / setter
// ====================================================================
#define PROP_IMPL(Field, Getter, Setter, Signal)                  \
    QString Backend::Getter() const {                             \
        return QString::fromStdString(m_content.Field);           \
    }                                                             \
    void Backend::Setter(const QString &v) {                      \
        std::string s = v.toStdString();                          \
        if (m_content.Field != s) { m_content.Field = s; emit Signal(); } \
    }

PROP_IMPL(name,               characterName,        setCharacterName,        characterChanged)
PROP_IMPL(personality,        characterPersonality, setCharacterPersonality, characterChanged)
PROP_IMPL(background,         characterBackground,  setCharacterBackground,  characterChanged)
PROP_IMPL(speaking_style,     characterSpeakingStyle, setCharacterSpeakingStyle, characterChanged)
PROP_IMPL(goals,              characterGoals,       setCharacterGoals,       characterChanged)
PROP_IMPL(scene,              scene,                setScene,                characterChanged)
PROP_IMPL(time,               time,                 setTime,                 characterChanged)
PROP_IMPL(memory,             memory,               setMemory,               characterChanged)
PROP_IMPL(example_dialogues,  exampleDialogues,     setExampleDialogues,     characterChanged)
PROP_IMPL(extra_commend,      extraCommend,         setExtraCommend,         characterChanged)
PROP_IMPL(frankenstein_state, frankensteinState,    setFrankensteinState,    characterChanged)

#undef PROP_IMPL

// ====================================================================
// API 设置属性 getter / setter
// ====================================================================
QString Backend::apiKey() const {
    return QString::fromStdString(m_question.api_key);
}
void Backend::setApiKey(const QString &v) {
    std::string s = v.toStdString();
    if (m_question.api_key != s) { m_question.api_key = s; emit questionChanged(); }
}

QString Backend::baseUrl() const {
    return QString::fromStdString(m_question.base_url);
}
void Backend::setBaseUrl(const QString &v) {
    std::string s = v.toStdString();
    if (m_question.base_url != s) { m_question.base_url = s; emit questionChanged(); }
}

QString Backend::model() const {
    return QString::fromStdString(m_question.model);
}
void Backend::setModel(const QString &v) {
    std::string s = v.toStdString();
    if (m_question.model != s) { m_question.model = s; emit modelChanged(); }
}

QString Backend::reasoningEffort() const {
    return QString::fromStdString(m_question.reasoning_effort);
}
void Backend::setReasoningEffort(const QString &v) {
    std::string s = v.toStdString();
    if (m_question.reasoning_effort != s) { m_question.reasoning_effort = s; emit questionChanged(); }
}

QString Backend::thinkingType() const {
    return QString::fromStdString(m_question.extra_body.thinking_type);
}
void Backend::setThinkingType(const QString &v) {
    std::string s = v.toStdString();
    if (m_question.extra_body.thinking_type != s) { m_question.extra_body.thinking_type = s; emit questionChanged(); }
}

// ====================================================================
// loadChatHistory — 解析 history_communication 为 QVariantList
// 每项 {"role": "User"|"AI", "content": "..."}
// ====================================================================
QVariantList Backend::loadChatHistory() const {
    QVariantList list;
    std::stringstream ss(m_content.history_communication);
    std::string line;
    QString pendingRole;
    QString pendingContent;

    auto flush = [&]() {
        if (!pendingRole.isEmpty() && !pendingContent.isEmpty()) {
            while (pendingContent.endsWith('\n'))
                pendingContent.chop(1);
            QVariantMap m;
            m["role"]    = pendingRole;
            m["content"] = pendingContent;
            list.append(m);
        }
        pendingRole.clear();
        pendingContent.clear();
    };

    while (std::getline(ss, line)) {
        QString qline = QString::fromStdString(line);
        if (qline.startsWith("User: ")) {
            flush();
            pendingRole = "User";
            pendingContent = qline.mid(6);
        } else if (qline.startsWith("Assistant: ")) {
            flush();
            pendingRole = "AI";
            pendingContent = qline.mid(11);
        } else {
            if (!pendingContent.isEmpty()) pendingContent += '\n';
            pendingContent += qline;
        }
    }
    flush();
    return list;
}
