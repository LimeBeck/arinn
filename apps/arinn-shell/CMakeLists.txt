# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

add_subdirectory("${PROJECT_SOURCE_DIR}/platform/session"
    "${CMAKE_CURRENT_BINARY_DIR}/platform-session")

qt_add_executable(arinn-shell main.cpp backend.cpp backend.h input_bindings.cpp input_bindings.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_sources(arinn-shell PRIVATE startup_home.cpp startup_home.h)
qt_add_resources(arinn-shell ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::Qml Qt6::DBus)
qt_add_executable(arinn-startup-home-test startup_home_test.cpp)
target_link_libraries(arinn-startup-home-test PRIVATE Qt6::DBus Qt6::Test)
target_compile_definitions(arinn-startup-home-test PRIVATE
    ARINN_SHELL_BINARY="$<TARGET_FILE:arinn-shell>")
add_dependencies(arinn-startup-home-test arinn-shell)
add_test(NAME shell-startup-home COMMAND dbus-run-session --
    $<TARGET_FILE:arinn-startup-home-test>)
qt_add_executable(arinn-shell-test backend_test.cpp backend.cpp backend.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
qt_add_resources(arinn-shell-test test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell-test PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::Qml Qt6::DBus Qt6::Test)

set(ARINN_PRESENTATION_FILES
    ui/qmldir ui/Theme.qml ui/Focus.qml ui/InputMode.qml ui/UiText.qml ui/LogicalCanvas.qml
    ui/Surface.qml ui/ActionButton.qml ui/ContentCard.qml ui/AppTile.qml
    ui/HomeRow.qml ui/HomeView.qml ui/BrandLockup.qml ui/BrandButton.qml
    ui/ProgressTrack.qml ui/Skeleton.qml ui/StatusPane.qml ui/LaunchOverlay.qml
    ui/AmbientGlow.qml ui/VolumeHud.qml ui/OnScreenKeyboard.qml ui/KeyboardButton.qml ui/KeyboardIcon.qml
    ui/fonts/Onest.ttf)
foreach(target arinn-shell arinn-shell-test)
    qt_add_resources(${target} presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})
endforeach()

# Development-only gallery: no backend, bus registration or platform actions.
qt_add_executable(arinn-ui-gallery gallery/main.cpp keyboard_model.cpp keyboard_model.h)
qt_add_resources(arinn-ui-gallery gallery_ui PREFIX "/" FILES
    gallery/Gallery.qml gallery/HomeGallery.qml gallery/BrowserGallery.qml
    gallery/SessionGallery.qml gallery/LibraryGallery.qml gallery/SettingsGallery.qml
    gallery/KeyboardGallery.qml ui/screens/browse/qmldir ui/screens/browse/BrowseView.qml
    gallery/fixture-artwork.svg
    ui/screens/settings/qmldir ui/screens/settings/SettingsView.qml
    ui/screens/browser/qmldir ui/screens/browser/BrowserView.qml
    ui/screens/session/qmldir ui/screens/session/SessionView.qml
    ui/screens/library/qmldir ui/screens/library/LibraryView.qml
    ${ARINN_PRESENTATION_FILES})
target_link_libraries(arinn-ui-gallery PRIVATE Qt6::Quick)

# The QML tests load screens directly; the keyboard panel needs its model.
qt_add_executable(arinn-ui-test ../../tests/ui/main.cpp keyboard_model.cpp keyboard_model.h)
target_link_libraries(arinn-ui-test PRIVATE Qt6::QuickTest Qt6::Qml)
add_test(NAME ui-components COMMAND arinn-ui-test
    -input "${PROJECT_SOURCE_DIR}/tests/ui")
set_tests_properties(ui-components PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
add_test(NAME focus-ownership COMMAND ${CMAKE_COMMAND}
    -DSHELL_DIR=${CMAKE_CURRENT_SOURCE_DIR}
    -P "${PROJECT_SOURCE_DIR}/tests/ui/check_focus_calls.cmake")

# Exercise the production window binding against isolated lifecycle/daemon doubles.
target_sources(arinn-session-lifecycle-test PRIVATE backend.cpp backend.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_include_directories(arinn-session-lifecycle-test PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
qt_add_resources(arinn-session-lifecycle-test lifecycle_test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-session-lifecycle-test PRIVATE Qt6::Quick Qt6::Qml)

qt_add_resources(arinn-session-lifecycle-test lifecycle_presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})

qt_add_executable(arinn-catalog-test catalog_test.cpp backend.cpp backend.h)
target_link_libraries(arinn-catalog-test PRIVATE Qt6::DBus Qt6::Test)
add_test(NAME application-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-catalog-test>)

qt_add_executable(arinn-browser-catalog-test browser_catalog_test.cpp browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-browser-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-browser-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME browser-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-browser-catalog-test>)
set_tests_properties(browser-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

set(ARINN_BROWSER_FILES ui/screens/browser/qmldir ui/screens/browser/BrowserView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} browser_screen PREFIX "/" FILES ${ARINN_BROWSER_FILES})
endforeach()

set(ARINN_SETTINGS_FILES ui/screens/settings/qmldir ui/screens/settings/SettingsView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} settings_screen PREFIX "/" FILES ${ARINN_SETTINGS_FILES})
endforeach()

set(ARINN_LIBRARY_FILES ui/screens/library/qmldir ui/screens/library/LibraryView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} library_screen PREFIX "/" FILES ${ARINN_LIBRARY_FILES})
endforeach()

set(ARINN_SESSION_FILES ui/screens/session/qmldir ui/screens/session/SessionView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} session_screen PREFIX "/" FILES ${ARINN_SESSION_FILES})
endforeach()

