use codegraph_core::index::{build_index, DefKind};
use codegraph_core::resolve::{resolve_refs, Confidence, Resolved};

use crate::cli::RenameArgs;
use crate::output::print_json;
use crate::serialize::{AppliedEdit, AppliedFile, RenameData, SkippedSite};

pub fn run(args: RenameArgs) -> anyhow::Result<i32> {
    let path = args.path.as_path();
    let index = build_index(path)?;
    let resolved = resolve_refs(&index, &args.old);

    // Step 2: find defs by name. Resolver doesn't expose this — query the
    // index directly.
    let defs: Vec<&codegraph_core::index::Definition> = index
        .definitions
        .iter()
        .filter(|d| d.name == args.old)
        .collect();

    // Count distinct files; same-file multiple defs (e.g. nested modules) are
    // handled by the resolver — only cross-file ambiguity requires --anchor.
    let def_files: std::collections::HashSet<&str> =
        defs.iter().map(|d| d.file.as_str()).collect();

    if def_files.len() > 1 && args.anchor.is_none() {
        // Multi-def disambiguation: emit needs_anchor + candidates and exit non-zero.
        let candidates: Vec<crate::serialize::Candidate> = defs
            .iter()
            .map(|d| crate::serialize::Candidate {
                file: d.file.clone(),
                line: d.line,
                kind: def_kind_str(d.kind).to_string(),
            })
            .collect();
        let data = RenameData {
            subcommand: "rename",
            dry_run: !args.apply,
            needs_anchor: Some(true),
            candidates: Some(candidates),
            applied: None,
            skipped: None,
            errors: None,
        };
        if args.json {
            print_json(data)?;
        }
        return Ok(2);
    }

    let mut applied: Vec<AppliedFile> = Vec::new();
    let mut skipped: Vec<SkippedSite> = Vec::new();
    let errors: Vec<crate::serialize::ErrorEntry> = Vec::new();

    // Low-confidence refs go to skipped[low-confidence] (Task 11).
    for r in &resolved {
        if matches!(r.confidence, Confidence::Low) {
            skipped.push(SkippedSite {
                file: r.reference.file.clone(),
                line: r.reference.line,
                col: r.reference.column,
                start_byte: r.reference.byte_offset,
                end_byte: r.reference.byte_offset + args.old.len(),
                name: r.reference.name.clone(),
                confidence: r.confidence.as_str(),
                reason: r.reason.as_str(),
                skip_reason: "low-confidence",
                via_alias: None,
                via_module: None,
            });
        }
    }

    // Alias re-export sites → skipped[re-export-alias].
    if let Some(sites) = index.alias_reexports.get(&args.old) {
        for site in sites {
            skipped.push(SkippedSite {
                file: site.file.clone(),
                line: site.line,
                col: 0,
                start_byte: 0,
                end_byte: 0,
                name: args.old.clone(),
                confidence: "high",
                reason: "same-file-scope",
                skip_reason: "re-export-alias",
                via_alias: Some(site.original.clone()),
                via_module: None,
            });
        }
    }

    // Wildcard re-exports: surface every wildcard whose target module defines
    // a symbol named OLD. We use a lossy match (file stem == module path's
    // last segment) — same heuristic the resolver uses for wildcard imports.
    let defines_old: std::collections::HashSet<String> = index
        .definitions
        .iter()
        .filter(|d| d.name == args.old)
        .map(|d| d.file.clone())
        .collect();

    for sites in index.wildcard_reexports.values() {
        for site in sites {
            // Cheap match: the module path's tail segment appears in some
            // definition-file's path. e.g. module_path "crate::inner" tail
            // "inner" matches "src/inner.rs" in `defines_old`.
            let tail = site.module_path.rsplit("::").next().unwrap_or(&site.module_path);
            let related = defines_old.iter().any(|f| {
                let stem = std::path::Path::new(f)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                stem == tail
            });
            if !related { continue; }

            let already = skipped.iter().any(|s|
                s.file == site.file && s.line == site.line && s.skip_reason == "wildcard-reexport"
            );
            if already { continue; }

            skipped.push(SkippedSite {
                file: site.file.clone(),
                line: site.line,
                col: 0,
                start_byte: 0,
                end_byte: 0,
                name: args.old.clone(),
                confidence: "medium",
                reason: "import-resolved",
                skip_reason: "wildcard-reexport",
                via_alias: None,
                via_module: Some(site.module_path.clone()),
            });
        }
    }

    // High/Medium → queued for edit, minus any collision with an alias site.
    let alias_keys: std::collections::HashSet<(String, usize)> = index
        .alias_reexports
        .get(&args.old)
        .map(|sites| sites.iter().map(|s| (s.file.clone(), s.line)).collect())
        .unwrap_or_default();

    let mut by_file: std::collections::BTreeMap<String, Vec<&Resolved>> = Default::default();
    for r in &resolved {
        if !matches!(r.confidence, Confidence::High | Confidence::Medium) { continue; }
        if alias_keys.contains(&(r.reference.file.clone(), r.reference.line)) { continue; }
        by_file.entry(r.reference.file.clone()).or_default().push(r);
    }

    for (file, refs) in by_file {
        let edits = build_edits(&file, &refs, &args, path)?;
        applied.push(edits);
    }

    let data = RenameData {
        subcommand: "rename",
        dry_run: !args.apply,
        needs_anchor: None,
        candidates: None,
        applied: Some(applied),
        skipped: Some(skipped),
        errors: Some(errors),
    };

    if args.json {
        print_json(data)?;
    }
    // Exit-status rules from Task 14 onward override this.
    Ok(0)
}

/// Construct an `AppliedFile` for one file. In dry-run, builds the edit list
/// without touching disk. `--apply` write logic lands in Task 17.
fn build_edits(
    file: &str,
    refs: &[&Resolved<'_>],
    args: &RenameArgs,
    root: &std::path::Path,
) -> anyhow::Result<AppliedFile> {
    // For now we use the reference's pre-computed `byte_offset` + len(OLD) for
    // the replacement range. Trust comes from the index: build_index recorded
    // the offset for an identifier whose name matched OLD at parse time.
    // Task 17 adds writes + drift checking; this task only needs the dry-run shape.
    let _ = root; // used in Task 17 (file IO)

    let old_len = args.old.len();
    let new_len = args.new.len();
    let mut edits: Vec<AppliedEdit> = Vec::new();
    for r in refs {
        edits.push(AppliedEdit {
            line: r.reference.line,
            col: r.reference.column,
            start_byte: r.reference.byte_offset,
            end_byte: r.reference.byte_offset + old_len,
            old: args.old.clone(),
            new: args.new.clone(),
            confidence: r.confidence.as_str(),
            reason: r.reason.as_str(),
        });
    }
    // Sort by byte position descending so later steps apply in reverse.
    edits.sort_by_key(|e| std::cmp::Reverse(e.start_byte));

    let bytes_changed = (new_len as i64 - old_len as i64) * edits.len() as i64;

    Ok(AppliedFile {
        file: file.to_string(),
        bytes_changed,
        edits,
    })
}

fn def_kind_str(k: DefKind) -> &'static str {
    use codegraph_core::index::DefKind::*;
    match k {
        Fn => "fn",
        Struct => "struct",
        Enum => "enum",
        Trait => "trait",
        Class => "class",
        Interface => "interface",
        Type => "type",
        Const => "const",
        Method => "method",
    }
}
