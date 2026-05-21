use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}

#[test]
fn rust_index_finds_lib_and_module_definitions() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Compose a small project in-place rather than copying the fixture so this test
    // does not depend on file paths under tests/fixtures.
    let src = tmp.path().join("src");
    std::fs::create_dir(&src).unwrap();
    std::fs::write(
        src.join("lib.rs"),
        "pub mod m;\npub fn alpha() {}\nstruct Beta;\n",
    )
    .unwrap();
    std::fs::write(src.join("m.rs"), "pub fn gamma() {}\n").unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "alpha", "--json", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    // We do not assert refs yet — only that the definition is reported.
    let kinds: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["kind"].as_str().unwrap())
        .collect();
    assert!(
        kinds.contains(&"definition"),
        "expected a definition entry, got: {:?}",
        kinds
    );
}

#[test]
fn find_refs_on_empty_dir_returns_empty_data() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let out = Command::new(bin())
        .args(["find-refs", "Nonexistent", "--json", "--path"])
        .arg(tmp.path())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}
