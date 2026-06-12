# Z&S-AI 的 Qt Quick / QML 图形界面构建文件
# 依赖：Qt6 Quick（QML 界面）、Qt6 Network（HTTP 请求）
# 用法：mkdir build && cd build && cmake .. && make

cmake_minimum_required(VERSION 3.16)
project(ZS_AI LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

# 查找 Qt6 所需的模块
find_package(Qt6 REQUIRED COMPONENTS Quick Network)

# QML 资源文件（包含 main.qml）
qt_add_resources(QML_RESOURCES qml.qrc)

# 可执行目标
add_executable(role
    main.cpp          # QML 入口点
    backend.h         # 后端头文件（MOC 需扫描）
    backend.cpp       # 后端实现（API + 数据 + 持久化）
    ${QML_RESOURCES}  # QML 资源
)

# 链接 Qt 库
target_link_libraries(role PRIVATE
    Qt6::Quick
    Qt6::Network
)

# 包含当前目录（用于 #include 项目内头文件）
target_include_directories(role PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
