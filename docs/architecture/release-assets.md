# Release asset staging

Version releases and master snapshots implement the existing publication contracts
in [ADR-0033](../adr/0033-ci-cd-and-releases.md) and
[ADR-0086](../adr/0086-automatic-master-prereleases.md).

Package CI artifacts contain both distributable files and internal smoke-test
helpers, including `private-systemd`. Publication jobs download these mixed
artifacts into `incoming/`. `scripts/stage-release-assets.py` copies only Arinn
Arch packages, optional Arch debug packages, source archives and Debian packages
into a new `dist/` directory. Selected files must be regular files, never symlinks.
An existing output directory is rejected to prevent stale files from being published.

Staging requires a main Arch package and a source archive. Version releases also
require a Debian package. Smoke helpers and incoming checksum manifests stay out
of `dist/`; internal CI artifacts and their install smoke tests remain unchanged.
The existing publication steps generate `SHA256SUMS` only after staging, so its
entries and the release uploads cover the same distributable files. Snapshot
immutability and post-upload digest verification remain unchanged.
