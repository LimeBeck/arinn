# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

qt_add_library(arinn-session-lifecycle STATIC
    sessionlifecycle.h dbussessionlifecycle.h dbussessionlifecycle.cpp)
target_link_libraries(arinn-session-lifecycle PUBLIC Qt6::Core PRIVATE Qt6::DBus)
target_include_directories(arinn-session-lifecycle PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
qt_add_executable(arinn-session-lifecycle-test sessionlifecycle_test.cpp)
target_link_libraries(arinn-session-lifecycle-test PRIVATE
    arinn-session-lifecycle Qt6::DBus Qt6::Test)
add_test(NAME session-lifecycle COMMAND dbus-run-session --
    $<TARGET_FILE:arinn-session-lifecycle-test>)
set_tests_properties(session-lifecycle PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