set(ARINN_POWER_FILES ui/screens/power/qmldir ui/screens/power/PowerMenu.qml
    ui/screens/power/PowerOverlay.qml ui/screens/power/AmbientScreen.qml
    ui/screens/power/AmbientIdle.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} power_screen PREFIX "/" FILES ${ARINN_POWER_FILES})
endforeach()

set(ARINN_PLAYER_FILES ui/screens/player/qmldir ui/screens/player/PlayerView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} player_screen PREFIX "/" FILES ${ARINN_PLAYER_FILES})
endforeach()

qt_add_executable(arinn-media-catalog-test media_catalog_test.cpp media_catalog.cpp media_catalog.h
    browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-media-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-media-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME media-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-media-catalog-test>)
set_tests_properties(media-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

qt_add_executable(arinn-library-catalog-test library_catalog_test.cpp library_catalog.cpp library_catalog.h
    media_catalog.cpp media_catalog.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-library-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-library-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME library-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-library-catalog-test>)
set_tests_properties(library-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

qt_add_executable(arinn-settings-catalog-test settings_catalog_test.cpp settings_catalog.cpp settings_catalog.h)
target_link_libraries(arinn-settings-catalog-test PRIVATE Qt6::DBus Qt6::Gui Qt6::Qml Qt6::Test)
add_test(NAME settings-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-settings-catalog-test>)

qt_add_executable(arinn-asset-images-test asset_images_test.cpp asset_images.cpp asset_images.h)
target_link_libraries(arinn-asset-images-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-asset-images-test PRIVATE
    ARINN_PLACEHOLDER_QML="${PROJECT_SOURCE_DIR}/tests/ui/asset_placeholder.qml")
add_test(NAME asset-images COMMAND dbus-run-session -- $<TARGET_FILE:arinn-asset-images-test>)
qt_add_executable(arinn-input-bindings-test input_bindings_test.cpp input_bindings.cpp input_bindings.h)
target_link_libraries(arinn-input-bindings-test PRIVATE Qt6::Gui Qt6::DBus Qt6::Test)
add_test(NAME input-bindings COMMAND dbus-run-session -- $<TARGET_FILE:arinn-input-bindings-test>)
set_tests_properties(input-bindings PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
qt_add_executable(arinn-input-mode-test input_mode_test.cpp input_mode.cpp input_mode.h)
target_link_libraries(arinn-input-mode-test PRIVATE Qt6::Gui Qt6::Qml Qt6::Test)
add_test(NAME input-mode COMMAND arinn-input-mode-test)
set_tests_properties(input-mode PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
set_tests_properties(asset-images PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# Reuse the approved handoff asset unchanged, including its existing attribution.
foreach(target arinn-shell arinn-shell-test arinn-ui-gallery arinn-ui-test arinn-session-lifecycle-test)
    qt_add_resources(${target} brand PREFIX "/brand"
        BASE "${PROJECT_SOURCE_DIR}/docs/design/handoff/assets"
        FILES "${PROJECT_SOURCE_DIR}/docs/design/handoff/assets/arinn-mark.svg")
endforeach()

foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test
    arinn-browser-catalog-test arinn-media-catalog-test arinn-library-catalog-test)
    target_sources(${target} PRIVATE browse_catalog.cpp browse_catalog.h
        keyboard_model.cpp keyboard_model.h
        download_catalog.cpp download_catalog.h input_mode.cpp input_mode.h
        system_status.cpp system_status.h system_control.cpp system_control.h)
endforeach()
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} downloads_screen PREFIX "/" FILES
        ui/screens/downloads/qmldir ui/screens/downloads/DownloadsView.qml)
endforeach()

qt_add_executable(arinn-download-catalog-test download_catalog_test.cpp
    download_catalog.cpp download_catalog.h)
target_link_libraries(arinn-download-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME download-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-download-catalog-test>)
qt_add_executable(arinn-system-status-test system_status_test.cpp
    system_status.cpp system_status.h)
target_link_libraries(arinn-system-status-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME system-status COMMAND dbus-run-session -- $<TARGET_FILE:arinn-system-status-test>)
qt_add_executable(arinn-system-control-test system_control_test.cpp
    system_control.cpp system_control.h)
target_link_libraries(arinn-system-control-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME system-control COMMAND dbus-run-session -- $<TARGET_FILE:arinn-system-control-test>)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} browse_screen PREFIX "/" FILES
        ui/screens/browse/qmldir ui/screens/browse/BrowseView.qml)
