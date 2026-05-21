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
