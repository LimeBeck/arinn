# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

# The on-screen keyboard panel window over external applications (ADR-0105):
# the platform-specific window setup that keeps it from taking focus.
qt_add_library(arinn-keyboard-panel STATIC keyboardpanel.h keyboardpanel.cpp)
target_link_libraries(arinn-keyboard-panel PUBLIC Qt6::Gui)
target_include_directories(arinn-keyboard-panel PUBLIC "${CMAKE_CURRENT_SOURCE_DIR}")
