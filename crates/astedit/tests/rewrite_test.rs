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

#[test]
fn rewrite_typescript_matches() {
    let tmp = copy_fixture("rewrite_typescript");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "console.log($A)",
        "--rewrite",
        "console.error($A)",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(
        data["errors"].as_array().unwrap().is_empty(),
        "errors: {:?}",
        data["errors"]
    );

    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1);
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.ts"));

    let edits = applied[0]["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2, "two console.log calls expected");
    for e in edits {
        assert!(e["old"].as_str().unwrap().starts_with("console.log"));
        assert!(e["new"].as_str().unwrap().starts_with("console.error"));
    }
}

#[test]
fn rewrite_tsx_matches_jsx_file() {
    let tmp = copy_fixture("rewrite_tsx");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "console.log($A)",
        "--rewrite",
        "console.error($A)",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0);
    assert!(
        data["errors"].as_array().unwrap().is_empty(),
        "errors: {:?}",
        data["errors"]
    );
    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1, "exactly one .tsx file expected");
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.tsx"));
    assert_eq!(applied[0]["edits"].as_array().unwrap().len(), 1);
}

#[test]
fn rewrite_javascript_matches() {
    let tmp = copy_fixture("rewrite_javascript");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "console.log($A)",
        "--rewrite",
        "console.error($A)",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1);
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.js"));
    assert_eq!(applied[0]["edits"].as_array().unwrap().len(), 1);
}

#[test]
fn rewrite_python_matches() {
    let tmp = copy_fixture("rewrite_python");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "print($A)",
        "--rewrite",
        "logging.info($A)",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().expect("applied array");
    assert_eq!(applied.len(), 1);
    assert!(applied[0]["file"].as_str().unwrap().ends_with("main.py"));
    assert_eq!(applied[0]["edits"].as_array().unwrap().len(), 2);
    for e in applied[0]["edits"].as_array().unwrap() {
        assert!(e["new"].as_str().unwrap().starts_with("logging.info"));
    }
}

#[test]
fn rewrite_single_metavar_substituted_in_replacement() {
    let tmp = copy_fixture("rewrite_metavar");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "String::from($S)",
        "--rewrite",
        "String::from($S.to_owned())",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().unwrap();
    assert_eq!(applied.len(), 1);
    let edits = applied[0]["edits"].as_array().unwrap();
    assert_eq!(edits.len(), 2);

    let news: Vec<&str> = edits.iter().map(|e| e["new"].as_str().unwrap()).collect();
    assert!(
        news.iter().any(|n| n.contains("\"alice\".to_owned()")),
        "expected `alice` capture materialised in rewrite; news = {news:?}",
    );
    assert!(
        news.iter().any(|n| n.contains("\"bob\".to_owned()")),
        "expected `bob` capture materialised in rewrite; news = {news:?}",
    );
}

#[test]
fn rewrite_multimatch_metavar_preserves_argument_list() {
    let tmp = copy_fixture("rewrite_multimatch");
    let path = tmp.path().to_str().unwrap();

    let (code, data) = run_astedit_json(&[
        "rewrite",
        "--pattern",
        "log($$$ARGS)",
        "--rewrite",
        "tracing::info!($$$ARGS)",
        "--path",
        path,
        "--json",
    ]);

    assert_eq!(code, 0);
    let applied = data["applied"].as_array().unwrap();
    assert_eq!(
        applied.len(),
        1,
        "single-file fixture expected; got: {applied:?}"
    );

    let edits = applied[0]["edits"].as_array().unwrap();
    assert_eq!(
        edits.len(),
        2,
        "expected 2 call-site matches; edits={edits:?}"
    );

    let news: Vec<&str> = edits.iter().map(|e| e["new"].as_str().unwrap()).collect();
    assert!(
        news.iter().any(|n| n.contains("\"a\", 1, true")),
        "expected 3-arg capture preserved; news = {news:?}"
    );
    assert!(
        news.iter().any(|n| n.contains("(\"b\")")),
        "expected single-arg capture preserved; news = {news:?}"
    );
}
