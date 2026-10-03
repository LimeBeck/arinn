# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# ADR-0076: only ui/Focus.qml may call forceActiveFocus(). Every other caller
# goes through Focus.take(), which refuses hidden or disabled items.
file(GLOB_RECURSE qml_files "${SHELL_DIR}/*.qml")
set(offenders "")
foreach(file IN LISTS qml_files)
    if(file MATCHES "/ui/Focus\\.qml$")
        continue()
    endif()
    file(STRINGS "${file}" lines REGEX "forceActiveFocus\\(")
    foreach(line IN LISTS lines)
        # Comments may name the function.
        if(NOT line MATCHES "^[ \t]*//")
            list(APPEND offenders "${file}: ${line}")
        endif()
    endforeach()
endforeach()
if(offenders)
    list(JOIN offenders "\n" text)
    message(FATAL_ERROR "forceActiveFocus() outside ui/Focus.qml; use Focus.take():\n${text}")
endif()
