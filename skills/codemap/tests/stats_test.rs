use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf { PathBuf::from(env!("CARGO_BIN_EXE_codemap")) }
fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn stats_json_returns_per_language_and_per_kind() {
    let out = Command::new(bin())
        .args(["stats", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let d = &v["data"];
    assert!(d["total_files"].as_u64().unwrap() >= 5);
    assert!(d["total_lines"].as_u64().unwrap() > 0);
    assert!(d["languages"]["rust"]["files"].as_u64().unwrap() >= 1);
    assert!(d["languages"]["python"]["files"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["fn"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["struct"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["class"].as_u64().unwrap() >= 1);
    assert!(d["symbols"]["interface"].as_u64().unwrap() >= 1);
}
