mod common;

use common::{copy_fixture, run_astedit_json};

#[test]
fn rename_same_file_high_confidence_dry_run_default() {
    let tmp = copy_fixture("same_file");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0, "dry-run with matches should exit 0");
    assert_eq!(data["subcommand"], "rename");
    assert_eq!(data["dry_run"], true);
    assert!(data["errors"].as_array().unwrap().is_empty(), "no errors expected: {:?}", data["errors"]);

    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1, "single fixture file expected; got: {applied:?}");
    let file_entry = &applied[0];
    assert!(file_entry["file"].as_str().unwrap().ends_with("main.rs"));

    let edits = file_entry["edits"].as_array().unwrap();
    // The fixture has the struct DEFINITION (1) + 3 use sites = 4 identifier
    // sites total. The resolver returns references (not definitions); whether
    // the definition's identifier is itself a Reference depends on the index.
    // The test asserts at least 3 (the use sites) and at most 4.
    assert!(edits.len() >= 3 && edits.len() <= 4, "expected 3–4 edits, got {}", edits.len());

    for e in edits {
        assert_eq!(e["old"], "User");
        assert_eq!(e["new"], "Account");
        assert_eq!(e["confidence"], "high");
        assert_eq!(e["reason"], "same-file-scope");
        assert!(e["start_byte"].as_u64().unwrap() < e["end_byte"].as_u64().unwrap());
        assert_eq!(
            e["end_byte"].as_u64().unwrap() - e["start_byte"].as_u64().unwrap(),
            "User".len() as u64,
        );
    }

    // Dry-run must NOT mutate the fixture copy.
    let fixture_file = tmp.path().join("main.rs");
    let after = std::fs::read_to_string(&fixture_file).unwrap();
    assert!(after.contains("struct User"), "dry-run modified the file: {after}");
    assert!(!after.contains("Account"), "dry-run wrote Account into the file");
}

#[test]
fn rename_cross_file_import_resolved() {
    let tmp = copy_fixture("cross_file_import");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(data["errors"].as_array().unwrap().is_empty(), "errors: {:?}", data["errors"]);

    let applied = data["applied"].as_array().expect("applied array");
    // Expect at least src/lib.rs to be touched (the import-resolved use).
    // src/inner.rs contains the definition itself; whether its self-references
    // count depends on the indexer — accept 1 or 2 files.
    assert!(!applied.is_empty() && applied.len() <= 2, "got {} files: {applied:?}", applied.len());

    let files: Vec<&str> = applied.iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert!(
        files.iter().any(|f| f.ends_with("lib.rs")),
        "lib.rs not in applied: {files:?}",
    );

    let lib = applied.iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("lib.rs"))
        .unwrap();
    let lib_edits = lib["edits"].as_array().unwrap();
    assert!(!lib_edits.is_empty(), "expected at least one edit in lib.rs");
    for e in lib_edits {
        assert_eq!(e["old"], "User");
        assert_eq!(e["new"], "Account");
        // import-resolved across files is "high" via ImportResolved reason.
        assert_eq!(e["reason"], "import-resolved", "got {e:?}");
        assert_eq!(e["confidence"], "high", "got {e:?}");
    }
}

#[test]
fn rename_glob_import_medium_confidence_applied() {
    let tmp = copy_fixture("glob_import");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");

    let lib = applied.iter().find(|f| f["file"].as_str().unwrap().ends_with("lib.rs"))
        .expect("lib.rs should be in applied");
    let lib_edits = lib["edits"].as_array().unwrap();
    assert!(!lib_edits.is_empty(), "expected at least one edit in lib.rs");
    for e in lib_edits {
        // Glob-only import → resolver assigns Medium.
        assert_eq!(e["confidence"], "medium", "expected medium for glob import: {e:?}");
    }
}

#[test]
fn rename_name_only_goes_to_skipped_low_confidence() {
    let tmp = copy_fixture("name_only");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let lows: Vec<&serde_json::Value> = skipped.iter()
        .filter(|s| s["skip_reason"] == "low-confidence")
        .collect();
    assert!(!lows.is_empty(), "expected at least one low-confidence skip; skipped: {skipped:?}");

    for s in &lows {
        assert_eq!(s["confidence"], "low");
        assert_eq!(s["reason"], "name-only");
        assert_eq!(s["name"], "User");
        assert!(s["file"].as_str().unwrap().ends_with("unrelated.rs"));
    }

    // unrelated.rs must NOT appear in applied.
    let applied = data["applied"].as_array().unwrap();
    let bad = applied.iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("unrelated.rs"));
    assert!(bad.is_none(), "unrelated.rs should not be in applied: {bad:?}");
}

#[test]
fn rename_alias_reexport_skipped_with_via_alias() {
    let tmp = copy_fixture("alias_reexport");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account",
        "--path", path,
        "--json",
    ]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let aliases: Vec<&serde_json::Value> = skipped.iter()
        .filter(|s| s["skip_reason"] == "re-export-alias")
        .collect();
    assert_eq!(aliases.len(), 1, "expected one alias skip; got {skipped:?}");

    let alias = aliases[0];
    assert!(alias["file"].as_str().unwrap().ends_with("lib.rs"));
    assert_eq!(alias["name"], "User");
    assert_eq!(alias["via_alias"], "Bar", "via_alias should be the original symbol");
    match alias.get("via_module") {
        None => {},
        Some(v) if v.is_null() => {},
        Some(other) => panic!("via_module must not appear on re-export-alias entries: {other:?}"),
    }
}
