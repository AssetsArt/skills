use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codemap"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn files_json_lists_all_supported_extensions() {
    let out = Command::new(bin())
        .args(["files", "--json", "--path"])
        .arg(fixture())
        .output()
        .expect("run codemap");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let arr = v["data"].as_array().expect("data array");
    let paths: Vec<String> = arr
        .iter()
        .map(|e| e["path"].as_str().unwrap().to_string())
        .collect();
    assert!(paths.iter().any(|p| p.ends_with("src/lib.rs")));
    assert!(paths.iter().any(|p| p.ends_with("src/types.ts")));
    assert!(paths.iter().any(|p| p.ends_with("src/component.tsx")));
    assert!(paths.iter().any(|p| p.ends_with("src/util.js")));
    assert!(paths.iter().any(|p| p.ends_with("app.py")));
    let langs: std::collections::HashSet<&str> =
        arr.iter().map(|e| e["language"].as_str().unwrap()).collect();
    for expected in ["rust", "typescript", "tsx", "javascript", "python"] {
        assert!(langs.contains(expected), "missing language {expected}");
    }
}

#[test]
fn files_human_groups_by_language() {
    let out = Command::new(bin())
        .args(["files", "--path"])
        .arg(fixture())
        .output()
        .expect("run codemap");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("rust"));
    assert!(text.contains("python"));
    assert!(text.contains("typescript"));
}
