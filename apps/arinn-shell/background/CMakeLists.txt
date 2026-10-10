# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required(VERSION 3.24)
if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
    project(ArinnBackgroundConsumer LANGUAGES CXX)
    enable_testing()
endif()
if(NOT TARGET arinn-background-pixels)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../../platform/background"
        "${CMAKE_CURRENT_BINARY_DIR}/pixels")
endif()
add_library(arinn-background-frame-gate STATIC frame_gate.cpp frame_gate.h)
target_compile_features(arinn-background-frame-gate PUBLIC cxx_std_20)
target_link_libraries(arinn-background-frame-gate PUBLIC arinn-background-pixels)
target_include_directories(arinn-background-frame-gate PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
add_executable(arinn-background-frame-gate-test frame_gate_test.cpp)
target_link_libraries(arinn-background-frame-gate-test PRIVATE arinn-background-frame-gate)
add_test(NAME background-frame-gate COMMAND arinn-background-frame-gate-test)
