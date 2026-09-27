import QtQuick
import QtQuick.Window
import QtQuick.Controls

Window {
    id: root
    width: 1280
    height: 720
    visibility: backend.homeVisible && !sessionGuard.blocked ? Window.FullScreen : Window.Hidden
    color: "#e6131416"
    title: "Arinn · Phase 0"
    flags: Qt.Window | Qt.FramelessWindowHint
    property bool navOpen: false
    onVisibleChanged: if (visible) {
        navOpen = false;
        returnButton.forceActiveFocus();
    }
    Shortcut {
        sequences: ["Escape", "Backspace"]
        onActivated: {
            if (root.navOpen)
                root.navOpen = false;
            else if (backend.returnWindow.length)
                backend.back();
            else
                root.navOpen = true;
        }
    }
    Item {
        width: 1280
        height: 720
        anchors.centerIn: parent
        scale: Math.min(root.width / 1280, root.height / 720)
        Text {
            x: 80
            y: 40
            text: "●  Arinn"
            color: "#f2b45a"
            font.pixelSize: 26
            font.weight: Font.DemiBold
        }
        Text {
            x: 80
            y: 115
            text: "Продолжить"
            color: "#f4f5f7"
            font.pixelSize: 58
            font.bold: true
        }
        Text {
            x: 80
            y: 198
            text: backend.connected ? "Home поверх текущего приложения" : "Соединение с Arinn потеряно"
            color: "#c9cbcf"
            font.pixelSize: 24
        }
        Row {
            x: 80
            y: 270
            spacing: 28
            Button {
                id: returnButton
                objectName: "returnButton"
                width: 360
                height: 160
                text: backend.returnWindow.length ? "Вернуться в приложение" : "Навигация"
                onClicked: if (backend.returnWindow.length)
                    backend.back()
                else
                    root.navOpen = !root.navOpen
                KeyNavigation.right: demoButton
                background: Rectangle {
                    radius: 24
                    color: parent.activeFocus ? "#44464a" : "#252629"
                    border.width: parent.activeFocus ? 3 : 0
                    border.color: "#f4f5f7"
                }
                contentItem: Text {
                    text: parent.text
                    color: "#f4f5f7"
                    font.pixelSize: 26
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    wrapMode: Text.WordWrap
                }
            }
            Button {
                id: demoButton
                objectName: "demoButton"
                width: 400
                height: 160
                enabled: backend.available && backend.items.length > 0
                text: enabled ? backend.items[0].title : "Provider недоступен"
                onClicked: backend.activate(backend.items[0].id)
                KeyNavigation.left: returnButton
                background: Rectangle {
                    radius: 24
                    color: parent.activeFocus ? "#44464a" : "#252629"
                    border.width: parent.activeFocus ? 3 : 0
                    border.color: "#f4f5f7"
                    opacity: parent.enabled ? 1 : 0.4
                }
                contentItem: Text {
                    text: parent.text
                    color: "#f4f5f7"
                    font.pixelSize: 26
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    wrapMode: Text.WordWrap
                }
            }
        }
        Text {
            x: 80
            y: 480
            text: backend.message
            color: "#f2b45a"
            font.pixelSize: 24
        }
        Text {
            x: 80
            y: 600
            text: "Meta+F12 — Home    ·    Esc / Backspace — назад    ·    ← → Enter — управление"
            color: "#9a9da3"
            font.pixelSize: 22
        }
        Text {
            x: 80
            y: 645
            text: "Архитектурный прототип · не финальный интерфейс"
            color: "#9a9da3"
            font.pixelSize: 18
        }
        Rectangle {
            visible: root.navOpen
            anchors.centerIn: parent
            width: 640
            height: 200
            radius: 28
            color: "#36383c"
            border.color: "#f4f5f7"
            Text {
                anchors.centerIn: parent
                text: "Главная\nEsc — закрыть навигацию"
                color: "#f4f5f7"
                font.pixelSize: 28
                horizontalAlignment: Text.AlignHCenter
            }
        }
    }
}
