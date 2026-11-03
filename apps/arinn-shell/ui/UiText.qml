// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

import QtQuick

Text {
    id: uiText
    // Named roles keep action labels at least 26 logical pixels.
    // [pixel size, weight, letter spacing, content face]. Content titles use
    // Ysabeau; everything else stays Onest (ADR-0136, D-2). Eyebrows are
    // sentence case: the role does not space letters out for capitals.
    property string role: "body"
    readonly property var typography: ({
        display: [220, 600, 0, true], hero: [96, 600, 0, true],
        detailTitle: [104, 600, 0, true],
        title: [56, 600, 0, false], section: [30, 600, 0, false],
        body: [30, 400, 0, false], control: [30, 600, 0, false],
        meta: [26, 500, 0, false], eyebrow: [26, 500, 0, false], caption: [24, 500, 0, false]
    })
    readonly property var metrics: typography[role] || typography.body
    readonly property bool contentFace: metrics[3] === true
    // Steps for a content title, largest first: [[pixel size, max lines], …].
    // The first step whose rendered text is not elided wins; the last one
    // elides. Empty keeps the role's size and the instance's line count.
    property var fitSteps: []
    // The step in use, -1 without fitSteps.
    property int fitStep: -1
    // How many times the steps were measured, and with which family. A face
    // that finishes loading after the first measurement changes the family,
    // and the title is measured again.
    property int fitCount: 0
    property string fittedFamily: ""
    font.family: contentFace ? Theme.contentFontFamily : Theme.fontFamily
    font.pixelSize: metrics[0]
    font.weight: metrics[1]
    font.letterSpacing: metrics[2]
    // Lining figures for content titles: Ysabeau's default figures are
    // old-style, which a clock or a year must not use.
    font.features: contentFace ? ({ "tnum": 1, "lnum": 1 }) : ({ "tnum": 1 })
    color: Theme.textPrimary
    wrapMode: Text.WordWrap

    // Measures the rendered text at each step. Layout runs when the size or
    // the line count changes, so `truncated` is current when it is read.
    function fit() {
        if (!fitSteps || !fitSteps.length)
            return;
        fitCount += 1;
        fittedFamily = font.family;
        for (let i = 0; i < fitSteps.length; ++i) {
            fitStep = i;
            font.pixelSize = fitSteps[i][0];
            maximumLineCount = fitSteps[i][1];
            if (!truncated)
                return;
        }
    }
    onTextChanged: fit()
    onWidthChanged: fit()
    onFitStepsChanged: fit()
    // The weight, the role's or one set on the instance, reaches the
    // variable font's wght axis (#546). fit() changes only the size, so the
    // family check cannot recurse.
    onFontChanged: {
        Theme.applyWeight(uiText);
        if (fitSteps && fitSteps.length && uiText.font.family !== fittedFamily)
            fit();
    }
    Component.onCompleted: {
        Theme.applyWeight(uiText);
        fit();
    }
}
