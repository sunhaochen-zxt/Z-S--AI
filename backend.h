#pragma once
// ====================================================================
// backend.h — Z&S-AI 后端核心
//
// 职责：
//   - 持有 ai_content 和 question_st 数据
//   - 管理 QNetworkAccessManager 和 DeepSeek API 通信
//   - 通过 Q_PROPERTY 暴露数据给 QML 绑定
//   - 通过 Qt 信号通知 UI 状态变化
//
// 零 UI 依赖：不包含任何 QWidget / QDialog / QLayout。
// ====================================================================

#include <QObject>
#include <QNetworkAccessManager>
#include <QNetworkReply>
#include <QString>
#include <QVariantList>
#include <QVariantMap>

#include "ai_content_creator.h"
#include "ai_reciver.h"

class Backend : public QObject {
    Q_OBJECT

    // ========== 角色卡属性 ==========
    Q_PROPERTY(QString characterName        READ characterName        WRITE setCharacterName        NOTIFY characterChanged)
    Q_PROPERTY(QString characterPersonality READ characterPersonality WRITE setCharacterPersonality NOTIFY characterChanged)
    Q_PROPERTY(QString characterBackground  READ characterBackground  WRITE setCharacterBackground  NOTIFY characterChanged)
    Q_PROPERTY(QString characterSpeakingStyle READ characterSpeakingStyle WRITE setCharacterSpeakingStyle NOTIFY characterChanged)
    Q_PROPERTY(QString characterGoals       READ characterGoals       WRITE setCharacterGoals       NOTIFY characterChanged)
    Q_PROPERTY(QString scene                READ scene                WRITE setScene                NOTIFY characterChanged)
    Q_PROPERTY(QString time                 READ time                 WRITE setTime                 NOTIFY characterChanged)
    Q_PROPERTY(QString memory               READ memory               WRITE setMemory               NOTIFY characterChanged)
    Q_PROPERTY(QString exampleDialogues     READ exampleDialogues     WRITE setExampleDialogues     NOTIFY characterChanged)
    Q_PROPERTY(QString extraCommend         READ extraCommend         WRITE setExtraCommend         NOTIFY characterChanged)
    Q_PROPERTY(QString frankensteinState    READ frankensteinState    WRITE setFrankensteinState    NOTIFY characterChanged)

    // ========== API 设置 ==========
    Q_PROPERTY(QString apiKey           READ apiKey           WRITE setApiKey           NOTIFY questionChanged)
    Q_PROPERTY(QString baseUrl          READ baseUrl          WRITE setBaseUrl          NOTIFY questionChanged)
    Q_PROPERTY(QString model            READ model            WRITE setModel            NOTIFY modelChanged)
    Q_PROPERTY(QString reasoningEffort  READ reasoningEffort  WRITE setReasoningEffort  NOTIFY questionChanged)
    Q_PROPERTY(QString thinkingType     READ thinkingType     WRITE setThinkingType     NOTIFY questionChanged)

    // ========== 状态（只读） ==========
    Q_PROPERTY(bool    busy           READ isBusy           NOTIFY busyChanged)
    Q_PROPERTY(bool    hasApiKey      READ hasApiKey        NOTIFY questionChanged)
    Q_PROPERTY(QString modelDisplay   READ modelDisplayName NOTIFY modelChanged)

public:
    explicit Backend(QObject *parent = nullptr);

    // ========== 角色卡 getter / setter ==========
    QString characterName() const;
    void setCharacterName(const QString &v);
    QString characterPersonality() const;
    void setCharacterPersonality(const QString &v);
    QString characterBackground() const;
    void setCharacterBackground(const QString &v);
    QString characterSpeakingStyle() const;
    void setCharacterSpeakingStyle(const QString &v);
    QString characterGoals() const;
    void setCharacterGoals(const QString &v);
    QString scene() const;
    void setScene(const QString &v);
    QString time() const;
    void setTime(const QString &v);
    QString memory() const;
    void setMemory(const QString &v);
    QString exampleDialogues() const;
    void setExampleDialogues(const QString &v);
    QString extraCommend() const;
    void setExtraCommend(const QString &v);
    QString frankensteinState() const;
    void setFrankensteinState(const QString &v);

    // ========== API 设置 getter / setter ==========
    QString apiKey() const;
    void setApiKey(const QString &v);
    QString baseUrl() const;
    void setBaseUrl(const QString &v);
    QString model() const;
    void setModel(const QString &v);
    QString reasoningEffort() const;
    void setReasoningEffort(const QString &v);
    QString thinkingType() const;
    void setThinkingType(const QString &v);

    // ========== 底层引用（C++ 侧直接操作 std::string） ==========
    ai_content&  content()        { return m_content; }
    const ai_content& content() const { return m_content; }
    question_st& question()       { return m_question; }
    const question_st& question() const { return m_question; }

    // ========== 操作（Q_INVOKABLE 供 QML 调用） ==========
    Q_INVOKABLE void sendMessage(const QString &userText);
    Q_INVOKABLE void clearHistory();
    Q_INVOKABLE bool saveConfig(const QString &filePath);
    Q_INVOKABLE bool loadConfig(const QString &filePath);
    Q_INVOKABLE QString buildSystemPrompt() const;
    Q_INVOKABLE QVariantList loadChatHistory() const;

    // ========== 查询 ==========
    bool hasApiKey() const;
    bool isBusy() const;
    QString modelDisplayName() const;

signals:
    void busyChanged(bool busy);
    void responseReady(const QString &content);
    void errorOccurred(const QString &title, const QString &message);
    void dataChanged();
    void characterChanged();
    void questionChanged();
    void modelChanged();
    void historyCleared();

private:
    void initDefaults();
    void autoSave();
    void onReplyFinished(QNetworkReply *reply, const QString &userText);

    ai_content  m_content;
    question_st m_question;
    QNetworkAccessManager *m_network = nullptr;
    bool m_busy = false;
};
