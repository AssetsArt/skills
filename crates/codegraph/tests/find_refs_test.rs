use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multi_lang/rust_app")
}

fn run_find_refs(name: &str) -> serde_json::Value {
    let out = Command::new(bin())
        .args(["find-refs", name, "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("json")
}

#[test]
fn cross_file_call_to_imported_fn_is_high_confidence() {
    let v = run_find_refs("authenticate");
    let hits = v["data"].as_array().unwrap();
    let cross_file_call = hits
        .iter()
        .find(|h| {
            h["file"].as_str().unwrap().ends_with("handlers.rs")
                && h["kind"].as_str().unwrap() == "call"
        })
        .expect("handlers.rs should contain a call to authenticate");
    assert_eq!(cross_file_call["confidence"].as_str().unwrap(), "high");
    assert_eq!(
        cross_file_call["reason"].as_str().unwrap(),
        "import-resolved"
    );
}

#[test]
fn same_file_call_is_high_confidence() {
    let v = run_find_refs("authenticate");
    let hits = v["data"].as_array().unwrap();
    let same_file_call = hits
        .iter()
        .find(|h| {
            h["file"].as_str().unwrap().ends_with("auth.rs")
                && h["kind"].as_str().unwrap() == "call"
        })
        .expect("auth.rs should call authenticate from revoke");
    assert_eq!(same_file_call["confidence"].as_str().unwrap(), "high");
    assert_eq!(
        same_file_call["reason"].as_str().unwrap(),
        "same-file-scope"
    );
}

#[test]
fn unused_helper_has_definition_but_no_calls() {
    let v = run_find_refs("unused_helper");
    let hits = v["data"].as_array().unwrap();
    assert!(hits.iter().any(|h| h["kind"] == "definition"));
    assert!(
        !hits.iter().any(|h| h["kind"] == "call"),
        "unused_helper should not have any call sites, got: {hits:?}"
    );
}

fn ts_fixture() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/multi_lang/ts_app")
}

#[test]
fn ts_cross_file_import_is_high_confidence() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_codegraph"))
        .args(["find-refs", "authenticate", "--json", "--path"])
        .arg(ts_fixture())
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let cross = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["file"].as_str().unwrap().ends_with("handlers.ts") && h["kind"] == "call")
        .expect("handlers.ts call to authenticate");
    assert_eq!(cross["confidence"], "high");
    assert_eq!(cross["reason"], "import-resolved");
}
