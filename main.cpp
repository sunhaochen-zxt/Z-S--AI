// ====================================================================
// main.cpp — Qt 应用程序入口点
//
// 创建一个 QApplication 实例，启动主窗口的事件循环。
// 使用 Qt6 Widgets 模块处理所有 GUI 交互。
// ====================================================================

#include <QApplication>
#include "mainwindow.h"

int main(int argc, char *argv[]) {
    // Qt 应用程序对象（管理全局资源、事件循环、字体、主题等）
    QApplication app(argc, argv);

    // 设置应用程序元信息（用于 QSettings 等）
    app.setApplicationName("Z&S-AI");
    app.setApplicationVersion("1.0");
    app.setOrganizationName("Z&S-AI");

    // 创建并显示主窗口
    MainWindow w;
    w.show();

    // 进入 Qt 事件循环（等待用户操作，处理信号/槽和网络回调）
    return app.exec();
}
