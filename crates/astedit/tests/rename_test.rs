mod common;

use common::{copy_fixture, run_astedit_json};

#[test]
fn rename_same_file_high_confidence_dry_run_default() {
    let tmp = copy_fixture("same_file");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_eq!(code, 0, "dry-run with matches should exit 0");
    assert_eq!(data["subcommand"], "rename");
    assert_eq!(data["dry_run"], true);
    assert!(
        data["errors"].as_array().unwrap().is_empty(),
        "no errors expected: {:?}",
        data["errors"]
    );

    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(
        applied.len(),
        1,
        "single fixture file expected; got: {applied:?}"
    );
    let file_entry = &applied[0];
    assert!(file_entry["file"].as_str().unwrap().ends_with("main.rs"));

    let edits = file_entry["edits"].as_array().unwrap();
    // The fixture has the struct DEFINITION (1) + 3 use sites = 4 identifier
    // sites total. The resolver returns references (not definitions); whether
    // the definition's identifier is itself a Reference depends on the index.
    // The test asserts at least 3 (the use sites) and at most 4.
    assert!(
        edits.len() >= 3 && edits.len() <= 4,
        "expected 3–4 edits, got {}",
        edits.len()
    );

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
    assert!(
        after.contains("struct User"),
        "dry-run modified the file: {after}"
    );
    assert!(
        !after.contains("Account"),
        "dry-run wrote Account into the file"
    );
}

#[test]
fn rename_cross_file_import_resolved() {
    let tmp = copy_fixture("cross_file_import");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_eq!(code, 0);
    assert!(
        data["errors"].as_array().unwrap().is_empty(),
        "errors: {:?}",
        data["errors"]
    );

    let applied = data["applied"].as_array().expect("applied array");
    // Expect at least src/lib.rs to be touched (the import-resolved use).
    // src/inner.rs contains the definition itself; whether its self-references
    // count depends on the indexer — accept 1 or 2 files.
    assert!(
        !applied.is_empty() && applied.len() <= 2,
        "got {} files: {applied:?}",
        applied.len()
    );

    let files: Vec<&str> = applied
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert!(
        files.iter().any(|f| f.ends_with("lib.rs")),
        "lib.rs not in applied: {files:?}",
    );

    let lib = applied
        .iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("lib.rs"))
        .unwrap();
    let lib_edits = lib["edits"].as_array().unwrap();
    assert!(
        !lib_edits.is_empty(),
        "expected at least one edit in lib.rs"
    );
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

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");

    let lib = applied
        .iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("lib.rs"))
        .expect("lib.rs should be in applied");
    let lib_edits = lib["edits"].as_array().unwrap();
    assert!(
        !lib_edits.is_empty(),
        "expected at least one edit in lib.rs"
    );
    for e in lib_edits {
        // Glob-only import → resolver assigns Medium.
        assert_eq!(
            e["confidence"], "medium",
            "expected medium for glob import: {e:?}"
        );
    }
}

#[test]
fn rename_name_only_goes_to_skipped_low_confidence() {
    let tmp = copy_fixture("name_only");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let lows: Vec<&serde_json::Value> = skipped
        .iter()
        .filter(|s| s["skip_reason"] == "low-confidence")
        .collect();
    assert!(
        !lows.is_empty(),
        "expected at least one low-confidence skip; skipped: {skipped:?}"
    );

    for s in &lows {
        assert_eq!(s["confidence"], "low");
        assert_eq!(s["reason"], "name-only");
        assert_eq!(s["name"], "User");
        assert!(s["file"].as_str().unwrap().ends_with("unrelated.rs"));
    }

    // unrelated.rs must NOT appear in applied.
    let applied = data["applied"].as_array().unwrap();
    let bad = applied
        .iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("unrelated.rs"));
    assert!(
        bad.is_none(),
        "unrelated.rs should not be in applied: {bad:?}"
    );
}

#[test]
fn rename_alias_reexport_skipped_with_via_alias() {
    let tmp = copy_fixture("alias_reexport");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let aliases: Vec<&serde_json::Value> = skipped
        .iter()
        .filter(|s| s["skip_reason"] == "re-export-alias")
        .collect();
    assert_eq!(aliases.len(), 1, "expected one alias skip; got {skipped:?}");

    let alias = aliases[0];
    assert!(alias["file"].as_str().unwrap().ends_with("lib.rs"));
    assert_eq!(alias["name"], "User");
    assert_eq!(
        alias["via_alias"], "Bar",
        "via_alias should be the original symbol"
    );
    match alias.get("via_module") {
        None => {}
        Some(v) if v.is_null() => {}
        Some(other) => panic!("via_module must not appear on re-export-alias entries: {other:?}"),
    }
}

#[test]
fn rename_wildcard_reexport_skipped_with_via_module() {
    let tmp = copy_fixture("wildcard_reexport");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_eq!(code, 0);

    let skipped = data["skipped"].as_array().expect("skipped array");
    let wilds: Vec<&serde_json::Value> = skipped
        .iter()
        .filter(|s| s["skip_reason"] == "wildcard-reexport")
        .collect();
    assert_eq!(
        wilds.len(),
        1,
        "expected one wildcard skip; got {skipped:?}"
    );

    let w = wilds[0];
    assert!(w["file"].as_str().unwrap().ends_with("lib.rs"));
    assert!(
        w["via_module"].as_str().unwrap().contains("inner"),
        "via_module should reference inner module; got {:?}",
        w["via_module"]
    );
    match w.get("via_alias") {
        None => {}
        Some(v) if v.is_null() => {}
        Some(other) => panic!("via_alias must not appear on wildcard-reexport entries: {other:?}"),
    }
}

