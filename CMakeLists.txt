# ====================================================================
# Z&S-AI — Qt Quick / QML 跨平台构建配置
#
# 支持平台：Linux（原生）、Windows（MinGW 交叉编译）、Android（NDK）
#
# === Linux 原生构建 ===
#   mkdir build && cd build && cmake .. && make -j$(nproc)
#
# === Windows 交叉编译（从 Linux，需要 mingw-w64） ===
#   # 安装工具链（Fedora）：
#   sudo dnf install mingw64-qt6-qtquick mingw64-qt6-qtnetwork
#   sudo dnf install mingw64-gcc-c++ mingw64-winpthreads-static
#   # 构建：
#   mkdir build-win && cd build-win
#   mingw64-cmake -DCMAKE_BUILD_TYPE=Release ..
#   mingw64-make -j$(nproc)
#   # 产物：build-win/role.exe（需将 Qt DLL 一起分发）
#
# === Android（需要 Qt for Android + NDK） ===
#   # 前置：安装 Android SDK/NDK + Qt for Android
#   export ANDROID_NDK_ROOT=/path/to/ndk
#   export ANDROID_SDK_ROOT=/path/to/sdk
#   # 构建：
#   mkdir build-android && cd build-android
#   /path/to/Qt/6.x/android_arm64_v8a/bin/qt-cmake ..
#   make -j$(nproc)
#   # 产物：build-android/android-build/*.apk
# ====================================================================

cmake_minimum_required(VERSION 3.16)
project(ZS_AI LANGUAGES CXX VERSION 2.0)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)

# ====================================================================
# 平台检测
# ====================================================================
if(CMAKE_SYSTEM_NAME STREQUAL "Linux")
    message(STATUS "目标平台：Linux")
elseif(CMAKE_SYSTEM_NAME STREQUAL "Windows")
    message(STATUS "目标平台：Windows")
    set(WIN32_FLAG WIN32)
elseif(CMAKE_SYSTEM_NAME STREQUAL "Android")
    message(STATUS "目标平台：Android")
elseif(CMAKE_SYSTEM_NAME STREQUAL "Darwin")
    message(STATUS "目标平台：macOS")
    set(MACOSX_BUNDLE_FLAG MACOSX_BUNDLE)
else()
    message(WARNING "未知目标平台：${CMAKE_SYSTEM_NAME}，按 Linux 处理")
endif()

# ====================================================================
# Qt 依赖
# ====================================================================
find_package(Qt6 REQUIRED COMPONENTS Quick Network)

# QML 资源（main.qml + M3Colors.qml）
qt_add_resources(QML_RESOURCES qml.qrc)

# ====================================================================
# 可执行目标
# ====================================================================
if(ANDROID)
    qt_add_executable(role
        ${WIN32_FLAG}
        main.cpp
        backend.h
        backend.cpp
        ${QML_RESOURCES}
    )
    target_link_libraries(role PRIVATE Qt6::Quick Qt6::Network)
else()
    add_executable(role
        ${WIN32_FLAG}
        ${MACOSX_BUNDLE_FLAG}
        main.cpp
        backend.h
        backend.cpp
        ${QML_RESOURCES}
    )
    target_link_libraries(role PRIVATE Qt6::Quick Qt6::Network)
endif()

target_include_directories(role PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})

# ====================================================================
# 部署
# ====================================================================
if(CMAKE_SYSTEM_NAME STREQUAL "Windows")
    message(STATUS "构建后请运行 windeployqt 收集依赖 DLL：")
    message(STATUS "  cd build && windeployqt role.exe --qmldir ${CMAKE_SOURCE_DIR}")
endif()
