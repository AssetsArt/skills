use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codemap"))
}
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn find_substring_matches_across_languages() {
    let out = Command::new(bin())
        .args(["find", "User", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let names: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"User"));
    assert!(names.contains(&"UserRepo"));
    assert!(names.contains(&"findUser"));
}

#[test]
fn find_exact_only_returns_exact_name() {
    let out = Command::new(bin())
        .args(["find", "User", "--exact", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let arr = v["data"].as_array().unwrap();
    for e in arr {
        assert_eq!(e["name"].as_str().unwrap(), "User");
    }
    assert!(!arr.is_empty());
}
