#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Arinn contributors
# SPDX-License-Identifier: GPL-3.0-or-later

"""Compatibility entry point; the private-bus suite now exercises production API."""
import runpy
from pathlib import Path
runpy.run_path(str(Path(__file__).with_name("phase1.py")), run_name="__main__")
