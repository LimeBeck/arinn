# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# Also usable as a focused build without the shell or its fetched dependencies.
cmake_minimum_required(VERSION 3.24)
if(CMAKE_SOURCE_DIR STREQUAL CMAKE_CURRENT_SOURCE_DIR)
    project(ArinnBackgroundPixels LANGUAGES CXX)
    enable_testing()
endif()
add_library(arinn-background-pixels STATIC pixels.cpp pixels.h)
target_compile_features(arinn-background-pixels PUBLIC cxx_std_20)
target_include_directories(arinn-background-pixels PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
add_executable(arinn-background-pixels-test pixels_test.cpp)
target_link_libraries(arinn-background-pixels-test PRIVATE arinn-background-pixels)
add_test(NAME background-pixels COMMAND arinn-background-pixels-test)
