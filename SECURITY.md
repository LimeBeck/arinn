# Security policy

## Supported versions

Arinn is pre-release software (Phase 0 of the [roadmap](ROADMAP.md) is complete; there is no released version yet). Security fixes are made on the `master` branch only.

## Security model

Arinn's plugins are **trusted, locally installed software**; Arinn is not a sandbox for hostile plugins. The architecture still enforces process isolation, IPC validation, bounded messages, timeouts, no shell execution from QML, and no page-content access for the Chrome extension ([specification §13](docs/specifications/arinn-specification.md#13-security)).

In scope, for example:

- a way for another local process or a web page to make Arinn execute commands, open arbitrary schemes or files, or read browser data;
- bypassing plugin registration checks (token, UID, cgroup) on the session bus;
- Arinn showing its UI over the lock screen, or leaking secrets, URLs or page content into logs;
- memory safety bugs or crashes triggered by malformed IPC messages.

Out of scope: a malicious plugin that the user installed themselves, and vulnerabilities in Chrome, KDE, Qt, mpv or Transmission themselves (report those upstream).

## Reporting a vulnerability

Please **do not open a public issue.** Report privately through GitHub: [Report a vulnerability](https://github.com/LimeBeck/arinn/security/advisories/new) (repository → Security → Advisories).

Include the affected component, steps to reproduce, the impact you expect and your environment (distribution, KDE/Qt versions). You will receive an acknowledgement within 7 days. Please allow time for a fix before public disclosure; we will agree on a disclosure date with you and credit you unless you prefer otherwise.
