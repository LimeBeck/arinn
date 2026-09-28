# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

qt_add_executable(arinn-shell main.cpp backend.cpp backend.h sessionguard.cpp sessionguard.h)
qt_add_resources(arinn-shell ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell PRIVATE Qt6::Quick Qt6::DBus)
qt_add_executable(arinn-shell-test backend_test.cpp backend.cpp backend.h sessionguard.cpp sessionguard.h)
qt_add_resources(arinn-shell-test test_ui PREFIX "/" FILES Main.qml)
target_link_libraries(arinn-shell-test PRIVATE Qt6::Quick Qt6::DBus Qt6::Test)

# Development-only gallery: no backend, bus registration or platform actions.
qt_add_executable(arinn-ui-gallery gallery/main.cpp)
qt_add_resources(arinn-ui-gallery gallery_ui PREFIX "/" FILES
    gallery/Gallery.qml gallery/HomeGallery.qml gallery/fixture-artwork.svg
    ui/qmldir ui/Theme.qml ui/UiText.qml ui/LogicalCanvas.qml
    ui/Surface.qml ui/ActionButton.qml ui/ContentCard.qml ui/AppTile.qml
    ui/HomeRow.qml ui/HomeView.qml
    ui/ProgressTrack.qml ui/Skeleton.qml ui/StatusPane.qml ui/fonts/Onest.ttf)
target_link_libraries(arinn-ui-gallery PRIVATE Qt6::Quick)

qt_add_executable(arinn-ui-test ../../tests/ui/main.cpp)
target_link_libraries(arinn-ui-test PRIVATE Qt6::QuickTest)
add_test(NAME ui-components COMMAND arinn-ui-test
    -input "${PROJECT_SOURCE_DIR}/tests/ui")
set_tests_properties(ui-components PROPERTIES ENVIRONMENT
    "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
