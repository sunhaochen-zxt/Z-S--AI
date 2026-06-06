# Z&S-AI 的 Qt 图形界面构建文件
# 依赖：Qt6 Widgets（界面）、Qt6 Network（HTTP 请求）
# 用法：mkdir build && cd build && cmake .. && make

cmake_minimum_required(VERSION 3.16)
project(ZS_AI LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
# 启用 Qt MOC（元对象编译器），自动处理 Q_OBJECT 宏
set(CMAKE_AUTOMOC ON)

# 查找 Qt6 所需的模块
find_package(Qt6 REQUIRED COMPONENTS Widgets Network)

# 可执行目标
add_executable(role
    main.cpp          # Qt 入口点
    mainwindow.h      # 主窗口头文件（MOC 需扫描）
    mainwindow.cpp    # 主窗口实现
)

# 链接 Qt 库
target_link_libraries(role PRIVATE
    Qt6::Widgets
    Qt6::Network
)

# 包含当前目录（用于 #include 项目内头文件）
target_include_directories(role PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
