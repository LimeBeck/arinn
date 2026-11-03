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
    // The size and line count fit() measured, -1 when no step is selected.
    // fit() writes these and never font.pixelSize or maximumLineCount, so
    // the bindings below stay. A later text or font change measures again
    // and the bindings pick up the new values.
    property int fittedPixelSize: -1
    property int fittedLineCount: -1
    // How many times the steps were measured, and with which face. A face
    // that finishes loading after the first measurement changes the family,
    // and the title is measured again. Weight and spacing do too; the size
    // written by the binding does not, or the measurement would recurse.
    property int fitCount: 0
    property string fittedFamily: ""
    property int fittedWeight: -1
    property real fittedLetterSpacing: 0
    property bool fitting: false
    font.family: contentFace ? Theme.contentFontFamily : Theme.fontFamily
    // The role size, unless a fit step is selected. A later role change
    // still applies when no step is selected.
    font.pixelSize: fittedPixelSize >= 0 ? fittedPixelSize : metrics[0]
    // Callers that bind maximumLineCount themselves keep that binding; this
    // one applies when they do not. 2147483647 is Qt's default.
    maximumLineCount: fittedLineCount >= 0 ? fittedLineCount : 2147483647
    font.weight: metrics[1]
    font.letterSpacing: metrics[2]
    // Lining figures for content titles: Ysabeau's default figures are
    // old-style, which a clock or a year must not use.
    font.features: contentFace ? ({ "tnum": 1, "lnum": 1 }) : ({ "tnum": 1 })
    color: Theme.textPrimary
    wrapMode: Text.WordWrap

    // Measures the rendered text at each step. Writing the fitted size and
    // line count updates the bindings, and layout runs before `truncated`
    // is read. Clearing the steps sets both back to -1, which returns the
    // bindings to the role size and the caller's line count.
    function fit() {
        if (fitting)
            return;
        fitting = true;
        fittedFamily = font.family;
        fittedWeight = font.weight;
        fittedLetterSpacing = font.letterSpacing;
        if (!fitSteps || !fitSteps.length) {
            fitStep = -1;
            fittedPixelSize = -1;
            fittedLineCount = -1;
            fitting = false;
            return;
        }
        fitCount += 1;
        for (let i = 0; i < fitSteps.length; ++i) {
            fitStep = i;
            fittedPixelSize = fitSteps[i][0];
            fittedLineCount = fitSteps[i][1];
            if (!truncated)
                break;
        }
        fitting = false;
    }
    onTextChanged: fit()
    onWidthChanged: fit()
    onFitStepsChanged: fit()
    // The weight, the role's or one set on the instance, reaches the
    // variable font's wght axis (#546). A size change made by the binding
    // is the measurement itself, so it does not measure again.
    onFontChanged: {
        Theme.applyWeight(uiText);
        if (fitting || !fitSteps || !fitSteps.length)
            return;
        if (uiText.font.family !== fittedFamily || uiText.font.weight !== fittedWeight
                || uiText.font.letterSpacing !== fittedLetterSpacing)
            fit();
    }
    Component.onCompleted: {
        Theme.applyWeight(uiText);
        fit();
    }
}
