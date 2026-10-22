/*
 * SPDX-FileCopyrightText: 2026 Arinn contributors
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

// Arinn's Activity background producer for KWin (ADR-0087, specification
// §12.3). A packaged declarative KWin script, loaded next to the adapter
// script by arinn-kwin-adapter. It captures only the exact window of a
// pending daemon request (its internalId is the Return target), through
// WindowThumbnail into an invisible, non-focusable stage, and hands the grab
// to BackgroundProducer, which publishes one sealed frame from KWin's own
// bus connection. It never writes a file, never captures the desktop or
// another window, and never delays Home: a failure only cancels the request.
import QtQuick
import QtQuick.Window
import org.kde.kwin as KWin
import org.arinn.background

Window {
    id: stage
    // Bounded logical size; the producer limits physical pixels itself.
    width: 1920
    height: 1080
    visible: false
    opacity: 0
    color: "transparent"
    // Never takes pointer input over Home while it captures.
    flags: Qt.FramelessWindowHint | Qt.WindowDoesNotAcceptFocus | Qt.BypassWindowManagerHint
        | Qt.WindowTransparentForInput
    readonly property bool _q_showWithoutActivating: true
    property var client: null
    property bool armed: false

    BackgroundProducer {
        id: producer
        onRequestChanged: request ? stage.prepare() : stage.release()
    }
    KWin.WindowThumbnail {
        id: thumbnail
        anchors.fill: parent
        client: stage.client
    }
    function findClient(target) {
        for (const w of KWin.Workspace.windows) {
            if (w.internalId.toString() === target)
                return w;
        }
        return null;
    }
    function prepare() {
        const w = findClient(producer.target);
        // Exact identity only; a missing, minimized or deleted window gives
        // no frame instead of a substitute.
        if (!w || w.minimized || w.deleted) {
            producer.fail();
            return;
        }
        client = w;
        for (const display of Qt.application.screens) {
            if (w.output && display.name === w.output.name)
                stage.screen = display;
        }
        // Keep the window's aspect ratio so the thumbnail is not letterboxed.
        const geometry = w.frameGeometry;
        const scale = Math.min(1, 1920 / Math.max(1, geometry.width), 1080 / Math.max(1, geometry.height));
        stage.width = Math.max(1, Math.round(geometry.width * scale));
        stage.height = Math.max(1, Math.round(geometry.height * scale));
        armed = true;
        stage.visible = true;
        stage.requestUpdate();
    }
    function release() {
        armed = false;
        stage.visible = false;
        client = null;
    }
    // Grab after the thumbnail rendered once into the stage.
    onFrameSwapped: {
        if (!armed)
            return;
        armed = false;
        producer.capture(thumbnail);
    }
}
