# Debian package: opt-in session autostart (#708)

The `.deb` no longer enables `arinn-session.target` or `arinn-ewmh-session.target` on install. Upgrades from `0.2.0-1` and earlier remove symlinks that prior packages created under `/etc/systemd/user/`.
