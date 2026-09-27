# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

qt_add_executable(arinn-shell main.cpp backend.cpp backend.h sessionguard.cpp sessionguard.h)
qt_add_resources(arinn-shell ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell PRIVATE Qt6::Quick Qt6::DBus)
qt_add_executable(arinn-shell-test backend_test.cpp backend.cpp backend.h sessionguard.cpp sessionguard.h)
qt_add_resources(arinn-shell-test test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell-test PRIVATE Qt6::Quick Qt6::DBus Qt6::Test)
