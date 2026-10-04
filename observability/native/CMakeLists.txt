# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

include(FetchContent)
find_package(CURL REQUIRED)
set(SENTRY_BACKEND inproc CACHE STRING "" FORCE)
set(SENTRY_BUILD_SHARED_LIBS OFF CACHE BOOL "" FORCE)
set(SENTRY_BUILD_TESTS OFF CACHE BOOL "" FORCE)
set(SENTRY_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
set(SENTRY_INTEGRATION_QT OFF CACHE BOOL "" FORCE)
set(SENTRY_LIBUNWIND_SYSTEM ON CACHE BOOL "" FORCE)
set(SENTRY_TRANSPORT_COMPRESSION OFF CACHE BOOL "" FORCE)
set(SENTRY_TRANSPORT curl CACHE STRING "" FORCE)
FetchContent_Declare(sentry_native
    URL https://github.com/getsentry/sentry-native/archive/refs/tags/0.17.1.tar.gz
    URL_HASH SHA256=5d2f1c33205eddd0f6ed130406c18534633ce9107bde76b7e9bbfe0fe88ef57c
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE)
FetchContent_Declare(tomlplusplus
    URL https://github.com/marzer/tomlplusplus/archive/refs/tags/v3.4.0.tar.gz
    URL_HASH SHA256=8517f65938a4faae9ccf8ebb36631a38c1cadfb5efa85d9a72e15b9e97d25155
    DOWNLOAD_EXTRACT_TIMESTAMP TRUE)
FetchContent_MakeAvailable(sentry_native tomlplusplus)
execute_process(COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/harden_sdk.py
    ${sentry_native_SOURCE_DIR} COMMAND_ERROR_IS_FATAL ANY)
execute_process(COMMAND ${Python3_EXECUTABLE} ${CMAKE_CURRENT_SOURCE_DIR}/package_notices.py
    ${sentry_native_SOURCE_DIR} ${tomlplusplus_SOURCE_DIR} ${CMAKE_CURRENT_BINARY_DIR}/licenses
    COMMAND_ERROR_IS_FATAL ANY)
execute_process(COMMAND ${Python3_EXECUTABLE} ${CMAKE_SOURCE_DIR}/scripts/release.py version
    OUTPUT_VARIABLE ARINN_VERSION OUTPUT_STRIP_TRAILING_WHITESPACE
    COMMAND_ERROR_IS_FATAL ANY)
add_library(arinn-telemetry STATIC telemetry.cpp telemetry.h transport.cpp transport.h)
target_link_libraries(arinn-telemetry PUBLIC Qt6::Core sentry tomlplusplus::tomlplusplus CURL::libcurl)
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS ${CMAKE_SOURCE_DIR}/observability/sentry-default-dsn.txt)
file(STRINGS ${CMAKE_SOURCE_DIR}/observability/sentry-default-dsn.txt ARINN_DEFAULT_DSN REGEX "^https://")
list(LENGTH ARINN_DEFAULT_DSN ARINN_DEFAULT_DSN_COUNT)
if(NOT ARINN_DEFAULT_DSN_COUNT EQUAL 1)
    message(FATAL_ERROR "The shared public Sentry default must contain exactly one HTTPS DSN")
endif()
target_compile_definitions(arinn-telemetry PRIVATE ARINN_RELEASE="arinn@${ARINN_VERSION}"
    ARINN_DEFAULT_SENTRY_DSN="${ARINN_DEFAULT_DSN}")
add_executable(arinn-telemetry-config-test config_test.cpp)
target_link_libraries(arinn-telemetry-config-test PRIVATE arinn-telemetry Qt6::Test)
target_compile_definitions(arinn-telemetry-config-test PRIVATE ARINN_DSN_SOURCE="${CMAKE_SOURCE_DIR}/observability/sentry-default-dsn.txt")
add_test(NAME sentry-configuration COMMAND arinn-telemetry-config-test)
# Attach the startup hook directly so its object cannot be dropped by the linker.
target_sources(arinn-shell PRIVATE qt_startup.cpp)
target_link_libraries(arinn-shell PRIVATE arinn-telemetry)
add_executable(arinn-telemetry-fixture fixture.cpp qt_startup.cpp)
target_link_libraries(arinn-telemetry-fixture PRIVATE arinn-telemetry)
add_test(NAME sentry-isolated COMMAND ${Python3_EXECUTABLE}
    ${CMAKE_SOURCE_DIR}/tests/integration/sentry.py
    $<TARGET_FILE:arinn-telemetry-fixture>
    ${CMAKE_SOURCE_DIR}/target/debug/examples/telemetry-fixture)
