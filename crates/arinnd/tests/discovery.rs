// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("arinn-discovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(path.join("data/arinn/plugins")).unwrap();
        Self(path)
    }
    fn command(&self) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_arinnd"));
        cmd.env_clear()
            .env("HOME", &self.0)
            .env("XDG_DATA_HOME", self.0.join("data"))
            .env("XDG_DATA_DIRS", self.0.join("system"));
        cmd
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn offline_check_needs_no_bus_or_launch_token_and_never_executes_plugins() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let executable = f.0.join("must-not-run");
    fs::write(&executable, "#!/bin/sh\ntouch marker\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let manifest = f.0.join("data/arinn/plugins/test.toml");
    fs::write(&manifest, format!("id = 'test'\nname = 'Test'\nversion = '1'\napi_version = 1\nexecutable = {:?}\nargs = ['private-argument']\ncapabilities = ['application']\n", executable.to_str().unwrap())).unwrap();
    let output = f
        .command()
        .current_dir(&f.0)
        .arg("--check-plugins")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(json["providers"]["test"]["error"].is_null());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("private-argument"));
    assert!(!f.0.join("marker").exists());
    fs::write(&manifest, "id = 'test'\nname = 42\n").unwrap();
    let output = f.command().arg("--check-plugins").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["providers"]["test"]["error"], "InvalidArgument");
    assert!(output.stderr.is_empty());
}

#[test]
fn unknown_arguments_fail_before_connecting_to_the_bus() {
    let f = Fixture::new();
    let output = f.command().arg("--unknown").output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage:"));
}
