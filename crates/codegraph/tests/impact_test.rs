use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multi_lang/rust_app")
}

#[test]
fn impact_of_authenticate_includes_login_and_revoke() {
    let out = Command::new(bin())
        .args(["impact", "authenticate", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let names: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"authenticate"));
    assert!(names.contains(&"login"));
    assert!(names.contains(&"revoke"));
}

#[test]
fn impact_of_user_struct_includes_type_position_uses() {
    let out = Command::new(bin())
        .args(["impact", "User", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    // Every fn that takes `&User` should appear (authenticate, revoke, whoami).
    let names: Vec<&str> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["name"].as_str().unwrap())
        .collect();
    for fname in ["authenticate", "revoke", "whoami"] {
        assert!(
            names.contains(&fname),
            "impact of User missing {fname}: {names:?}"
        );
    }
}
