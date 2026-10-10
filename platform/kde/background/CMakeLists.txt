# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
cmake_minimum_required(VERSION 3.24)
if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
    project(ArinnBackgroundEncoder LANGUAGES CXX)
    enable_testing()
endif()
find_package(Qt6 6.8 REQUIRED COMPONENTS Gui)
if(NOT TARGET arinn-background-pixels)
    add_subdirectory("${CMAKE_CURRENT_SOURCE_DIR}/../../background"
        "${CMAKE_CURRENT_BINARY_DIR}/pixels")
endif()
add_library(arinn-background-encoder STATIC encode.cpp encode.h)
target_compile_features(arinn-background-encoder PUBLIC cxx_std_20)
target_link_libraries(arinn-background-encoder PUBLIC arinn-background-pixels Qt6::Gui)
target_include_directories(arinn-background-encoder PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
add_executable(arinn-background-encoder-test encode_test.cpp)
target_link_libraries(arinn-background-encoder-test PRIVATE arinn-background-encoder)
add_test(NAME background-encoder COMMAND arinn-background-encoder-test)