endforeach()

qt_add_executable(arinn-keyboard-model-test keyboard_model_test.cpp
    keyboard_model.cpp keyboard_model.h)
target_link_libraries(arinn-keyboard-model-test PRIVATE Qt6::Qml Qt6::Test)
add_test(NAME keyboard-model COMMAND arinn-keyboard-model-test)

qt_add_executable(arinn-browse-catalog-test browse_catalog_test.cpp browse_catalog.cpp browse_catalog.h)
target_link_libraries(arinn-browse-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME browse-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-browse-catalog-test>)

# Private-bus integration fixture, not installed or used by the shell.
qt_add_executable(arinn-browse-provider-fixture browse_provider_fixture.cpp browse_catalog.cpp browse_catalog.h)
target_link_libraries(arinn-browse-provider-fixture PRIVATE Qt6::DBus Qt6::Qml)

# Russian interface through Qt translations (#363). English qsTr()/tr()
# strings are the source; `cmake --build build --target arinn_lupdate`
# refreshes i18n/arinn_ru.ts from these sources.
file(GLOB ARINN_I18N_QML CONFIGURE_DEPENDS
    "${CMAKE_CURRENT_SOURCE_DIR}/Main.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/*.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/*/*.qml")
file(GLOB ARINN_I18N_CPP CONFIGURE_DEPENDS "${CMAKE_CURRENT_SOURCE_DIR}/*.cpp")
list(FILTER ARINN_I18N_CPP EXCLUDE REGEX "(_test|_fixture)\\.cpp$")
set(ARINN_TS_FILES "${CMAKE_CURRENT_SOURCE_DIR}/i18n/arinn_ru.ts")
qt_add_lupdate(TS_FILES ${ARINN_TS_FILES}
    SOURCES ${ARINN_I18N_QML} ${ARINN_I18N_CPP}
    LUPDATE_TARGET arinn_lupdate
    OPTIONS -no-obsolete -locations none)
qt_add_lrelease(TS_FILES ${ARINN_TS_FILES}
    LRELEASE_TARGET arinn_lrelease
    QM_FILES_OUTPUT_VARIABLE ARINN_QM_FILES)
qt_add_executable(arinn-translations-test translations_test.cpp)
target_link_libraries(arinn-translations-test PRIVATE Qt6::Core Qt6::Test)
add_test(NAME translations COMMAND arinn-translations-test)
foreach(target arinn-shell arinn-translations-test)
    qt_add_resources(${target} translations PREFIX "/i18n"
        BASE "${CMAKE_CURRENT_BINARY_DIR}" FILES ${ARINN_QM_FILES})
endforeach()
# Every shell string has a reviewed Russian translation: a new or changed
# source string without one fails this check (#363).
add_test(NAME translations-complete COMMAND ${CMAKE_COMMAND}
    -DLUPDATE=$<TARGET_FILE:Qt6::lupdate>
    -DTS=${ARINN_TS_FILES}
    "-DSOURCES=${ARINN_I18N_QML};${ARINN_I18N_CPP}"
    -DWORK=${CMAKE_CURRENT_BINARY_DIR}/i18n-check
    -P "${PROJECT_SOURCE_DIR}/tests/ui/check_translations.cmake")
