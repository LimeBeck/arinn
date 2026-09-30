# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

cmake_minimum_required(VERSION 3.24)
project(ArinnPerformance LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
find_package(Qt6 6.6 REQUIRED COMPONENTS Quick DBus)
qt_standard_project_setup()
enable_testing()
qt_add_executable(arinn-perf main.cpp)
target_link_libraries(arinn-perf PRIVATE Qt6::Quick Qt6::DBus)
target_compile_definitions(arinn-perf PRIVATE
    ARINN_PERF_BUILD_TYPE="${CMAKE_BUILD_TYPE}"
    ARINN_PERF_SCENE="${CMAKE_CURRENT_SOURCE_DIR}/../../tests/perf/Home.qml")
qt_add_resources(arinn-perf brand PREFIX "/brand"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../docs/design/handoff/assets"
    FILES "${CMAKE_CURRENT_SOURCE_DIR}/../../docs/design/handoff/assets/arinn-mark.svg")
find_package(Python3 REQUIRED COMPONENTS Interpreter)
add_test(NAME performance-harness COMMAND "${Python3_EXECUTABLE}"
    "${CMAKE_CURRENT_SOURCE_DIR}/run.py" --samples 20
    --executable $<TARGET_FILE:arinn-perf>
    --output "${CMAKE_CURRENT_BINARY_DIR}/performance-smoke.json")
set_tests_properties(performance-harness PROPERTIES TIMEOUT 60)
