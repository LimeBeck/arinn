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

// Draws an iw×ih image over w×h with a centre crop, as
// Image.PreserveAspectCrop does. Returns false when there is nothing to draw.
function drawCover(ctx, image, iw, ih, w, h) {
    if (!(iw > 0 && ih > 0 && w > 0 && h > 0))
        return false;
    const scale = Math.max(w / iw, h / ih);
    const sw = w / scale, sh = h / scale;
    ctx.drawImage(image, (iw - sw) / 2, (ih - sh) / 2, sw, sh, 0, 0, w, h);
    return true;
}
