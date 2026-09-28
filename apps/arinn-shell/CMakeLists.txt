# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

add_subdirectory("${PROJECT_SOURCE_DIR}/platform/session"
    "${CMAKE_CURRENT_BINARY_DIR}/platform-session")

qt_add_executable(arinn-shell main.cpp backend.cpp backend.h)
qt_add_resources(arinn-shell ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::DBus)
qt_add_executable(arinn-shell-test backend_test.cpp backend.cpp backend.h)
qt_add_resources(arinn-shell-test test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell-test PRIVATE arinn-session-lifecycle Qt6::Quick Qt6::DBus Qt6::Test)

set(ARINN_PRESENTATION_FILES
    ui/qmldir ui/Theme.qml ui/UiText.qml ui/LogicalCanvas.qml
    ui/Surface.qml ui/ActionButton.qml ui/ContentCard.qml ui/AppTile.qml
    ui/HomeRow.qml ui/HomeView.qml
    ui/ProgressTrack.qml ui/Skeleton.qml ui/StatusPane.qml ui/fonts/Onest.ttf)
foreach(target arinn-shell arinn-shell-test)
    qt_add_resources(${target} presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})
endforeach()

# Development-only gallery: no backend, bus registration or platform actions.
qt_add_executable(arinn-ui-gallery gallery/main.cpp)
qt_add_resources(arinn-ui-gallery gallery_ui PREFIX "/" FILES
    gallery/Gallery.qml gallery/HomeGallery.qml gallery/fixture-artwork.svg
    ${ARINN_PRESENTATION_FILES})
target_link_libraries(arinn-ui-gallery PRIVATE Qt6::Quick)

qt_add_executable(arinn-ui-test ../../tests/ui/main.cpp)
target_link_libraries(arinn-ui-test PRIVATE Qt6::QuickTest)
add_test(NAME ui-components COMMAND arinn-ui-test
    -input "${PROJECT_SOURCE_DIR}/tests/ui")
set_tests_properties(ui-components PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")

# Exercise the production window binding against isolated lifecycle/daemon doubles.
target_sources(arinn-session-lifecycle-test PRIVATE backend.cpp backend.h)
target_include_directories(arinn-session-lifecycle-test PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}")
qt_add_resources(arinn-session-lifecycle-test lifecycle_test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-session-lifecycle-test PRIVATE Qt6::Quick)

qt_add_resources(arinn-session-lifecycle-test lifecycle_presentation PREFIX "/" FILES ${ARINN_PRESENTATION_FILES})

qt_add_executable(arinn-catalog-test catalog_test.cpp backend.cpp backend.h)
target_link_libraries(arinn-catalog-test PRIVATE Qt6::DBus Qt6::Test)
add_test(NAME application-catalog COMMAND dbus-run-session -- $<TARGET_FILE:arinn-catalog-test>)
