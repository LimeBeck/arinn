# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

cmake_minimum_required(VERSION 3.24)
project(ArinnPhase0 LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
find_package(Qt6 6.8 REQUIRED COMPONENTS Quick DBus Test QuickTest Svg LinguistTools)
qt_standard_project_setup()
enable_testing()
find_package(Python3 REQUIRED COMPONENTS Interpreter)
# One build id for every binary of this build (ADR-0097). Reconfigure when
# the commit or the tracked files change.
execute_process(COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/scripts/build_id.py
    OUTPUT_VARIABLE ARINN_BUILD_ID OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
execute_process(COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/scripts/build_id.py --watch
    OUTPUT_VARIABLE ARINN_BUILD_WATCH OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
string(REPLACE "\n" ";" ARINN_BUILD_WATCH "${ARINN_BUILD_WATCH}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    ${CMAKE_SOURCE_DIR}/scripts/build_id.py ${ARINN_BUILD_WATCH})
configure_file(apps/arinn-shell/arinn_build_id.h.in
    ${CMAKE_BINARY_DIR}/generated/arinn_build_id.h @ONLY)
include_directories(${CMAKE_BINARY_DIR}/generated)
add_subdirectory(apps/arinn-shell)
add_subdirectory(scripts/perf)
add_test(NAME session-failure-notice
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_session_failed.py)
add_test(NAME shell-session-selector
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_shell_launcher.py)
add_test(NAME desktop-entry-launcher
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_arinn_open.py)
add_subdirectory(observability/native)
