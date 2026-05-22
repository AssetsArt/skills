mod common;

use common::{copy_fixture, run_astedit_json};

#[test]
fn rewrite_rust_two_matches_dry_run_default() {
    let tmp = copy_fixture("rewrite_rust");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "println!($A)",
        "--rewrite",
        "eprintln!($A)",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0, "dry-run with matches exits 0");
    assert_eq!(data["subcommand"], "rewrite");
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
    assert_eq!(
        edits.len(),
        2,
        "expected 2 println matches; got {}",
        edits.len()
    );

    for e in edits {
        assert!(e["old"].as_str().unwrap().starts_with("println!"));
        assert!(e["new"].as_str().unwrap().starts_with("eprintln!"));
        assert!(
            e.get("confidence").is_none(),
            "rewrite edit must not have confidence: {e:?}"
        );
        assert!(
            e.get("reason").is_none(),
            "rewrite edit must not have reason: {e:?}"
        );
        assert!(e["start_byte"].as_u64().unwrap() < e["end_byte"].as_u64().unwrap());
    }

    let fixture_file = tmp.path().join("main.rs");
    let after = std::fs::read_to_string(&fixture_file).unwrap();
    assert!(
        after.contains("println!(\"hi\")"),
        "dry-run modified the fixture: {after}"
    );
    assert!(
        !after.contains("eprintln"),
        "dry-run wrote eprintln! into the fixture"
    );
}
