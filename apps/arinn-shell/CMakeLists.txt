# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

add_subdirectory("${PROJECT_SOURCE_DIR}/platform/session"
    "${CMAKE_CURRENT_BINARY_DIR}/platform-session")
# Activity background consumer foundations (ADR-0087): sealed pixels and the
# frame admission gate, with their own tests.
add_subdirectory(background)
add_subdirectory("${PROJECT_SOURCE_DIR}/platform/keyboard"
    "${CMAKE_CURRENT_BINARY_DIR}/platform-keyboard")

qt_add_executable(arinn-shell main.cpp backend.cpp backend.h input_bindings.cpp input_bindings.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h interface_sounds.cpp interface_sounds.h)
target_sources(arinn-shell PRIVATE startup_home.cpp startup_home.h
    localization_manager.cpp localization_manager.h)
qt_add_resources(arinn-shell ui PREFIX "/" FILES Main.qml)
set(ARINN_UI_SOUND_NAMES tap edge select back open close ignite kindle error)
set(ARINN_UI_SOUND_FILES)
foreach(name ${ARINN_UI_SOUND_NAMES})
  list(APPEND ARINN_UI_SOUND_FILES "sounds/${name}.wav")
endforeach()
target_link_libraries(arinn-shell PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::Qml Qt6::DBus Qt6::Multimedia)
qt_add_executable(arinn-startup-home-test startup_home_test.cpp)
target_link_libraries(arinn-startup-home-test PRIVATE Qt6::DBus Qt6::Test)
target_compile_definitions(arinn-startup-home-test PRIVATE
    ARINN_SHELL_BINARY="$<TARGET_FILE:arinn-shell>")
add_dependencies(arinn-startup-home-test arinn-shell)
add_test(NAME shell-startup-home COMMAND dbus-run-session --
    $<TARGET_FILE:arinn-startup-home-test>)
