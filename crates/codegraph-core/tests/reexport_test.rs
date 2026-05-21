use codegraph_core::index::build_index;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

#[test]
fn rust_alias_reexport_recorded() {
    let index = build_index(&fixture("reexport_rust")).unwrap();
    let sites = index
        .alias_reexports
        .get("Baz")
        .expect("Baz alias should be recorded under its local name");
    assert_eq!(sites.len(), 1);
    let site = &sites[0];
    assert!(site.file.ends_with("lib.rs"), "got {}", site.file);
    assert_eq!(site.alias, "Baz");
    assert_eq!(site.original, "Bar");
    assert!(site.module_path.contains("inner"));
}

#[test]
fn rust_wildcard_reexport_recorded() {
    let index = build_index(&fixture("reexport_rust")).unwrap();
    let sites = index
        .wildcard_reexports
        .get("widgets")
        .expect("widgets wildcard re-export should be recorded");
    assert_eq!(sites.len(), 1);
    assert!(sites[0].file.ends_with("lib.rs"));
    assert!(sites[0].module_path.contains("inner::widgets"));
}

#[test]
fn rust_non_pub_use_is_not_a_reexport() {
    // `use inner::Untouched;` (no `pub`) is a plain import, not a re-export.
    // It must NOT appear in either re-export table.
    let index = build_index(&fixture("reexport_rust")).unwrap();
    assert!(
        index.alias_reexports.values().flat_map(|v| v.iter()).all(|s| s.alias != "Untouched"),
        "Untouched is a pub use without alias — should not be in alias_reexports",
    );
}
