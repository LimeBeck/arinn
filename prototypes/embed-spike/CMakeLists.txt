# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required(VERSION 3.21)
project(arinn-embed-spike LANGUAGES CXX)
find_package(Qt6 REQUIRED COMPONENTS Gui Quick WaylandCompositor)
qt_standard_project_setup(REQUIRES 6.5)
qt_add_executable(arinn-embed-spike main.cpp)
qt_add_qml_module(arinn-embed-spike URI Spike VERSION 1.0 QML_FILES Main.qml)
target_link_libraries(arinn-embed-spike PRIVATE Qt6::Gui Qt6::Quick Qt6::WaylandCompositor)
