// ====================================================================
// main.cpp — Qt Quick / QML 应用程序入口点
//
// 使用 QQmlApplicationEngine 加载 Material 风格的 QML 界面。
// Backend 实例作为 context property 暴露给 QML。
// ====================================================================

#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>

#include "backend.h"

int main(int argc, char *argv[]) {
    // 强制 Material Design 风格（Qt Quick Controls 2）
    qputenv("QT_QUICK_CONTROLS_STYLE", "Material");

    QGuiApplication app(argc, argv);
    app.setApplicationName("Z&S-AI");
    app.setApplicationVersion("2.0");
    app.setOrganizationName("Z&S-AI");

    QQmlApplicationEngine engine;

    // 将 Backend 实例注册为 QML 上下文属性
    Backend backend(&app);
    engine.rootContext()->setContextProperty("backend", &backend);

    // 加载主 QML 文件
    engine.load(QUrl(QStringLiteral("qrc:/main.qml")));

    if (engine.rootObjects().isEmpty())
        return -1;

    return app.exec();
}
