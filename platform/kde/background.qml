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
    flags: Qt.FramelessWindowHint | Qt.WindowDoesNotAcceptFocus | Qt.BypassWindowManagerHint
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
        const geometry = w.frameGeometry;
        stage.width = Math.max(1, Math.min(1920, Math.round(geometry.width)));
        stage.height = Math.max(1, Math.min(1080, Math.round(geometry.height)));
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
