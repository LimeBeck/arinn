// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell

WaylandCompositor {
    id: compositor
    socketName: "arinn-player-0"

    WaylandOutput {
        id: output
        sizeFollowsWindow: true
        window: Window {
            id: win
            visible: true
            visibility: Window.FullScreen
            color: "black"
            title: "arinn-embed-spike"

            Item { id: stage; anchors.fill: parent }

            // Stand-in for the shell's OSD: QML drawn over the player.
            Rectangle {
                anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
                height: 160
                color: "#99000000"
                z: 10
                Text {
                    anchors.centerIn: parent
                    color: "white"
                    font.pixelSize: 40
                    text: "Arinn OSD over an embedded player · " + clock.text
                }
                Text { id: clock; visible: false; text: Qt.formatTime(new Date(), "hh:mm:ss") }
                Timer { interval: 500; running: true; repeat: true
                        onTriggered: clock.text = Qt.formatTime(new Date(), "hh:mm:ss") }
            }
        }
    }

    XdgShell {
        onToplevelCreated: (toplevel, xdgSurface) => {
            toplevel.sendFullscreen(Qt.size(win.width, win.height));
            surfaceItem.createObject(stage, { shellSurface: xdgSurface });
        }
    }

    Component {
        id: surfaceItem
        ShellSurfaceItem {
            anchors.fill: parent
            // Input stays with the shell; the provider controls the player.
            inputEventsEnabled: false
            onSurfaceDestroyed: destroy()
        }
    }
}
