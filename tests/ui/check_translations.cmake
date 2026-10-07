# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

# Fails when a shell string has no reviewed Russian translation (#363):
# lupdate runs over the sources into a copy of the committed .ts; any new or
# changed source then shows up as unfinished, and any removed one as vanished.
file(REMOVE_RECURSE "${WORK}")
file(MAKE_DIRECTORY "${WORK}")
get_filename_component(name "${TS}" NAME)
file(COPY "${TS}" DESTINATION "${WORK}")
execute_process(
  COMMAND "${LUPDATE}" ${SOURCES} -ts "${WORK}/${name}" -locations none
  RESULT_VARIABLE result OUTPUT_VARIABLE out ERROR_VARIABLE err)
if(NOT result EQUAL 0)
  message(FATAL_ERROR "lupdate failed: ${err}")
endif()
file(READ "${WORK}/${name}" text)
string(REGEX MATCHALL "type=\"(unfinished|vanished|obsolete)\"" bad "${text}")
list(LENGTH bad count)
if(count GREATER 0)
  message(FATAL_ERROR
    "${count} shell string(s) lack a reviewed Russian translation or are stale. "
    "Run `cmake --build build --target arinn_lupdate`, add the strings to "
    "apps/arinn-shell/i18n/fill_ru.py, run it, and commit arinn_ru.ts.")
endif()
message(STATUS "All shell strings are translated.")
