use codegraph_core::index::{build_index, DefKind};
use codegraph_core::resolve::{resolve_refs, Confidence, Resolved};

use crate::cli::RenameArgs;
use crate::error::AstEditError;
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
    let def_files: std::collections::HashSet<&str> = defs.iter().map(|d| d.file.as_str()).collect();

    // Multi-def + no anchor → emit needs_anchor envelope + non-zero exit.
    if def_files.len() > 1 && args.anchor.is_none() {
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

    // Pick the chosen definition. With --anchor present (or single def), this
    // narrows the resolved set to refs that resolve to that specific def.
    // Same-file multi-defs (def_files.len() == 1, defs.len() > 1) have no
    // cross-file ambiguity, so we don't require an anchor — treat as unfiltered.
    let chosen_def: Option<&codegraph_core::index::Definition> =
        match (defs.len(), args.anchor.as_deref()) {
            (0, _) => None,
            (1, _) => Some(defs[0]),
            (_, Some(anchor)) => {
                let (file, line) = parse_anchor(anchor)?;
                Some(
                    defs.iter()
                        .find(|d| d.file == file && d.line == line)
                        .copied()
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "anchor {file}:{line} did not match any definition of {}",
                                args.old,
                            )
                        })?,
                )
            }
            // Same-file multi-def (def_files.len() == 1) with no anchor: no
            // cross-file filtering needed; the resolver handles same-file scope.
            (_, None) => None,
        };

    // When chosen_def is set, filter resolved to refs that pin to that def.
    // Low-confidence refs (definition is None) stay in regardless of anchor —
    // they aren't tied to any specific def and we still want to surface them
    // as skipped[low-confidence].
    let resolved: Vec<Resolved<'_>> = if let Some(def) = chosen_def {
        resolved
            .into_iter()
            .filter(|r| match r.confidence {
                Confidence::Low => true,
                _ => match r.definition {
                    Some(d) => d.file == def.file && d.line == def.line && d.kind == def.kind,
                    None => r.reference.file == def.file,
                },
            })
            .collect()
    } else {
        resolved
    };

    let mut applied: Vec<AppliedFile> = Vec::new();
    let mut skipped: Vec<SkippedSite> = Vec::new();
    let mut errors: Vec<crate::serialize::ErrorEntry> = Vec::new();

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
            let tail = site
                .module_path
                .rsplit("::")
                .next()
                .unwrap_or(&site.module_path);
            let related = defines_old.iter().any(|f| {
                let stem = std::path::Path::new(f)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                stem == tail
            });
            if !related {
                continue;
            }

            let already = skipped.iter().any(|s| {
                s.file == site.file && s.line == site.line && s.skip_reason == "wildcard-reexport"
            });
            if already {
                continue;
            }

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
        if !matches!(r.confidence, Confidence::High | Confidence::Medium) {
            continue;
        }
        if alias_keys.contains(&(r.reference.file.clone(), r.reference.line)) {
            continue;
        }
        by_file.entry(r.reference.file.clone()).or_default().push(r);
    }

    for (file, refs) in by_file {
        match build_edits(&file, &refs, &args, path, &index) {
            Ok(entry) => applied.push(entry),
            Err(e) => errors.push(crate::serialize::ErrorEntry::from(&e)),
        }
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

fn build_edits(
    file: &str,
    refs: &[&Resolved<'_>],
    args: &RenameArgs,
    root: &std::path::Path,
    index: &codegraph_core::index::Index,
) -> Result<AppliedFile, AstEditError> {
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
    edits.sort_by_key(|e| std::cmp::Reverse(e.start_byte));
    let bytes_changed = (new_len as i64 - old_len as i64) * edits.len() as i64;

    if args.apply {
        let abs = root.join(file);

        // Step 5a: drift check. Skip the file if it changed since indexing.
        let meta = index
            .file_meta
            .get(file)
            .ok_or_else(|| AstEditError::HashMismatch {
                file: file.to_string(),
            })?;
        crate::apply::check_drift(&abs, file, meta, None)?;

        // Read after drift passes.
        let source = std::fs::read(&abs).map_err(|e| AstEditError::WriteFailed {
            file: file.to_string(),
            os_code: e.raw_os_error(),
            message: e.to_string(),
        })?;
        let original_len = source.len() as u64;
        let mut bytes = source;

        // Defensive node-kind check + splice in descending byte order.
        for e in &edits {
            if e.end_byte > bytes.len() || &bytes[e.start_byte..e.end_byte] != args.old.as_bytes() {
                return Err(AstEditError::NodeKindMismatch {
                    file: file.to_string(),
                    line: e.line,
                    col: e.col,
                });
            }
            bytes.splice(e.start_byte..e.end_byte, args.new.bytes());
        }

        // Step 5e: race-window guard. Re-stat just before the write and
        // compare against the length we read into memory. Same-length
        // concurrent writes slip through — accepted trade-off (spec).
        let current = crate::apply::current_len(&abs, file)?;
        if current != original_len {
            return Err(AstEditError::ConcurrentWrite {
                file: file.to_string(),
            });
        }

        // Step 5f: atomic write.
        crate::apply::write_atomic(&abs, &bytes)?;
    }

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

/// Parse a `--anchor FILE:LINE` value into `(file, line)`. The file is the
/// repo-relative, forward-slash-normalized form the index uses; the line is
/// 1-based.
fn parse_anchor(s: &str) -> anyhow::Result<(String, usize)> {
    let (file, line) = s
        .rsplit_once(':')
        .ok_or_else(|| anyhow::anyhow!("--anchor expected FILE:LINE, got {s:?}"))?;
    let line: usize = line
        .parse()
        .map_err(|_| anyhow::anyhow!("--anchor line must be a positive integer, got {line:?}"))?;
    if line == 0 {
        anyhow::bail!("--anchor line must be 1-based, got 0");
    }
    Ok((file.to_string(), line))
}
