use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multi_lang/rust_app")
}

#[test]
fn callees_of_login_includes_new_user_and_authenticate() {
    let out = Command::new(bin())
        .args(["callees", "login", "--json", "--path"])
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
    assert!(names.contains(&"new_user"), "names: {names:?}");
    assert!(names.contains(&"authenticate"), "names: {names:?}");
}

#[test]
fn callees_of_missing_fn_errors_cleanly() {
    let out = Command::new(bin())
        .args(["callees", "definitely_not_a_function", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    // We treat "no such fn" as success-with-empty rather than non-zero exit,
    // because pipelines that ask "what does X call?" want a clean empty list.
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["data"].as_array().unwrap().len(), 0);
}
