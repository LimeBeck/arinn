<!-- SPDX-FileCopyrightText: 2026 Arinn contributors -->
<!-- SPDX-License-Identifier: CC-BY-SA-4.0 -->

# Phase 3 evidence template

Template for one run of the [Phase 3 runbook](../phase-3-runbook.md) on one
platform. Copy the JSON below to `phase-3-<platform>.json` in this directory,
for example `phase-3-kde-wayland.json` or `phase-3-xfce-x11.json`, and fill it
in. Like the other files here, evidence is JSON; this template is Markdown so
that it can explain its fields.

## Rules

- `null` means "not recorded yet". Replace every `null`; a step that was not
  run gets `"result": "not_run"` and a reason in `notes`.
- `result` is one of `pass`, `fail`, `partial` or `not_run`. A criterion passes
  only when all its steps pass. `partial` needs a note saying what failed.
- No real URLs, page or tab titles, account names, personal paths or
  screenshots of personal pages ([specification §13](../../specifications/arinn-specification.md#13-security)).
  Name a DRM service only by its packaged shortcut name or as "owner
  subscription". Opaque Arinn IDs such as `return_window` are allowed.
- `notes` is short English text: what happened, the input used when it was not
  the remote, timings by eye.

## Fields

| Field | Content |
| --- | --- |
| `date` | Date of the run, `YYYY-MM-DD` |
| `authorization` | Who authorized and ran the operator test |
| `commit` | Full Git commit the package was built from |
| `package_version` | `pacman -Q arinn` |
| `build_id` | `arinnctl --version` |
| `platform` | `desktop` with version, `session` (`wayland` or `x11`), `backend` (`kde` or `ewmh`), `screens` and `resolution` |
| `browser` | `product` (Google Chrome or Chromium), `version`, `extension_id`, and `config` with any non-default `browser_application` or `home_shortcut` |
| `input` | Devices used, for example "G60S Pro remote, USB keyboard" |
| `criteria` | One object per criterion: `result`, `steps` (step ID → `{result, notes}`) and `notes` |
| `window_rules` | The ADR-0050 checks W-1 to W-4, same shape as a criterion |
| `spot_checks` | AC-10 and G-3.1, same shape |
| `not_tested` | What this run did not cover, for example "physical HomePage key" |
| `final_state` | What was left running or removed after the run |

## Template

```json
{
  "date": null,
  "authorization": "Project owner ran the Phase 3 runbook on a disposable session.",
  "commit": null,
  "package_version": null,
  "build_id": null,
  "platform": {
    "desktop": null,
    "session": null,
    "backend": null,
    "screens": null,
    "resolution": null
  },
  "browser": {
    "product": null,
    "version": null,
    "extension_id": "eajlncomibfmpmkhkbcdmajmkpfaaibf",
    "config": {
      "browser_application": "",
      "home_shortcut": "default"
    }
  },
  "input": null,
  "criteria": {
    "AC-03": {
      "result": null,
      "steps": {
        "AC03-1": {"result": null, "seconds_to_page": null, "extra_startup_window": null, "notes": ""},
        "AC03-2": {"result": null, "notes": ""},
        "AC03-3": {"result": null, "notes": ""},
        "AC03-4": {"result": null, "service": null, "protected_playback": null, "notes": ""},
        "AC03-5": {"result": null, "notes": ""}
      },
      "notes": ""
    },
    "AC-04": {
      "result": null,
      "steps": {
        "AC04-1": {"result": null, "notes": ""},
        "AC04-2": {"result": null, "rows": null, "tabs_counted": null, "notes": ""},
        "AC04-3": {"result": null, "notes": ""},
        "AC04-4": {"result": null, "notes": ""},
        "AC04-5": {"result": null, "notes": ""},
        "AC04-6": {"result": null, "notes": ""},
        "AC04-7": {"result": null, "notes": ""}
      },
      "notes": ""
    },
    "AC-05": {
      "result": null,
      "steps": {
        "AC05-1": {"result": null, "page_paused_itself": null, "notes": ""},
        "AC05-2": {"result": null, "notes": ""},
        "AC05-3": {"result": null, "notes": ""},
        "AC05-4": {"result": null, "notes": ""},
        "AC05-5": {"result": null, "notes": ""},
        "AC05-6": {"result": null, "cycles": 10, "passed": null, "notes": ""}
      },
      "notes": ""
    },
    "AC-06": {
      "result": null,
      "steps": {
        "AC06-1": {"result": null, "return_window_matched": null, "notes": ""},
        "AC06-2": {"result": null, "notes": ""},
        "AC06-3": {"result": null, "notes": ""},
        "AC06-4": {"result": null, "notes": ""},
        "AC06-5": {"result": null, "notes": ""},
        "AC06-6": {"result": null, "notes": ""}
      },
      "notes": ""
    }
  },
  "window_rules": {
    "result": null,
    "steps": {
      "W-1": {"result": null, "notes": ""},
      "W-2": {"result": null, "notes": ""},
      "W-3": {"result": null, "notes": ""},
      "W-4": {"result": "not_run", "notes": "optional"}
    },
    "notes": ""
  },
  "spot_checks": {
    "AC-10": {
      "result": null,
      "steps": {
        "AC10-1": {"result": null, "epoch_unchanged": null, "notes": ""},
        "AC10-2": {"result": null, "generation_changed": null, "browser_windows_kept": null, "notes": ""},
        "AC10-3": {"result": null, "seconds_to_tabs": null, "notes": ""},
        "AC10-4": {"result": null, "notes": ""}
      },
      "notes": ""
    },
    "G-3.1": {
      "result": null,
      "steps": {
        "G31-1": {"result": null, "notes": ""},
        "G31-2": {"result": null, "notes": ""},
        "G31-3": {"result": null, "notes": ""}
      },
      "notes": "Native host frame limits are checked only in CI."
    }
  },
  "not_tested": [],
  "final_state": null
}
```
