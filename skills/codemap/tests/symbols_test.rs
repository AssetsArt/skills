use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codemap"))
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample_project")
}

#[test]
fn symbols_rust_single_file() {
    let file = fixture().join("src/lib.rs");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    assert_eq!(v["schema_version"].as_u64().unwrap(), 1);
    let arr = v["data"].as_array().expect("data array");
    let mut names: Vec<(String, String)> = arr
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    names.sort();
    assert!(names.contains(&("Greeter".into(), "struct".into())));
    assert!(names.contains(&("Mood".into(), "enum".into())));
    assert!(names.contains(&("Speak".into(), "trait".into())));
    assert!(names.contains(&("Result".into(), "type".into())));
    assert!(names.contains(&("VERSION".into(), "const".into())));
    assert!(names.iter().any(|(n, k)| n == "greet" && k == "fn"));
}

#[test]
fn symbols_kind_filter_keeps_only_requested() {
    let file = fixture().join("src/lib.rs");
    let out = Command::new(bin())
        .args(["symbols", "--json", "--kind", "struct,enum"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    for s in v["data"].as_array().unwrap() {
        let k = s["kind"].as_str().unwrap();
        assert!(matches!(k, "struct" | "enum"), "unexpected kind {k}");
    }
}

#[test]
fn symbols_typescript_extracts_class_interface_type_fn() {
    let file = fixture().join("src/types.ts");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("User".into(), "interface".into())));
    assert!(pairs.contains(&("Status".into(), "type".into())));
    assert!(pairs.contains(&("UserRepo".into(), "class".into())));
    assert!(pairs.contains(&("findUser".into(), "fn".into())));
}

#[test]
fn symbols_tsx_extracts_function_and_interface() {
    let file = fixture().join("src/component.tsx");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("Props".into(), "interface".into())));
    assert!(pairs.contains(&("Header".into(), "fn".into())));
}

#[test]
fn symbols_javascript_extracts_function_and_class() {
    let file = fixture().join("src/util.js");
    let out = Command::new(bin())
        .args(["symbols", "--json"])
        .arg(&file)
        .output()
        .expect("run");
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json");
    let pairs: Vec<(String, String)> = v["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["name"].as_str().unwrap().to_string(),
                s["kind"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert!(pairs.contains(&("add".into(), "fn".into())));
    assert!(pairs.contains(&("Counter".into(), "class".into())));
}
