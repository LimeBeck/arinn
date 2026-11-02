#!/bin/sh
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
#
# Start a dev EWMH stack on the current session bus without systemd user units.
# Use inside an X11 graphical session (for example XFCE) after:
#   cargo build --workspace --locked
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
DATA="${ARINN_DEV_DATA_ROOT:-$ROOT/.dev-ewmh}"
mkdir -p "$DATA/system/arinn/plugins" "$DATA/state/arinn/plugins" "$DATA/config"
for id in chrome demo desktop library mpris mpv transmission; do
    src="$ROOT/plugins/$id/$id.toml"
    dst="$DATA/system/arinn/plugins/$id.toml"
    sed "s|/usr/lib/arinn/plugins/arinn-plugin-$id|$ROOT/target/debug/arinn-plugin-$id|g" \
        "$src" >"$dst"
done
export XDG_DATA_HOME="$DATA/state"
export XDG_CONFIG_HOME="$DATA/config"
export XDG_DATA_DIRS="$DATA/system:/usr/share"
export ARINN_DAEMON="$ROOT/target/debug/arinnd"
export ARINN_PLATFORM_X11="$ROOT/target/debug/arinn-platform-x11"
export ARINN_SHELL_LAUNCH="$ROOT/platform/kde/arinn-shell-launch"
export ARINN_SESSION_BACKEND=x11
export ARINN_PLATFORM_BACKEND=ewmh
export XDG_SESSION_TYPE=x11
export QT_QPA_PLATFORM=xcb
exec "$ROOT/target/debug/arinn-xfce-session"
