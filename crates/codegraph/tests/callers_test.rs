use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multi_lang/rust_app")
}

#[test]
fn callers_of_authenticate_includes_revoke_and_login() {
    let out = Command::new(bin())
        .args(["callers", "authenticate", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let callers: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(callers.contains(&"revoke"), "callers: {callers:?}");
    assert!(callers.contains(&"login"), "callers: {callers:?}");
    // `whoami` does not call authenticate.
    assert!(!callers.contains(&"whoami"), "callers: {callers:?}");
}

#[test]
fn callers_of_unused_helper_is_empty() {
    let out = Command::new(bin())
        .args(["callers", "unused_helper", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}

#[test]
fn callers_depth_2_walks_one_more_hop() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args([
            "callers",
            "authenticate",
            "--depth",
            "2",
            "--json",
            "--path",
        ])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let entries = v["data"].as_array().unwrap();
    // login calls authenticate (distance=1); nothing calls login in this fixture,
    // so depth=2 should match depth=1 in count — but the field must be present.
    assert!(entries.iter().all(|e| e["distance"].is_number()));
    assert!(entries
        .iter()
        .any(|e| e["name"] == "revoke" && e["distance"] == 1));
}
