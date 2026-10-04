#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later
"""Apply auditable privacy guards to the pinned Sentry native source."""
from pathlib import Path
import sys

root = Path(sys.argv[1]) / "src"
patches = [
    ("backends/sentry_backend_inproc.c", "        bool should_handle = true;",
     "        bool should_handle = !skip_hooks; /* Arinn: drop hook-bypass crashes. */"),
    ("sentry_transport.c", "#include \"sentry_sync.h\"",
     '#include "sentry_sync.h"\n\n/* Arinn privacy boundary: owns and rebuilds the envelope before queue/cache. */\nextern sentry_envelope_t *arinn_sentry_filter_envelope(sentry_envelope_t *envelope);'),
    ("sentry_transport.c", "    if (sentry__atomic_fetch(&transport->suspended)) {\n        sentry__run_write_envelope",
     "    envelope = arinn_sentry_filter_envelope(envelope);\n    if (!envelope) {\n        return;\n    }\n    if (sentry__atomic_fetch(&transport->suspended)) {\n        sentry__run_write_envelope"),
]
updates = {}
for name, original, replacement in patches:
    path = root / name
    source = updates.get(path, path.read_text())
    if source.count(replacement) == 1:
        continue
    if source.count(original) != 1:
        raise SystemExit(f"Unsupported Sentry source: privacy anchor changed in {name}")
    updates[path] = source.replace(original, replacement)
# Validate every anchor before changing any source; repeated configuration is inert.
for path, source in updates.items():
    path.write_text(source)
