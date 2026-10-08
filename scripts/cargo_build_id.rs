// SPDX-FileCopyrightText: 2026 Arinn contributors
// SPDX-License-Identifier: GPL-3.0-or-later

// Shared by the build scripts of the binaries (`include!`d, ADR-0097): sets
// `ARINN_BUILD_ID` from scripts/build_id.py, so Rust binaries and the Qt shell
// of one build report the same id.

/// Embed the build id as the compile-time variable `ARINN_BUILD_ID`.
fn emit_build_id() {
    use std::{path::PathBuf, process::Command};
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets it"));
    let root = manifest
        .ancestors()
        .find(|dir| dir.join("scripts/build_id.py").is_file())
        .expect("the workspace contains scripts/build_id.py")
        .to_path_buf();
    let script = root.join("scripts/build_id.py");
    let python = std::env::var("PYTHON").unwrap_or_else(|_| "python3".into());
    println!("cargo:rerun-if-env-changed=ARINN_BUILD_ID");
    println!("cargo:rerun-if-env-changed=PYTHON");
    println!("cargo:rerun-if-changed={}", script.display());
    let version = std::env::var("CARGO_PKG_VERSION").expect("cargo sets it");
    let run = |args: &[&str]| Command::new(&python).arg(&script).args(args).output();
    let id = match run(&[]) {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        // An explicit but malformed ARINN_BUILD_ID is a broken build input.
        Ok(out) => panic!(
            "scripts/build_id.py failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(error) => {
            println!("cargo:warning=no build id ({python}: {error}); using {version}");
            version
        }
    };
    if let Ok(out) = run(&["--watch"]) {
        for path in String::from_utf8_lossy(&out.stdout).lines() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    println!("cargo:rustc-env=ARINN_BUILD_ID={id}");
}
