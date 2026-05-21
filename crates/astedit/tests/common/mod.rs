#![allow(dead_code)] // not every test consumes every helper

use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Copy `crates/astedit/tests/fixtures/<name>/` into a fresh TempDir
/// and return the TempDir (so the caller controls its lifetime — drop
/// removes the copy).
pub fn copy_fixture(name: &str) -> TempDir {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name);
    assert!(
        src.is_dir(),
        "fixture {:?} not found — did you forget to add it under tests/fixtures/?",
        src,
    );
    let dst = TempDir::new().expect("create tempdir");
    copy_recursive(&src, dst.path());
    dst
}

fn copy_recursive(from: &Path, to: &Path) {
    if !to.exists() {
        fs::create_dir_all(to).expect("mkdir -p tempdir target");
    }
    for entry in fs::read_dir(from).expect("read_dir fixture") {
        let entry = entry.expect("dir entry");
        let kind = entry.file_type().expect("file_type");
        let src = entry.path();
        let dst = to.join(entry.file_name());
        if kind.is_dir() {
            copy_recursive(&src, &dst);
        } else if kind.is_file() {
            fs::copy(&src, &dst).expect("copy fixture file");
        }
        // Symlinks in fixtures are not supported (yagni).
    }
}

/// Invoke the astedit binary built by `cargo test` and capture stdout +
/// exit code. Returns the parsed JSON `data` payload (the helper assumes
/// `--json` and asserts `schema_version == 1`).
pub fn run_astedit_json(args: &[&str]) -> (i32, serde_json::Value) {
    let exe = env!("CARGO_BIN_EXE_astedit");
    let out = std::process::Command::new(exe)
        .args(args)
        .output()
        .expect("spawn astedit");
    let code = out.status.code().unwrap_or(-1);
    let stdout = std::str::from_utf8(&out.stdout).expect("stdout utf8");
    let env: serde_json::Value = serde_json::from_str(stdout)
        .unwrap_or_else(|e| panic!("parse JSON: {e}\nstdout was: {stdout}\nstderr was: {}",
            std::str::from_utf8(&out.stderr).unwrap_or("(non-utf8)")));
    assert_eq!(env["schema_version"], 1, "schema_version must be 1");
    (code, env["data"].clone())
}