qt_add_executable(arinn-interface-sounds-test interface_sounds_test.cpp interface_sounds.cpp interface_sounds.h)
qt_add_resources(arinn-interface-sounds-test ui_sounds PREFIX "/"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}" FILES ${ARINN_UI_SOUND_FILES})
target_link_libraries(arinn-interface-sounds-test PRIVATE Qt6::Gui Qt6::Multimedia Qt6::Qml Qt6::Test)
target_compile_definitions(arinn-interface-sounds-test PRIVATE ARINN_INTERFACE_SOUNDS_TEST)
add_test(NAME interface-sounds COMMAND arinn-interface-sounds-test)
set_tests_properties(interface-sounds PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

qt_add_executable(arinn-shell-test backend_test.cpp backend.cpp backend.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_sources(arinn-shell-test PRIVATE localization_manager.cpp localization_manager.h)
qt_add_resources(arinn-shell-test test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell-test PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::Qml Qt6::DBus Qt6::Test)

set(ARINN_PRESENTATION_FILES
    ui/qmldir ui/Theme.qml ui/Focus.qml ui/InputMode.qml ui/UiText.qml ui/FontWeightAxis.qml ui/LogicalCanvas.qml
    ui/Surface.qml ui/FocusRing.qml ui/ArtworkFocus.qml ui/CastLight.qml ui/ActionButton.qml ui/ContentCard.qml ui/AppTile.qml
    ui/ConfirmationDialog.qml ui/SideSheet.qml
    ui/HomeRow.qml ui/HomeView.qml ui/BrandLockup.qml ui/BrandButton.qml
    ui/ProgressTrack.qml ui/Skeleton.qml ui/StatusPane.qml ui/LaunchOverlay.qml
    ui/AmbientGlow.qml ui/ActivityBackground.qml ui/VolumeHud.qml ui/OnScreenKeyboard.qml ui/KeyboardButton.qml ui/LineIcon.qml
    ui/QuickMenu.qml ui/KeyboardPanel.qml ui/StatusArea.qml ui/artwork.js
    ui/fonts/Onest.ttf ui/fonts/PlayfairDisplay.ttf)
foreach(target arinn-shell arinn-shell-test)
    qt_add_resources(${target} presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})
endforeach()
qt_add_resources(arinn-shell ui_sounds PREFIX "/" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
    FILES ${ARINN_UI_SOUND_FILES})

# Development-only component gallery: no backend, bus registration or platform
# actions. Screens and their states live in tests/ui/visual (#567).
qt_add_executable(arinn-ui-gallery gallery/main.cpp keyboard_model.cpp keyboard_model.h)
qt_add_resources(arinn-ui-gallery gallery_ui PREFIX "/" FILES
    gallery/Gallery.qml gallery/KeyboardGallery.qml
    ui/screens/browse/qmldir ui/screens/browse/BrowseView.qml
    gallery/fixture-artwork.svg
    ${ARINN_PRESENTATION_FILES})
target_link_libraries(arinn-ui-gallery PRIVATE Qt6::Quick)

# The QML tests load screens directly; the keyboard panel needs its model.
qt_add_executable(arinn-ui-test ../../tests/ui/main.cpp keyboard_model.cpp keyboard_model.h)
target_link_libraries(arinn-ui-test PRIVATE Qt6::QuickTest Qt6::Quick Qt6::Qml Qt6::Gui)
# Each file runs in its own process: isolate QML singleton/input state and
# keep the other screens running if one Qt Quick Test process aborts.
# Register Hearth light once; it remains isolated from Home.
file(GLOB_RECURSE ARINN_UI_TEST_FILES CONFIGURE_DEPENDS
    "${PROJECT_SOURCE_DIR}/tests/ui/tst_*.qml")
foreach(file IN LISTS ARINN_UI_TEST_FILES)
    file(RELATIVE_PATH suite "${PROJECT_SOURCE_DIR}/tests/ui" "${file}")
    string(REGEX REPLACE "\\.qml$" "" suite "${suite}")
    string(REPLACE "/" "-" suite "${suite}")
    add_test(NAME "ui-${suite}" COMMAND arinn-ui-test -input "${file}")
    set_tests_properties("ui-${suite}" PROPERTIES ENVIRONMENT
        "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
endforeach()
add_test(NAME focus-ownership COMMAND ${CMAKE_COMMAND}
    -DSHELL_DIR=${CMAKE_CURRENT_SOURCE_DIR}
    -P "${PROJECT_SOURCE_DIR}/tests/ui/check_focus_calls.cmake")
# Shell screens take their focus plate from Theme.plateFocus (#722). A
# hardcoded white plate at 20% misses the night contrast bar.
add_test(NAME focus-fill COMMAND ${Python3_EXECUTABLE}
    "${PROJECT_SOURCE_DIR}/tests/ui/check_focus_fill.py"
    "${PROJECT_SOURCE_DIR}")

# Exercise the production window binding against isolated lifecycle/daemon doubles.
target_sources(arinn-session-lifecycle-test PRIVATE backend.cpp backend.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_include_directories(arinn-session-lifecycle-test PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
qt_add_resources(arinn-session-lifecycle-test lifecycle_test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-session-lifecycle-test PRIVATE Qt6::Quick Qt6::Qml)

qt_add_resources(arinn-session-lifecycle-test lifecycle_presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})

qt_add_executable(arinn-catalog-test catalog_test.cpp backend.cpp backend.h)
target_link_libraries(arinn-catalog-test PRIVATE Qt6::DBus Qt6::Test)
add_test(NAME application-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-catalog-test>)

qt_add_executable(arinn-browser-catalog-test browser_catalog_test.cpp browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-browser-catalog-test PRIVATE Qt6::DBus Qt6::Multimedia Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-browser-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME browser-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-browser-catalog-test>)
set_tests_properties(browser-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

set(ARINN_BROWSER_FILES ui/screens/browser/qmldir ui/screens/browser/BrowserView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} browser_screen PREFIX "/" FILES ${ARINN_BROWSER_FILES})
endforeach()

set(ARINN_MUSIC_FILES ui/screens/music/qmldir ui/screens/music/MusicView.qml
    ui/screens/music/MusicButton.qml ui/screens/music/MusicArtwork.qml
    ui/screens/music/MusicRoomView.qml
    ui/screens/music/MusicShelf.qml ui/screens/music/MusicCard.qml
    ui/screens/music/MusicSourcesSheet.qml ui/screens/music/MusicSourceMark.qml
    ui/screens/music/MusicAllView.qml ui/screens/music/MusicDetailHeader.qml
    ui/screens/music/MusicSearchView.qml ui/screens/music/MusicPill.qml
    ui/screens/music/MusicMoreSheet.qml ui/screens/music/MusicQueueSheet.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} music_screen PREFIX "/" FILES ${ARINN_MUSIC_FILES})
endforeach()

set(ARINN_SETTINGS_FILES ui/screens/settings/qmldir ui/screens/settings/SettingsView.qml
    ui/screens/settings/AboutBlock.qml
    ui/screens/settings/LibraryFoldersView.qml ui/screens/settings/FolderPicker.qml
    ui/screens/settings/FolderGlyph.qml)
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

set(ARINN_RUNNING_FILES ui/screens/running/qmldir ui/screens/running/RunningView.qml
    ui/screens/running/CloseFlow.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} running_screen PREFIX "/" FILES ${ARINN_RUNNING_FILES})
endforeach()

set(ARINN_PLAYER_FILES ui/screens/player/qmldir ui/screens/player/PlayerView.qml)
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} player_screen PREFIX "/" FILES ${ARINN_PLAYER_FILES})
endforeach()

