// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import "../../apps/arinn-shell/ui" as Ui

Ui.HomeView {
    continueEntries: [
        {id: "first", title: "Fixture activity", running: true},
        {id: "second", title: "Fixture content"}
    ]
    favoriteEntries: [{id: "favorite", title: "Fixture favorite", monogram: "F"}]
    applicationEntries: [{id: "application", title: "Fixture application", group: "other"}]
}
