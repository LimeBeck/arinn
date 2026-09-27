#!/usr/bin/env python3

# SPDX-FileCopyrightText: 2026 Arinn contributors
#
# SPDX-License-Identifier: GPL-3.0-or-later

"""Re-render the exported Activity snapshot in a separate Qt process."""
import json
import subprocess
import tempfile
from pathlib import Path
ROOT = Path(__file__).resolve().parents[2]
source = ROOT/'.phase0/background-probe/raw.png'
output = ROOT/'.phase0/background-probe/external-shell.png'
assert source.is_file(), 'Run kwin_background.py first'
output.unlink(missing_ok=True)
qml = (ROOT/'tests/ui/background-preview.qml.in').read_text()
qml = qml.replace('@COMPONENT_DIR@', (ROOT/'apps/arinn-shell').as_uri())
qml = qml.replace('@SOURCE@', json.dumps(source.as_uri())).replace('@OUTPUT@', json.dumps(str(output)))
with tempfile.TemporaryDirectory(prefix='arinn-background-preview-') as folder:
    file = Path(folder)/'main.qml'
    file.write_text(qml)
    subprocess.run(['qml6', str(file)], check=True, timeout=15)
assert output.is_file(), 'Qt did not export the rendered frame'
print('Rendered outside KWin:', output)
