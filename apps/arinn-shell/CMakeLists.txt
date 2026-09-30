# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

add_subdirectory("${PROJECT_SOURCE_DIR}/platform/session"
    "${CMAKE_CURRENT_BINARY_DIR}/platform-session")

qt_add_executable(arinn-shell main.cpp backend.cpp backend.h browser_catalog.cpp browser_catalog.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
qt_add_resources(arinn-shell ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::Qml Qt6::DBus)
qt_add_executable(arinn-shell-test backend_test.cpp backend.cpp backend.h browser_catalog.cpp browser_catalog.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
qt_add_resources(arinn-shell-test test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell-test PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::Qml Qt6::DBus Qt6::Test)

set(ARINN_PRESENTATION_FILES
    ui/qmldir ui/Theme.qml ui/UiText.qml ui/LogicalCanvas.qml
    ui/Surface.qml ui/ActionButton.qml ui/ContentCard.qml ui/AppTile.qml
    ui/HomeRow.qml ui/HomeView.qml ui/BrandLockup.qml ui/BrandButton.qml
    ui/ProgressTrack.qml ui/Skeleton.qml ui/StatusPane.qml ui/LaunchOverlay.qml
    ui/fonts/Onest.ttf)
foreach(target arinn-shell arinn-shell-test)
    qt_add_resources(${target} presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})
endforeach()

# Development-only gallery: no backend, bus registration or platform actions.
qt_add_executable(arinn-ui-gallery gallery/main.cpp)
qt_add_resources(arinn-ui-gallery gallery_ui PREFIX "/" FILES
    gallery/Gallery.qml gallery/HomeGallery.qml gallery/BrowserGallery.qml
    gallery/SessionGallery.qml gallery/LibraryGallery.qml gallery/SettingsGallery.qml
    gallery/fixture-artwork.svg
    ui/screens/settings/qmldir ui/screens/settings/SettingsView.qml
    ui/screens/browser/qmldir ui/screens/browser/BrowserView.qml
    ui/screens/session/qmldir ui/screens/session/SessionView.qml
    ui/screens/library/qmldir ui/screens/library/LibraryView.qml
    ${ARINN_PRESENTATION_FILES})
target_link_libraries(arinn-ui-gallery PRIVATE Qt6::Quick)

qt_add_executable(arinn-ui-test ../../tests/ui/main.cpp)
target_link_libraries(arinn-ui-test PRIVATE Qt6::QuickTest)
add_test(NAME ui-components COMMAND arinn-ui-test
    -input "${PROJECT_SOURCE_DIR}/tests/ui")
set_tests_properties(ui-components PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# Exercise the production window binding against isolated lifecycle/daemon doubles.
target_sources(arinn-session-lifecycle-test PRIVATE backend.cpp backend.h browser_catalog.cpp browser_catalog.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_include_directories(arinn-session-lifecycle-test PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
qt_add_resources(arinn-session-lifecycle-test lifecycle_test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-session-lifecycle-test PRIVATE Qt6::Quick Qt6::Qml)

qt_add_resources(arinn-session-lifecycle-test lifecycle_presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})

qt_add_executable(arinn-catalog-test catalog_test.cpp backend.cpp backend.h)
target_link_libraries(arinn-catalog-test PRIVATE Qt6::DBus Qt6::Test)
add_test(NAME application-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-catalog-test>)

qt_add_executable(arinn-browser-catalog-test browser_catalog_test.cpp browser_catalog.cpp browser_catalog.h media_catalog.cpp media_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
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

qt_add_executable(arinn-media-catalog-test media_catalog_test.cpp media_catalog.cpp media_catalog.h
    browser_catalog.cpp browser_catalog.h library_catalog.cpp library_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
target_link_libraries(arinn-media-catalog-test PRIVATE Qt6::DBus Qt6::Qml Qt6::Quick Qt6::Test)
target_compile_definitions(arinn-media-catalog-test PRIVATE
    ARINN_MAIN_QML="${CMAKE_CURRENT_SOURCE_DIR}/Main.qml")
add_test(NAME media-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-media-catalog-test>)
set_tests_properties(media-catalog PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

qt_add_executable(arinn-library-catalog-test library_catalog_test.cpp library_catalog.cpp library_catalog.h
    media_catalog.cpp media_catalog.h browser_catalog.cpp browser_catalog.h settings_catalog.cpp settings_catalog.h asset_images.cpp asset_images.h)
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
set_tests_properties(asset-images PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# Reuse the approved handoff asset unchanged, including its existing attribution.
foreach(target arinn-shell arinn-shell-test arinn-ui-gallery arinn-ui-test arinn-session-lifecycle-test)
    qt_add_resources(${target} brand PREFIX "/brand"
        BASE "${PROJECT_SOURCE_DIR}/docs/design/handoff/assets"
        FILES "${PROJECT_SOURCE_DIR}/docs/design/handoff/assets/arinn-mark.svg")
endforeach()
