# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required(VERSION 3.24)
if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
    project(ArinnBackgroundEncoder LANGUAGES CXX)
    enable_testing()
endif()
find_package(Qt6 6.8 REQUIRED COMPONENTS Gui Qml Quick DBus)
if(NOT TARGET arinn-background-pixels)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../background"
        "${CMAKE_CURRENT_BINARY_DIR}/pixels")
endif()
add_library(arinn-background-encoder STATIC encode.cpp encode.h)
target_compile_features(arinn-background-encoder PUBLIC cxx_std_20)
target_link_libraries(arinn-background-encoder PUBLIC arinn-background-pixels Qt6::Gui)
target_include_directories(arinn-background-encoder PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
# Linked into the KWin QML plugin below.
set_target_properties(arinn-background-encoder arinn-background-pixels
    PROPERTIES POSITION_INDEPENDENT_CODE ON)
add_executable(arinn-background-encoder-test encode_test.cpp)
target_link_libraries(arinn-background-encoder-test PRIVATE arinn-background-encoder)
add_test(NAME background-encoder COMMAND arinn-background-encoder-test)

# The in-process KWin producer (ADR-0087): QML module org.arinn.background,
# loaded by the packaged declarative KWin script platform/kde/background.qml.
# The package installs qml/org/arinn/background into Qt's QML directory.
qt_add_qml_module(arinn-kwin-background
    URI org.arinn.background
    VERSION 1.0
    PLUGIN_TARGET arinn-kwin-background
    OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/qml/org/arinn/background"
    SOURCES producer.cpp producer.h)
target_link_libraries(arinn-kwin-background PRIVATE arinn-background-encoder
    Qt6::Quick Qt6::DBus)
find_package(Qt6 6.8 COMPONENTS Test)
if(TARGET Qt6::Test)
    add_executable(arinn-kwin-background-test producer_test.cpp producer.cpp producer.h)
    set_target_properties(arinn-kwin-background-test PROPERTIES AUTOMOC ON)
    target_link_libraries(arinn-kwin-background-test PRIVATE arinn-background-encoder
        Qt6::Quick Qt6::DBus Qt6::Test)
    # The packaged KWin script, loaded with a stand-in for KWin's QML API.
    target_compile_definitions(arinn-kwin-background-test PRIVATE
        ARINN_BACKGROUND_QML="${CMAKE_CURRENT_SOURCE_DIR}/../background.qml")
    add_test(NAME kwin-background-producer COMMAND dbus-run-session --
        $<TARGET_FILE:arinn-kwin-background-test>)
    set_tests_properties(kwin-background-producer PROPERTIES ENVIRONMENT
        "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
    # The script's stage on a HiDPI screen whose name matches no output.
    add_test(NAME kwin-background-script COMMAND dbus-run-session --
        $<TARGET_FILE:arinn-kwin-background-test> scriptBoundsAnUnmatchedOutputByTheStageScreen)
    set_tests_properties(kwin-background-script PROPERTIES ENVIRONMENT
        "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software;QT_SCALE_FACTOR=2")
endif()