qt_add_executable(arinn-media-catalog-test media_catalog_test.cpp media_catalog.cpp media_catalog.h
    browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-media-catalog-test PRIVATE Qt6::DBus Qt6::Multimedia Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-media-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME media-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-media-catalog-test>)
set_tests_properties(media-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# The quick menu in Main.qml with the real Backend and a fake daemon (ADR-0106).
qt_add_executable(arinn-quick-menu-test quick_menu_test.cpp backend.cpp backend.h
    media_catalog.cpp media_catalog.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h
    library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-quick-menu-test PRIVATE Qt6::DBus Qt6::Multimedia Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-quick-menu-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME quick-menu COMMAND dbus-run-session -- $<TARGET_FILE:arinn-quick-menu-test>)
set_tests_properties(quick-menu PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# The application launch screen in Main.qml with the real Backend and a fake
# daemon that launches (spec §6.5, ADR-0110).
qt_add_executable(arinn-launch-screen-test launch_screen_test.cpp backend.cpp backend.h
    media_catalog.cpp media_catalog.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h
    library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-launch-screen-test PRIVATE Qt6::DBus Qt6::Multimedia Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-launch-screen-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME launch-screen COMMAND dbus-run-session -- $<TARGET_FILE:arinn-launch-screen-test>)
set_tests_properties(launch-screen PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

qt_add_executable(arinn-library-catalog-test library_catalog_test.cpp library_catalog.cpp library_catalog.h
    media_catalog.cpp media_catalog.h browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-library-catalog-test PRIVATE Qt6::DBus Qt6::Multimedia Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-library-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME library-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-library-catalog-test>)
set_tests_properties(library-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

qt_add_executable(arinn-settings-catalog-test settings_catalog_test.cpp settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h)
target_link_libraries(arinn-settings-catalog-test PRIVATE Qt6::DBus Qt6::Gui Qt6::Qml Qt6::Test)
add_test(NAME settings-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-settings-catalog-test>)

# The folder picker's Folders1 client (#764, ADR-0071).
qt_add_executable(arinn-folder-browser-test folder_browser_test.cpp folder_browser.cpp folder_browser.h)
target_link_libraries(arinn-folder-browser-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME folder-browser COMMAND dbus-run-session -- $<TARGET_FILE:arinn-folder-browser-test>)

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

# The global on-screen keyboard (ADR-0105): the daemon client, the panel's
# external mode and Main.qml's handling of Back, Home and a lock.
qt_add_executable(arinn-external-keyboard-test external_keyboard_test.cpp
    browser_catalog.cpp browser_catalog.h return_association.cpp return_association.h
    media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h
    settings_catalog.cpp settings_catalog.h folder_browser.cpp folder_browser.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-external-keyboard-test PRIVATE Qt6::DBus Qt6::Multimedia Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-external-keyboard-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml"
    ARINN_PANEL_QML="${CMAKE_CURRENT_SOURCE_DIR}/ui/KeyboardPanel.qml")
add_test(NAME external-keyboard COMMAND dbus-run-session -- $<TARGET_FILE:arinn-external-keyboard-test>)
set_tests_properties(external-keyboard PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
set(ARINN_MAIN_QML_TEST_TARGETS
    arinn-shell-test arinn-session-lifecycle-test arinn-browser-catalog-test
    arinn-media-catalog-test arinn-library-catalog-test arinn-quick-menu-test
    arinn-launch-screen-test arinn-external-keyboard-test)
foreach(target ${ARINN_MAIN_QML_TEST_TARGETS})
    target_sources(${target} PRIVATE interface_sounds.cpp interface_sounds.h)
    target_link_libraries(${target} PRIVATE Qt6::Multimedia)
    qt_add_resources(${target} ui_sounds PREFIX "/" BASE "${CMAKE_CURRENT_SOURCE_DIR}"
        FILES ${ARINN_UI_SOUND_FILES})
endforeach()
# The same cases on an isolated Xvfb with Qt's xcb platform: the panel is
# override-redirect, has WM_HINTS input = False and leaves focus alone.
find_path(ARINN_XCB_INCLUDE xcb/xcb.h)
find_library(ARINN_XCB_LIBRARY xcb)
if(ARINN_XCB_INCLUDE AND ARINN_XCB_LIBRARY)
    target_compile_definitions(arinn-external-keyboard-test PRIVATE ARINN_TEST_XCB)
    target_include_directories(arinn-external-keyboard-test PRIVATE "${ARINN_XCB_INCLUDE}")
    target_link_libraries(arinn-external-keyboard-test PRIVATE "${ARINN_XCB_LIBRARY}")
endif()
add_test(NAME external-keyboard-x11 COMMAND dbus-run-session -- $<TARGET_FILE:arinn-external-keyboard-test>)
set_tests_properties(external-keyboard-x11 PROPERTIES
    ENVIRONMENT "ARINN_TEST_XVFB=1;QT_QUICK_BACKEND=software"
    SKIP_RETURN_CODE 77)

foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test
    arinn-browser-catalog-test arinn-media-catalog-test arinn-library-catalog-test
    arinn-quick-menu-test arinn-launch-screen-test arinn-external-keyboard-test)
    target_sources(${target} PRIVATE activity_background.cpp activity_background.h music_catalog.cpp music_catalog.h music_room.cpp music_room.h)
    target_link_libraries(${target} PRIVATE arinn-background-frame-gate)
    target_sources(${target} PRIVATE browse_catalog.cpp browse_catalog.h
        keyboard_model.cpp keyboard_model.h
        download_catalog.cpp download_catalog.h input_mode.cpp input_mode.h
        system_status.cpp system_status.h system_control.cpp system_control.h
        system_about.cpp system_about.h
        external_keyboard.cpp external_keyboard.h)
    target_link_libraries(${target} PRIVATE arinn-keyboard-panel)
endforeach()
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} downloads_screen PREFIX "/" FILES
        ui/screens/downloads/qmldir ui/screens/downloads/DownloadsView.qml)
endforeach()

qt_add_executable(arinn-download-catalog-test download_catalog_test.cpp
    download_catalog.cpp download_catalog.h)
target_link_libraries(arinn-download-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
target_sources(arinn-download-catalog-test PRIVATE localization_manager.cpp localization_manager.h)
add_test(NAME download-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-download-catalog-test>)
qt_add_executable(arinn-system-status-test system_status_test.cpp
    system_status.cpp system_status.h)
target_link_libraries(arinn-system-status-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME system-status COMMAND dbus-run-session -- $<TARGET_FILE:arinn-system-status-test>)
qt_add_executable(arinn-system-control-test system_control_test.cpp
    system_control.cpp system_control.h)
target_link_libraries(arinn-system-control-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Test)
add_test(NAME system-control COMMAND dbus-run-session -- $<TARGET_FILE:arinn-system-control-test>)
qt_add_executable(arinn-system-about-test system_about_test.cpp
    system_about.cpp system_about.h)
target_link_libraries(arinn-system-about-test PRIVATE Qt6::DBus Qt6::Gui Qt6::Multimedia Qt6::Qml Qt6::Test)
add_test(NAME system-about COMMAND dbus-run-session -- $<TARGET_FILE:arinn-system-about-test>)
set_tests_properties(system-about PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
foreach(target arinn-shell arinn-shell-test arinn-session-lifecycle-test)
    qt_add_resources(${target} browse_screen PREFIX "/" FILES
        ui/screens/browse/qmldir ui/screens/browse/BrowseView.qml)
endforeach()

qt_add_executable(arinn-activity-background-test activity_background_test.cpp
    activity_background.cpp activity_background.h return_association.cpp return_association.h)
target_link_libraries(arinn-activity-background-test PRIVATE arinn-background-frame-gate
    Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
add_test(NAME activity-background COMMAND dbus-run-session -- $<TARGET_FILE:arinn-activity-background-test>)
set_tests_properties(activity-background PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
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
list(FILTER ARINN_I18N_QML EXCLUDE REGEX "/screens/music/")
list(FILTER ARINN_I18N_CPP EXCLUDE REGEX "/music_(catalog|room)\\.cpp$")
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
qt_add_executable(arinn-localization-manager-test localization_manager_test.cpp
    localization_manager.cpp localization_manager.h keyboard_model.cpp keyboard_model.h)
target_link_libraries(arinn-localization-manager-test PRIVATE Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-localization-manager-test PRIVATE
    ARINN_LRELEASE_BINARY="$<TARGET_FILE:Qt6::lrelease>"
    ARINN_SHELL_SOURCE="${CMAKE_CURRENT_SOURCE_DIR}")
add_test(NAME localization-manager COMMAND arinn-localization-manager-test)
set_tests_properties(localization-manager PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
foreach(target arinn-shell arinn-shell-test arinn-translations-test arinn-ui-test arinn-localization-manager-test)
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

qt_add_executable(arinn-music-catalog-test music_catalog_test.cpp music_catalog.cpp music_catalog.h music_room.cpp music_room.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-music-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-music-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml"
    ARINN_HOME_QML="${CMAKE_CURRENT_SOURCE_DIR}/ui/HomeView.qml"
    ARINN_DAEMON_XML="${PROJECT_SOURCE_DIR}/docs/plugin-api/daemon1.xml")
add_test(NAME music-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-music-catalog-test>)
qt_add_executable(arinn-music-localization-test music_localization_test.cpp
    music_room.cpp music_room.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-music-localization-test PRIVATE Qt6::DBus Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-music-localization-test PRIVATE ARINN_SHELL_SOURCE="${CMAKE_CURRENT_SOURCE_DIR}")
add_test(NAME music-localization COMMAND dbus-run-session -- $<TARGET_FILE:arinn-music-localization-test>)
set_tests_properties(music-localization PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")

# Music has its own catalogue while #718 updates the shared shell translations.
set(ARINN_MUSIC_TS "${CMAKE_CURRENT_SOURCE_DIR}/i18n/music_ru.ts")
set(ARINN_MUSIC_TRANSLATION_SOURCES
    "${CMAKE_CURRENT_SOURCE_DIR}/music_catalog.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/music_room.cpp"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicView.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicRoomView.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicShelf.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicCard.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicSourcesSheet.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicSourceMark.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicAllView.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicDetailHeader.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicSearchView.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicPill.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicMoreSheet.qml"
    "${CMAKE_CURRENT_SOURCE_DIR}/ui/screens/music/MusicQueueSheet.qml")
qt_add_lupdate(TS_FILES ${ARINN_MUSIC_TS}
    SOURCES ${ARINN_MUSIC_TRANSLATION_SOURCES}
    LUPDATE_TARGET arinn_music_lupdate OPTIONS -no-obsolete -locations none)
qt_add_lrelease(TS_FILES ${ARINN_MUSIC_TS}
    LRELEASE_TARGET arinn_music_lrelease QM_FILES_OUTPUT_VARIABLE ARINN_MUSIC_QM)
qt_add_resources(arinn-shell music_translations PREFIX "/i18n"
    BASE "${CMAKE_CURRENT_BINARY_DIR}" FILES ${ARINN_MUSIC_QM})
add_test(NAME music-translations-complete COMMAND ${CMAKE_COMMAND}
    -DLUPDATE=$<TARGET_FILE:Qt6::lupdate> -DTS=${ARINN_MUSIC_TS}
    "-DSOURCES=${ARINN_MUSIC_TRANSLATION_SOURCES}"
    -DWORK=${CMAKE_CURRENT_BINARY_DIR}/music-i18n-check
    -P "${PROJECT_SOURCE_DIR}/tests/ui/check_translations.cmake")
