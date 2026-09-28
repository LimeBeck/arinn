# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

cmake_minimum_required(VERSION 3.24)
project(ArinnPhase0 LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
find_package(Qt6 6.6 REQUIRED COMPONENTS Quick DBus Test QuickTest Svg)
qt_standard_project_setup()
enable_testing()
add_subdirectory(apps/arinn-shell)
