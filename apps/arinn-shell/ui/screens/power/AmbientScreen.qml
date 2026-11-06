// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../.." as Ui
import "../music" as Music

// Ambient (screensaver) from handoff 20 and spec §5.7: a deep background
// with a dim clock and date that drift to a new place every 8 s. It starts
// by an explicit action or after the idle timeout, and any key or pointer
// movement wakes it. The first wake event is consumed: it never reaches the
// screen beneath. Ambient never stops applications, pauses playback or
// requests OS suspend; it only draws.
//
// While native music plays or is paused, a track line sits under the date
// in the drifting block, and a faint glow in the artwork's colour lies at
// the lower left (ADR-0142 MH-8, music brief *Presence › Ambient*, frame
// mh-07, #791). Play/Pause then acts on the music without waking.
FocusScope {
    id: root
    objectName: "ambientScreen"
    width: 1920
    height: 1080
    visible: active
    enabled: active

    property bool active: false
    property string clockText: ""
    property string dateText: ""
    // Seconds between drifts and the drift duration (handoff: 4 s every 8 s).
    property int driftInterval: 8000
    property int driftDuration: 4000
    // True once the opening event has gone by: only then does input wake.
    property bool armed: false
    property bool waking: false
    property bool anchorKnown: false
    property real anchorX: 0
    property real anchorY: 0
    // Where the drift puts the block, centred until the first drift. The
    // block's own position clamps it to the screen: the block is as wide as
    // its text, so a longer date (at midnight, or after a locale change)
    // never pushes it off screen where the last drift left it, also while a
    // drift is under way.
    property real driftX: (width - block.width) / 2
    property real driftY: (height - block.height) / 2
    Behavior on driftX { NumberAnimation { duration: root.driftDuration; easing.type: Easing.InOutQuad } }
    Behavior on driftY { NumberAnimation { duration: root.driftDuration; easing.type: Easing.InOutQuad } }

    signal wakeRequested()
    // Play/Pause while the track line shows: "toggle", "play" or "pause".
    // The screen stays on (MH-8).
    signal musicKeyRequested(string action)

    // The native music session as MusicCatalog.activeState publishes it, or
    // an empty map. Only read here: Ambient never plays or pauses it.
    property var music: ({})
    readonly property var musicTrack: music && music.current && music.current.track ? music.current.track : ({})
    readonly property string musicPlayback: music && music.playback ? String(music.playback) : ""
    readonly property bool musicShown: (musicPlayback === "playing" || musicPlayback === "paused")
                                       && String(musicTrack.title || "").length > 0
    readonly property bool musicPaused: musicPlayback === "paused"
    // "artist · station", the album when there is no station.
    readonly property string musicSubline: {
        const station = music && music.station ? String(music.station.title || "") : "";
        return [String(musicTrack.artist || ""), station || String(musicTrack.album || "")]
            .filter(part => part.length > 0).join(" · ");
    }
    // A Play/Pause key while the line shows: true when it belongs to the
    // music, so it never wakes, held or not; the action is sent once per
    // press. Media keys reach here as ordinary keys.
    function musicKey(key, repeated) {
        if (!musicShown)
            return false;
        const action = key === Qt.Key_MediaTogglePlayPause || key === Qt.Key_Play ? "toggle"
                     : key === Qt.Key_MediaPlay ? "play"
                     : key === Qt.Key_MediaPause ? "pause" : "";
        if (!action.length)
            return false;
        if (!repeated)
            musicKeyRequested(action);
        return true;
    }

    function start() {
        if (active)
            return;
        armed = false;
        waking = false;
        anchorKnown = false;
        if (pointerTracker.hovered)
            remember(pointerTracker.point.position.x, pointerTracker.point.position.y);
        active = true;
        // Take focus after the opening key release, so that release is not
        // also the wake.
        Qt.callLater(() => {
            if (!active)
                return;
            Ui.Focus.take(root);
            Qt.callLater(() => { if (active) armed = true; });
        });
    }
    // Leaves without a wake event, e.g. when the session locks.
    function stop() {
        active = false;
        armed = false;
        waking = false;
        anchorKnown = false;
    }
    function remember(x, y) {
        anchorX = x;
        anchorY = y;
        anchorKnown = true;
    }
    function wake() {
        if (!active)
            return;
        stop();
        wakeRequested();
    }
    // Drifts the block towards (x, y), kept on screen.
    function moveTo(x, y) {
        driftX = Math.max(0, Math.min(x, width - block.width));
        driftY = Math.max(0, Math.min(y, height - block.height));
    }
    // A random place where the whole block fits: sampling the screen and
    // clamping would pile places up on the right and bottom edges.
    function drift() {
        moveTo(Math.random() * Math.max(0, width - block.width),
               Math.random() * Math.max(0, height - block.height));
    }

    // Colours of handoff 20. The clock and date are the prototype's oklch
    // values converted exactly to sRGB. The background keeps bg-deep's hex
    // from the handoff token table: the token's oklch(0.07 0.003 260), which
    // the prototype markup also uses, converts exactly to #010101 (#487).
    readonly property color backgroundColor: "#050506" // bg-deep, token-table hex
    readonly property color clockColor: "#7F8083" // oklch(0.60 0.004 260)
    readonly property color dateColor: "#545557" // oklch(0.45 0.004 260)

    Rectangle {
        objectName: "ambientBackground"
        anchors.fill: parent
        color: root.backgroundColor
    }
    // The artwork's mean colour, from a 24×24 decode; nothing full-size is
    // read (spec §6.5). Transparent until it is known.
    property color musicGlowColor: "transparent"
    Image {
        id: glowSample
        visible: false
        source: root.musicShown ? (root.musicTrack.image || "") : ""
        sourceSize.width: 24
        sourceSize.height: 24
        asynchronous: true
        onStatusChanged: {
            if (status === Image.Ready) glowSampler.requestPaint();
            else root.musicGlowColor = "transparent";
        }
    }
    Canvas {
        id: glowSampler
        width: 24; height: 24
        opacity: 0
        renderTarget: Canvas.Image
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            if (glowSample.status !== Image.Ready) return;
            ctx.drawImage(glowSample, 0, 0, width, height);
            const data = ctx.getImageData(0, 0, width, height).data;
            // Weighted by saturation, so the colour is the artwork's hue and
            // not its grey.
            let r = 0, g = 0, b = 0, weight = 0;
            for (let i = 0; i < data.length; i += 4) {
                const max = Math.max(data[i], data[i + 1], data[i + 2]);
                const min = Math.min(data[i], data[i + 1], data[i + 2]);
                const w = (max - min) / 255 * (data[i + 3] / 255) + 0.001;
                r += data[i] * w; g += data[i + 1] * w; b += data[i + 2] * w;
                weight += w;
            }
            root.musicGlowColor = Qt.rgba(r / weight / 255, g / weight / 255, b / weight / 255, 1);
        }
    }
    // The faint second glow of mh-07: an ellipse 40 % × 34 % of the screen
    // centred at 30 % across and 115 % down, at 30 %.
    Canvas {
        id: musicGlow
        objectName: "ambientMusicGlow"
        anchors.fill: parent
        visible: root.musicShown && root.musicGlowColor.a > 0
        opacity: root.musicPaused ? 0.6 : 1
        onVisibleChanged: if (visible) requestPaint()
        onWidthChanged: requestPaint()
        Connections {
            target: root
            function onMusicGlowColorChanged() { musicGlow.requestPaint(); }
        }
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const c = root.musicGlowColor;
            if (c.a <= 0) return;
            const rx = width * 0.40, ry = height * 0.34;
            ctx.save();
            ctx.translate(width * 0.30, height * 1.15);
            ctx.scale(1, ry / rx);
            const gradient = ctx.createRadialGradient(0, 0, 0, 0, 0, rx);
            gradient.addColorStop(0, Qt.rgba(c.r, c.g, c.b, 0.30));
            gradient.addColorStop(1, Qt.rgba(c.r, c.g, c.b, 0));
            ctx.fillStyle = gradient;
            ctx.fillRect(-rx, -rx, rx * 2, rx * 2);
            ctx.restore();
        }
    }
    // As in the prototype: the date sits left-aligned under the clock,
    // 6 px in, 10 px below it.
    Column {
        id: block
        objectName: "ambientBlock"
        x: Math.max(0, Math.min(root.driftX, root.width - width))
        y: Math.max(0, Math.min(root.driftY, root.height - height))
        spacing: 10
        // 160/500, tabular numerals and −3 % tracking.
        Ui.UiText {
            objectName: "ambientClock"
            font.pixelSize: 160
            font.weight: 500
            font.letterSpacing: -0.03 * 160
            font.features: ({ "tnum": 1 })
            // The prototype's line-height 1: a 160 px line box with the
            // glyphs centred in it, as CSS does.
            height: 160
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.NoWrap
            color: root.clockColor
            text: root.clockText
        }
        // 32/500.
        Ui.UiText {
            objectName: "ambientDate"
            leftPadding: 6
            font.pixelSize: 32
            font.weight: 500
            wrapMode: Text.NoWrap
            color: root.dateColor
            text: root.dateText
        }
        // The track line (mh-07): the cover 96, r8, at 90 %; the equaliser
        // and the track in 600/30; "artist · station" in 26 below. On pause
        // the line dims to 60 %, the bars stand still and the second line
        // reads "Paused". It goes when the music stops.
        Item {
            objectName: "ambientMusic"
            visible: root.musicShown
            width: visible ? musicText.x + musicText.width : 0
            height: visible ? 96 + 14 : 0
            opacity: root.musicPaused ? 0.6 : 1
            Music.MusicArtwork {
                objectName: "ambientMusicCover"
                x: 4; y: 14
                width: 96; height: 96
                radius: 8
                opacity: 0.9
                source: root.musicTrack.image || ""
            }
            Column {
                id: musicText
                x: 128; y: 22
                spacing: 8
                Row {
                    spacing: 14
                    Row {
                        id: equaliser
                        objectName: "ambientMusicEqualiser"
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 4
                        height: 20
                        Repeater {
                            model: [{rest: 12, period: 1100}, {rest: 18, period: 900}, {rest: 9, period: 1300}]
                            delegate: Rectangle {
                                required property var modelData
                                width: 5; radius: 2
                                anchors.bottom: parent.bottom
                                color: root.musicPaused ? root.dateColor : Ui.Theme.ember
                                height: modelData.rest
                                SequentialAnimation on height {
                                    running: root.active && root.musicShown && !root.musicPaused
                                    loops: Animation.Infinite
                                    NumberAnimation { to: 20; duration: modelData.period / 2; easing.type: Easing.InOutQuad }
                                    NumberAnimation { to: 6; duration: modelData.period / 2; easing.type: Easing.InOutQuad }
                                }
                            }
                        }
                    }
                    Ui.UiText {
                        objectName: "ambientMusicTitle"
                        font.pixelSize: 30
                        font.weight: 600
                        wrapMode: Text.NoWrap
                        elide: Text.ElideRight
                        width: Math.min(implicitWidth, 900)
                        color: root.clockColor
                        text: String(root.musicTrack.title || "")
                    }
                }
                Ui.UiText {
                    objectName: "ambientMusicSubline"
                    font.pixelSize: 26
                    font.weight: 500
                    wrapMode: Text.NoWrap
                    elide: Text.ElideRight
                    width: Math.min(implicitWidth, 900 + 19 + 14)
                    color: root.dateColor
                    text: root.musicPaused
                          ? (root.musicSubline.length ? qsTr("Paused · %1").arg(root.musicSubline) : qsTr("Paused"))
                          : root.musicSubline
                }
            }
        }
    }
    Timer {
        interval: root.driftInterval
        running: root.active
        repeat: true
        onTriggered: root.drift()
    }

    Keys.onPressed: event => {
        event.accepted = true;
        // Play/Pause acts on the music without waking (MH-8).
        if (root.musicKey(event.key, event.isAutoRepeat))
            return;
        if (root.armed)
            root.waking = true;
    }
    // The wake happens on release, so the release never reaches the screen
    // beneath either.
    Keys.onReleased: event => {
        event.accepted = true;
        if (root.waking && root.armed)
            root.wake();
        else
            root.waking = false;
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.AllButtons
        onPressed: mouse => {
            mouse.accepted = true;
            if (root.armed)
                root.wake();
        }
        onWheel: wheel => {
            wheel.accepted = true;
            if (root.armed)
                root.wake();
        }
        onPositionChanged: mouse => {
            if (!root.anchorKnown) {
                root.remember(mouse.x, mouse.y);
                return;
            }
            if (root.armed && (mouse.x !== root.anchorX || mouse.y !== root.anchorY))
                root.wake();
        }
    }
    HoverHandler { id: pointerTracker }
}
