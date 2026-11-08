// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Canvas helpers shared by artwork slots (Home cards, Downloads posters).
.pragma library

// Clips the context to a w×h rectangle with corner radius r.
function clipRounded(ctx, w, h, r) {
    ctx.beginPath();
    ctx.moveTo(r, 0);
    ctx.lineTo(w - r, 0); ctx.quadraticCurveTo(w, 0, w, r);
    ctx.lineTo(w, h - r); ctx.quadraticCurveTo(w, h, w - r, h);
    ctx.lineTo(r, h); ctx.quadraticCurveTo(0, h, 0, h - r);
    ctx.lineTo(0, r); ctx.quadraticCurveTo(0, 0, r, 0);
    ctx.closePath();
    ctx.clip();
}

// Whether the shell's asset provider decodes this URL. It honours a requested
// size for an Image and for a Canvas copy alike, so a copy loaded at the
// Image's implicit size has exactly that size (#611). Other sources load at
// their own size.
function decodesAtSize(url) {
    return String(url).startsWith("image://arinn-assets/");
}

// The Image sourceSize for artwork shown at w×h: only what the slot covers,
// not a full-resolution poster (#611). Undefined keeps the image's own size.
function sourceSize(url, w, h) {
    return decodesAtSize(url) && w > 0 && h > 0 ? Qt.size(Math.ceil(w), Math.ceil(h)) : undefined;
}

// Loads the Canvas copy of a ready Image at the Image's decoded size.
function loadCopy(canvas, image) {
    if (decodesAtSize(image.source))
        canvas.loadImage(image.source, Qt.size(image.implicitWidth, image.implicitHeight));
    else
        canvas.loadImage(image.source);
}

// Draws the Canvas copy over w×h with a centre crop, as
// Image.PreserveAspectCrop does. iw/ih are retained for existing callers, but
// belong to a separate Image and can already describe another decoded size.
// Snapshot the loaded Canvas image so both crop dimensions and drawn pixels
// come from the same copy (#716). No fetch or decode is started here.
function drawCover(ctx, image, iw, ih, w, h) {
    if (!(w > 0 && h > 0))
        return false;
    const copy = ctx.createImageData(String(image));
    iw = copy.width;
    ih = copy.height;
    if (!(iw > 0 && ih > 0))
        return false;
    const scale = Math.max(w / iw, h / ih);
    const sw = w / scale, sh = h / scale;
    ctx.drawImage(copy, (iw - sw) / 2, (ih - sh) / 2, sw, sh, 0, 0, w, h);
    return true;
}
