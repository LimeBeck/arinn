// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later
import QtQuick
import "../.." as Ui

Item {
    id: root
    property url source: ""
    property real radius: Ui.Theme.stillRadius
    // The note drawn while there is no image; off where the caller draws its
    // own placeholder, so only one glyph shows (#843).
    property bool placeholder: true
    property Ui.ArtworkFrame frame: null
    readonly property var renderer: frame || embedded.item
    readonly property bool loaded: renderer ? renderer.loaded : false
    property int retryInterval: 1000
    property int attempts: 0
    property bool retryBlank: false
    function retryAfterFailure() {
        if (visible && source.toString().length && attempts < 6 && !retry.running)
            retry.start();
    }
    onSourceChanged: {
        retry.stop();
        attempts = 0;
        retryBlank = false;
    }
    onVisibleChanged: {
        if (!visible)
            retry.stop();
        else if (renderer && renderer.image.status === Image.Error)
            retryAfterFailure();
    }
    Timer {
        id: retry
        objectName: "musicArtworkRetry"
        interval: Math.min(30000, root.retryInterval * Math.pow(2, root.attempts))
        onTriggered: {
            if (!root.visible || root.attempts >= 6)
                return;
            root.attempts++;
            root.retryBlank = true;
            Qt.callLater(() => root.retryBlank = false);
        }
    }
    Binding {
        target: root.renderer
        property: "source"
        value: root.retryBlank ? "" : root.source
        when: root.renderer !== null
    }
    Connections {
        target: root.renderer ? root.renderer.image : null
        function onStatusChanged() {
            if (root.renderer.image.status === Image.Error)
                root.retryAfterFailure();
        }
    }
    Loader {
        id: embedded
        anchors.fill: parent
        active: !root.frame
        sourceComponent: Ui.ArtworkFrame {
            radius: root.radius
            cache: false
            placeholderColor: Ui.Theme.plateObject
            placeholder: Ui.LineIcon {
                anchors.centerIn: parent
                width: Math.min(root.width, root.height) * 0.36
                height: width
                name: "music_note"
                color: Ui.Theme.textHint
                visible: root.placeholder
            }
        }
    }
}
