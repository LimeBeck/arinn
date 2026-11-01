<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Managed video decoding preference

The mpv provider exports the existing ProviderSettings1 schema with the enum
`hardware_decoding` in Settings › Playback: `auto` or `no` (default). The daemon
stores it in provider settings in `config.toml` and delivers it through the
existing validated startup/live settings mechanism (ADR-0064/0065).

```toml
[providers.mpv.settings]
hardware_decoding = "auto"
```

Use the configured provider instance ID if it differs from `mpv`. The screen's
generic enum editor changes the same value without editing TOML. Apply updates
a shared preference immediately but sends no player command; the next Open
includes `hwdec` in the per-file options with `start`, `alang` and `slang`, using
the same options for modern/legacy fallback. It also works with reused/restored
players, without needing a new process or changing current playback. Automatic
chooses a supported decoder and keeps software fallback. External player config
and scripts remain disabled. See [ADR-0127](../adr/0127-configurable-video-decoding.md)
(proposed; product specification change awaits owner review).

