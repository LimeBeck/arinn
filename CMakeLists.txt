# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

cmake_minimum_required(VERSION 3.24)
project(ArinnPhase0 LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 20)
find_package(Qt6 6.8 REQUIRED COMPONENTS Quick DBus Test QuickTest Svg LinguistTools Multimedia)
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
# Channel and commit date for Settings → About. Not part of the build id
# (ADR-0097): a detached HEAD or a tarball without git leaves them empty.
execute_process(COMMAND git -C ${CMAKE_SOURCE_DIR} rev-parse --abbrev-ref HEAD
    OUTPUT_VARIABLE ARINN_BUILD_CHANNEL OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET)
execute_process(COMMAND git -C ${CMAKE_SOURCE_DIR} log -1 --format=%cs
    OUTPUT_VARIABLE ARINN_BUILD_DATE OUTPUT_STRIP_TRAILING_WHITESPACE
    ERROR_QUIET)
if(NOT ARINN_BUILD_CHANNEL OR ARINN_BUILD_CHANNEL STREQUAL "HEAD")
    set(ARINN_BUILD_CHANNEL "")
endif()
# CMake's MATCHES does not treat {n} as a count, so the date is spelled out.
if(NOT ARINN_BUILD_DATE MATCHES "^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$")
    set(ARINN_BUILD_DATE "")
endif()
string(REPLACE "\n" ";" ARINN_BUILD_WATCH "${ARINN_BUILD_WATCH}")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
    ${CMAKE_SOURCE_DIR}/scripts/build_id.py ${ARINN_BUILD_WATCH})
configure_file(apps/arinn-shell/arinn_build_id.h.in
    ${CMAKE_BINARY_DIR}/generated/arinn_build_id.h @ONLY)
include_directories(${CMAKE_BINARY_DIR}/generated)
add_test(NAME ui-sounds-render COMMAND ${Python3_EXECUTABLE}
    ${CMAKE_SOURCE_DIR}/scripts/render-ui-sounds.py --check
    ${CMAKE_SOURCE_DIR}/apps/arinn-shell/sounds)
add_subdirectory(apps/arinn-shell)
# In-process KWin producer of Activity background frames (ADR-0087).
add_subdirectory(platform/kde/background)
add_subdirectory(scripts/perf)
add_test(NAME session-failure-notice
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_session_failed.py)
add_test(NAME shell-session-selector
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_shell_launcher.py)
add_test(NAME desktop-entry-launcher
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_arinn_open.py)
add_test(NAME arinn-session-entry
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_session_entry.py)
add_test(NAME restart-after-upgrade
    COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/tests/package/test_restart_user_services.py)
add_subdirectory(observability/native)
