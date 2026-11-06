// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick
import QtQuick.Window
import QtTest
import "../../apps/arinn-shell/ui" as Ui
import "../../apps/arinn-shell/ui/screens/settings" as Settings

TestCase {
    id: suite
    name: "SettingsAbout"
    width: 1920
    height: 1080
    visible: true
    when: windowShown
    property var view
    property int copies: 0
    property bool copiedPrivate: false
    property int lastCopyCode: -1

    Component {
        id: factory
        Settings.SettingsView {
            categories: [
                { id: "media", title: "Media", icon: "video_library" },
                { id: "playback", title: "Playback", icon: "play_circle" },
                { id: "sound", title: "Sound", icon: "volume_up" },
                { id: "network", title: "Network", icon: "wifi" },
                { id: "applications", title: "Applications", icon: "apps" },
                { id: "appearance", title: "Appearance", icon: "palette" },
                { id: "system", title: "System", icon: "memory" }
            ]
            rows: [
                { providerId: "library", providerName: "Library", id: "scan", categoryId: "media",
                  title: "Scan folders", description: "Look for new files", type: "boolean",
                  currentValue: false, constraints: {}, enabled: true },
                { providerId: "shell", providerName: "Shell", id: "idle", categoryId: "system",
                  title: "Idle minutes", description: "Wait before the ambient screen", type: "integer",
                  currentValue: 10, constraints: { min: 1, max: 60 }, enabled: true }
            ]
        }
    }

    function child(name) {
        const item = findChild(view, name);
        verify(item !== null, name);
        return item;
    }
    function maybe(name) { return findChild(view, name); }
    function focused(name) {
        tryCompare(child(name), "activeFocus", true, 5000, name);
    }
    function down(steps) {
        for (let i = 0; i < steps; ++i)
            keyClick(Qt.Key_Down);
    }
    function russian() {
        return (typeof aboutLocale === "string" && aboutLocale === "ru")
               || (maybe("about-title-shell") && maybe("about-title-shell").text === "Версия Arinn");
    }
    function either(actual, en, ru) {
        compare(actual, russian() ? ru : en);
    }
    function facts(extra) {
        const row = {
            shellBuildId: "0.2.0+g8fc320b8e67f",
            shellShort: "0.2.0 · 8fc320b8e67f",
            daemonAvailable: true,
            daemonBuildId: "0.2.0+g8fc320b8e67f",
            daemonShort: "0.2.0 · 8fc320b8e67f",
            daemonMismatch: false,
            channelDate: russian() ? "master · 3 ноября 2026" : "master · Nov 3, 2026",
            providerDetail: "Library 0.2.0 · Downloads 0.2.0 · Browser 0.2.0",
            providerCount: 3,
            osName: "Arch Linux",
            kernel: "6.12.113",
            plasma: "6.4.5",
            qtVersion: "6.8.3",
            session: "KDE · Wayland",
            display: russian() ? "1920 × 1080 · 60 Гц" : "1920 × 1080 · 60 Hz",
            audio: "HDMI 1",
            connection: "Ethernet · enp3s0",
            ipAddress: "203.0.113.24",
            storage: russian() ? "412 ГБ из 931 ГБ" : "412 GB of 931 GB",
            deviceName: "Living room",
            fileName: "arinn-about-2026-11-03-1900.txt",
            copyReport: function(flag) {
                suite.copies += 1;
                suite.copiedPrivate = !!flag;
                suite.lastCopyCode = 0;
                return 0;
            }
        };
        if (extra) {
            const keys = Object.keys(extra);
            for (let i = 0; i < keys.length; ++i)
                row[keys[i]] = extra[keys[i]];
        }
        return row;
    }
    function createView() {
        mouseMove(suite, 10, 10);
        copies = 0;
        copiedPrivate = false;
        lastCopyCode = -1;
        view = createTemporaryObject(factory, suite);
        verify(view);
        if (typeof aboutLocale === "string" && aboutLocale === "ru") {
            view.categories = [
                { id: "media", title: "Медиа", icon: "video_library" },
                { id: "playback", title: "Воспроизведение", icon: "play_circle" },
                { id: "sound", title: "Звук", icon: "volume_up" },
                { id: "network", title: "Сеть", icon: "wifi" },
                { id: "applications", title: "Приложения", icon: "apps" },
                { id: "appearance", title: "Оформление", icon: "palette" },
                { id: "system", title: "Система", icon: "memory" }
            ];
        }
        focused("category-media");
    }
    function init() { createView(); }
    function cleanup() { view = null; }
    function openAbout() {
        view.about = facts();
        down(6);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Down);
        focused("about-copy");
    }
    function saveFrame(name) {
        if (typeof captureAboutDir !== "string" || !String(captureAboutDir).length)
            return;
        const prefix = russian() ? "ru-" : "en-";
        let saved = false;
        view.grabToImage(function(result) {
            saved = result.saveToFile(String(captureAboutDir) + "/" + prefix + name + ".png");
        });
        tryVerify(function() { return saved; }, 8000);
    }

    function test_copy_is_first_and_focus_shows_the_build_id() {
        openAbout();
        compare(child("about-title-copy").text.length > 0, true);
        either(child("about-title-shell").text, "Arinn version", "Версия Arinn");
        compare(maybe("about-build-shell"), null);
        keyClick(Qt.Key_Down);
        focused("about-shell");
        compare(child("about-value-shell").text, "0.2.0 · 8fc320b8e67f");
        tryCompare(child("about-build-shell"), "text",
                   (russian() ? "Сборка " : "Build ") + "0.2.0+g8fc320b8e67f");
        const focusedHeight = child("about-shell").height;
        verify(focusedHeight > 88);
        keyClick(Qt.Key_Return);
        compare(child("aboutSheet").visible, false);
        compare(copies, 0);
        compare(maybe("settingsHint-1"), null);
        verify(child("settingsHints").visible);
        const captions = [];
        function collect(item) {
            if (!item)
                return;
            if (item.text !== undefined && String(item.text).length)
                captions.push(String(item.text));
            const kids = item.children || [];
            for (let i = 0; i < kids.length; ++i)
                collect(kids[i]);
        }
        collect(child("settingsHints"));
        verify(captions.indexOf(russian() ? "Назад" : "Back") >= 0);
        verify(captions.indexOf("OK") < 0);
        verify(captions.indexOf("Edit") < 0);
        verify(captions.indexOf("Изменить") < 0);
        keyClick(Qt.Key_Up);
        focused("about-copy");
        compare(maybe("about-build-shell"), null);
        saveFrame("version");
        keyClick(Qt.Key_Down);
        focused("about-shell");
        saveFrame("version-focus");
    }

    function test_mismatch_marks_the_daemon_row() {
        view.about = facts({
            daemonBuildId: "0.2.0+g4676886d1e20",
            daemonShort: "0.2.0 · 4676886d1e20",
            daemonMismatch: true
        });
        down(6);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("about-daemon");
        compare(child("about-value-daemon").text, "• 0.2.0 · 4676886d1e20");
        verify(Qt.colorEqual(child("about-value-daemon").color, Ui.Theme.error));
        either(child("about-mismatch-daemon").text,
               "Different build from Arinn. Restart Arinn",
               "Другая сборка, чем у Arinn. Перезапустите Arinn");
        verify(Qt.colorEqual(child("about-mismatch-daemon").color, Ui.Theme.error));
        compare(child("about-value-shell").text, "0.2.0 · 8fc320b8e67f");
        either(child("about-value-channel").text, "master · Nov 3, 2026", "master · 3 ноября 2026");
        compare(child("about-value-providers").text, "3");
        compare(child("about-detail-providers").text,
               "Library 0.2.0 · Downloads 0.2.0 · Browser 0.2.0");
        tryCompare(child("about-build-daemon"), "text",
                   (russian() ? "Сборка " : "Build ") + "0.2.0+g4676886d1e20");
        verify(!Qt.colorEqual(child("about-build-daemon").color, Ui.Theme.error));
        saveFrame("mismatch");
    }

    function test_unavailable_keeps_about_and_covers_other_sections() {
        view.about = facts({
            daemonAvailable: false,
            daemonBuildId: "",
            daemonShort: "",
            daemonMismatch: false,
            providerDetail: "",
            providerCount: -1
        });
        view.screenState = "unavailable";
        tryCompare(child("screenStatus"), "visible", true);
        compare(child("category-media").parent.visible, true);
        compare(child("category-media").focus, true);
        down(6);
        focused("category-system");
        tryCompare(child("screenStatus"), "visible", false);
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Down);
        focused("about-daemon");
        either(child("about-value-daemon").text, "Unavailable", "Недоступно");
        compare(child("about-value-daemon").color.toString(), Ui.Theme.textSecondary.toString());
        either(child("about-detail-daemon").text,
               "The arinnd service is not responding",
               "Служба arinnd не отвечает");
        compare(child("about-value-shell").text, "0.2.0 · 8fc320b8e67f");
        either(child("about-value-providers").text, "Unavailable", "Недоступно");
        compare(maybe("about-detail-providers"), null);
        compare(child("about-value-os").text, "Arch Linux");
        saveFrame("no-daemon");
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        focused("about-copy");
        keyClick(Qt.Key_Return);
        tryCompare(child("aboutPrivacy"), "activeFocus", true);
        keyClick(Qt.Key_Down);
        focused("aboutCopyButton");
        keyClick(Qt.Key_Return);
        compare(copies, 1);
        compare(copiedPrivate, false);
        tryCompare(child("settingsToastTitle"), "text",
                   (russian() ? "Скопировано. Файл: Загрузки/" : "Copied. File: Downloads/")
                   + "arinn-about-2026-11-03-1900.txt");
        keyClick(Qt.Key_Up);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Left);
        focused("category-system");
        keyClick(Qt.Key_Up);
        focused("category-appearance");
        tryCompare(child("screenStatus"), "visible", true);
    }

    function test_copy_sheet_defaults_to_private_off() {
        openAbout();
        keyClick(Qt.Key_Return);
        tryCompare(child("aboutSheet"), "visible", true);
        tryCompare(child("aboutPrivacy"), "activeFocus", true);
        compare(child("aboutPrivacyKnob").x, 5);
        either(child("aboutExcluded").text,
               "No IP address or device name",
               "Без IP-адреса и имени устройства");
        either(child("aboutPrivacyDetail").text,
               "IP address, connection and name. Off — left out",
               "IP-адрес, подключение и имя. Выключено — их нет в сведениях");
        verify(child("aboutFile").text.indexOf("arinn-about-2026-11-03-1900.txt") >= 0);
        saveFrame("sheet");
        keyClick(Qt.Key_Return);
        compare(child("aboutPrivacyKnob").x, 43);
        either(child("aboutExcluded").text,
               "IP address, connection and device name",
               "IP-адрес, подключение и имя устройства");
        keyClick(Qt.Key_Down);
        focused("aboutCopyButton");
        keyClick(Qt.Key_Return);
        compare(copies, 1);
        compare(copiedPrivate, true);
        compare(child("aboutSheet").visible, false);
        focused("about-copy");
        keyClick(Qt.Key_Return);
        tryCompare(child("aboutPrivacyKnob"), "x", 5);
        keyClick(Qt.Key_Back);
        compare(copies, 1);
        compare(child("aboutSheet").visible, false);
        focused("about-copy");
    }

    function test_network_storage_and_unknown() {
        view.about = facts({ plasma: "" });
        down(6);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Down);
        // copy, then walk to the IP row: shell daemon channel providers
        // os kernel plasma qt session display audio connection ip
        for (let i = 0; i < 13; ++i)
            keyClick(Qt.Key_Down);
        focused("about-ip");
        compare(child("about-value-ip").text, "203.0.113.24");
        compare(child("about-value-connection").text, "Ethernet · enp3s0");
        either(child("about-value-storage").text, "412 GB of 931 GB", "412 ГБ из 931 ГБ");
        either(child("about-detail-storage").text,
               "Disk with the Downloads folder",
               "Диск с папкой «Загрузки»");
        either(child("about-value-plasma").text, "Unknown", "Неизвестно");
        compare(child("about-value-plasma").color.toString(), Ui.Theme.textHint.toString());
        saveFrame("network");
        keyClick(Qt.Key_Left);
        focused("category-system");
    }

    function test_about_opens_with_no_system_rows() {
        view.rows = [
            { providerId: "library", providerName: "Library", id: "scan", categoryId: "media",
              title: "Scan folders", description: "Look for new files", type: "boolean",
              currentValue: false, constraints: {}, enabled: true }
        ];
        view.about = facts();
        down(6);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("about-copy");
        compare(maybe("setting-shell-idle"), null);
    }

    function test_without_about_the_system_list_ends_on_its_row() {
        down(6);
        focused("category-system");
        keyClick(Qt.Key_Right);
        focused("setting-shell-idle");
        keyClick(Qt.Key_Down);
        focused("setting-shell-idle");
        compare(maybe("about-copy"), null);
    }
}
