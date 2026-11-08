// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtTest
import "../../apps/arinn-shell/ui" as Ui

TestCase {
    id: suite
    name: "SharedIcons"
    width: 400; height: 300
    visible: true
    when: windowShown
    Component {
        id: factory
        Rectangle {
            width: 96; height: 96; color: "black"
            property alias name: icon.name
            property alias variant: icon.variant
            property alias ink: icon.color
            Ui.LineIcon { id: icon; anchors.fill: parent; color: "white" }
        }
    }
    function test_shared_symbols_render_data() {
        return ["video_library", "play_circle", "volume_up", "volume_off", "wifi", "apps", "palette", "memory", "power", "bedtime"]
            .map(name => ({tag: name, name: name}));
    }
    function test_shared_symbols_render(data) {
        const board = createTemporaryObject(factory, suite, {name: data.name, variant: "filled"});
        tryVerify(() => {
            const pixels = grabImage(board);
            let lit = 0;
            for (let y = 0; y < 96; y += 2)
                for (let x = 0; x < 96; x += 2)
                    if (pixels.red(x, y) > 128) lit++;
            return lit > 10 && lit < 1800;
        });
    }
    function test_filled_and_line_variants_and_cutouts() {
        const board = createTemporaryObject(factory, suite, {name: "apps", variant: "filled"});
        tryVerify(() => grabImage(board).red(48, 48) > 200);
        board.variant = "line";
        tryVerify(() => grabImage(board).red(48, 48) === 0);
        board.name = "play_circle";
        tryVerify(() => {
            const p = grabImage(board);
            return p.red(28, 48) > 200 && p.red(48, 48) === 0;
        });
        board.name = "memory";
        tryVerify(() => grabImage(board).red(48, 48) === 0);
        board.variant = "filled";
        board.name = "unknown-category";
        tryVerify(() => grabImage(board).red(48, 48) > 200);
        board.ink = "red";
        tryVerify(() => grabImage(board).red(48, 48) > 200 && grabImage(board).green(48, 48) === 0);
    }
}
