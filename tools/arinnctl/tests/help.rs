// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::process::{Command, Output};
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arinnctl"))
        .args(args)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "deliberately-invalid-test-address",
        )
        .output()
        .expect("run CLI with unavailable bus")
}
#[test]
fn help_succeeds_offline_on_stdout_for_root_groups_and_commands() {
    for args in [
        vec!["--help"],
        vec!["-h"],
        vec!["help"],
        vec!["--json", "--help"],
        vec!["provider", "--help"],
        vec!["providers", "-h"],
        vec!["shortcuts", "--help"],
        vec!["browser", "--help"],
        vec!["help", "browser", "activate"],
        vec!["shortcuts", "edit", "--help"],
        vec!["status", "--help"],
        vec!["translations", "--help"],
        vec!["translations", "export-provider", "--help"],
        vec!["help", "translations", "compile"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(0), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{args:?}: {output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.starts_with("Usage: arinnctl "));
        assert!(text.contains("Exit codes:"));
        assert!(!text.contains("Try 'arinnctl"));
    }
    let output = cli(&["--help"]);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("activities"));
    assert!(text.contains("Commands:") && text.contains("Examples:"));
    let text = String::from_utf8(cli(&["browser", "activate", "--help"]).stdout).unwrap();
    assert!(text.contains("browser activate <provider> <tab>"));
    assert!(text.contains("Tab ID from"));
}

#[test]
fn shell_translation_export_and_check_work_offline_and_preserve_edits() {
    let root = std::env::temp_dir().join(format!(
        "arinn-translation-offline-{}",
        uuid::Uuid::new_v4()
    ));
    let source = root.join("arinn_de.ts");
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_arinnctl"))
            .args(args)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                "deliberately-invalid-test-address",
            )
            .output()
            .unwrap()
    };
    std::fs::create_dir(&root).unwrap();
    let output = run(&[
        "--json",
        "translations",
        "export-shell",
        "de",
        "arinn",
        source.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let before = std::fs::read(&source).unwrap();
    let check = run(&["--json", "translations", "check", source.to_str().unwrap()]);
    assert_eq!(check.status.code(), Some(0), "{check:?}");
    let report: serde_json::Value = serde_json::from_slice(&check.stdout).unwrap();
    assert_eq!(report["locale"], "de");
    assert_eq!(report["finished"], 0);
    assert!(report["unfinished"].as_u64().unwrap() > 100);
    assert_eq!(
        run(&[
            "translations",
            "export-shell",
            "de",
            "arinn",
            source.to_str().unwrap()
        ])
        .status
        .code(),
        Some(1)
    );
    assert_eq!(std::fs::read(&source).unwrap(), before);
    let malformed = root.join("malformed.ts");
    std::fs::write(
        &malformed,
        "<TS language='de'><unexpected\u{1b}]52;test/></TS>",
    )
    .unwrap();
    let rejected = run(&["translations", "check", malformed.to_str().unwrap()]);
    assert_eq!(rejected.status.code(), Some(1));
    assert!(!rejected.stderr.contains(&0x1b));
    assert_eq!(
        run(&["translations", "export-provider", "radio", "de"])
            .status
            .code(),
        Some(3)
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn invalid_usage_has_code_two_and_concise_stderr_guidance() {
    for args in [
        vec!["unknown"],
        vec!["browser"],
        vec!["browser", "unknown", "--help"],
        vec!["help", "unknown"],
        vec!["provider", "restart"],
        vec!["shortcuts", "add", "Title"],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(2), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        let error = String::from_utf8(output.stderr).unwrap();
        assert!(error.contains("Invalid usage") && error.contains("--help"));
        assert!(error.lines().count() <= 2);
    }
}
#[test]
fn default_command_and_literal_help_values_still_require_the_daemon() {
    for args in [
        vec![],
        vec!["--json"],
        vec!["status"],
        vec!["activities"],
        vec!["activate", "synthetic-provider", "--help"],
        vec!["shortcuts", "add", "--help", "https://example.org/"],
        vec![
            "shortcuts",
            "edit",
            "synthetic-id",
            "Title",
            "https://example.org/",
            "-h",
        ],
    ] {
        let output = cli(&args);
        assert_eq!(output.status.code(), Some(3), "{args:?}: {output:?}");
        assert!(output.stdout.is_empty());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("Daemon unavailable")
        );
    }
}
#[test]
fn version_prints_the_build_id_offline() {
    for flag in ["--version", "-V"] {
        let output = cli(&[flag]);
        assert_eq!(output.status.code(), Some(0), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let id = text
            .trim()
            .strip_prefix("arinnctl ")
            .expect("arinnctl <build id>");
        assert_eq!(
            arinn_protocol::build::validate(env!("CARGO_PKG_VERSION"), id),
            Ok(()),
            "{id}"
        );
    }
}