#[test]
fn rename_multi_def_without_anchor_needs_anchor_envelope() {
    let tmp = copy_fixture("multi_def");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&["rename", "User", "Account", "--path", path, "--json"]);

    assert_ne!(code, 0, "multi-def without --anchor must exit non-zero");
    assert_eq!(data["subcommand"], "rename");
    assert_eq!(data["needs_anchor"], true);

    let candidates = data["candidates"].as_array().expect("candidates array");
    assert_eq!(
        candidates.len(),
        2,
        "expected exactly two candidates: {candidates:?}"
    );

    let kinds: Vec<&str> = candidates
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert!(kinds.contains(&"struct"));
    assert!(kinds.contains(&"fn"));

    for c in candidates {
        assert!(c["line"].as_u64().unwrap() >= 1);
        let f = c["file"].as_str().unwrap();
        assert!(f.ends_with("a.rs") || f.ends_with("b.rs"));
    }

    // No applied/skipped/errors on the needs_anchor path.
    match data.get("applied") {
        None => {}
        Some(v) if v.is_null() => {}
        Some(other) => panic!("applied should be absent: {other:?}"),
    }
    match data.get("skipped") {
        None => {}
        Some(v) if v.is_null() => {}
        Some(other) => panic!("skipped should be absent: {other:?}"),
    }
    match data.get("errors") {
        None => {}
        Some(v) if v.is_null() => {}
        Some(other) => panic!("errors should be absent: {other:?}"),
    }
}

#[test]
fn rename_with_anchor_picks_matching_definition() {
    let tmp = copy_fixture("multi_def");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rename",
        "User",
        "Account",
        "--path",
        path,
        "--anchor",
        "src/a.rs:1",
        "--json",
    ]);

    assert_eq!(code, 0, "anchor present → exit 0: data was {data:?}");
    match data.get("needs_anchor") {
        None => {}
        Some(v) if v.is_null() => {}
        Some(other) => panic!("needs_anchor should be absent: {other:?}"),
    }

    // The `fn User` in b.rs must not be touched — its references should not
    // appear in applied.
    let applied = data["applied"].as_array().expect("applied array");
    for f in applied {
        let file = f["file"].as_str().unwrap();
        assert!(
            file.ends_with("a.rs") || file.ends_with("lib.rs"),
            "anchor picked struct in a.rs; b.rs should not appear: {file}"
        );
    }
    // Specifically: b.rs must NOT be in applied
    let bad = applied
        .iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("b.rs"));
    assert!(
        bad.is_none(),
        "b.rs should not be in applied with anchor a.rs:1: {bad:?}"
    );
}

#[test]
fn rename_apply_writes_changes_to_disk() {
    let tmp = copy_fixture("apply_write");
    let path = tmp.path().to_str().unwrap();
    let target = tmp.path().join("main.rs");
    let before = std::fs::read_to_string(&target).unwrap();
    assert!(before.contains("struct User"));

    let (code, data) = run_astedit_json(&[
        "rename", "User", "Account", "--path", path, "--apply", "--json",
    ]);

    assert_eq!(code, 0, "errors: {:?}", data["errors"]);
    assert_eq!(data["dry_run"], false);
    assert!(
        data["errors"].as_array().unwrap().is_empty(),
        "errors: {:?}",
        data["errors"]
    );

    let after = std::fs::read_to_string(&target).unwrap();
    assert!(!after.contains("struct User"), "User should be renamed");
    assert!(
        after.contains("struct Account"),
        "Account should appear: {after}"
    );
    assert!(
        after.contains("Account { id: 42 }"),
        "body literal not renamed: {after}"
    );
    assert!(
        after.contains("-> Account"),
        "return type not renamed: {after}"
    );

    // `bytes_changed` should be (new - old) * #edits, where new=7, old=4, delta=+3.
    let applied = data["applied"].as_array().unwrap();
    let entry = applied
        .iter()
        .find(|f| f["file"].as_str().unwrap().ends_with("main.rs"))
        .unwrap();
    let bytes_changed = entry["bytes_changed"].as_i64().unwrap();
    let edit_count = entry["edits"].as_array().unwrap().len() as i64;
    assert_eq!(
        bytes_changed,
        3 * edit_count,
        "bytes_changed should be 3 per edit"
    );
}

#[test]
fn drift_between_index_and_apply_emits_hash_mismatch() {
    let tmp = copy_fixture("apply_write");

    // Build an index against the fresh fixture.
    let index = codegraph_core::index::build_index(tmp.path()).unwrap();
    // file_meta keys are repo-relative, forward-slash-normalized.
    let (rel, meta) = index
        .file_meta
        .iter()
        .find(|(k, _)| k.ends_with("main.rs"))
        .expect("main.rs in file_meta");
    let original_len = meta.len;

    // Mutate the file so its length differs from the snapshot.
    let target = tmp.path().join("main.rs");
    let mut content = std::fs::read_to_string(&target).unwrap();
    content.push_str("\n// drift bait\n");
    std::fs::write(&target, &content).unwrap();
    assert_ne!(content.len() as u64, original_len);

    // The drift checker now sees length mismatch + no recorded hash → error.
    let err =
        astedit::apply::check_drift(&target, rel, meta, None).expect_err("expected hash-mismatch");
    assert_eq!(err.kind(), "hash-mismatch");
}
