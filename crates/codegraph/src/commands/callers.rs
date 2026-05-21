use crate::cli::CallersArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize, Clone)]
struct CallerEntry {
    file: String,
    line: usize,
    column: usize,
    name: String,
    kind: &'static str, // "fn" or "method"
    confidence: &'static str,
    reason: &'static str,
    /// Call sites inside this caller that point at the target.
    sites: Vec<CallSite>,
}

#[derive(Serialize, Clone)]
struct CallSite {
    file: String,
    line: usize,
    column: usize,
    context: String,
}

pub fn run(args: CallersArgs) -> anyhow::Result<()> {
    if args.depth != 1 {
        // Depth-N is implemented in Task 16 — surface the limitation here rather than silently ignore.
        anyhow::bail!("callers: --depth > 1 is not implemented yet (tracked in Task 16)");
    }
    let idx = build_index(&args.path)?;

    let mut by_caller: BTreeMap<(String, usize, usize), CallerEntry> = BTreeMap::new();

    for resolved in resolve_refs(&idx, &args.name) {
        if resolved.reference.kind != RefKind::Call {
            continue;
        }
        let Some(enclosing) =
            idx.enclosing_definition(&resolved.reference.file, resolved.reference.byte_offset)
        else {
            // Reference at module scope (e.g. `const X: () = foo();`). Skip — not a "caller fn".
            continue;
        };
        if !matches!(enclosing.kind, DefKind::Fn | DefKind::Method) {
            continue;
        }
        let key = (enclosing.file.clone(), enclosing.line, enclosing.column);
        let entry = by_caller.entry(key).or_insert_with(|| CallerEntry {
            file: enclosing.file.clone(),
            line: enclosing.line,
            column: enclosing.column,
            name: enclosing.name.clone(),
            kind: if enclosing.kind == DefKind::Method {
                "method"
            } else {
                "fn"
            },
            confidence: resolved.confidence.as_str(),
            reason: resolved.reason.as_str(),
            sites: Vec::new(),
        });
        // Downgrade: if any site is low, keep the worst.
        if confidence_rank(entry.confidence) > confidence_rank(resolved.confidence.as_str()) {
            entry.confidence = resolved.confidence.as_str();
            entry.reason = resolved.reason.as_str();
        }
        entry.sites.push(CallSite {
            file: resolved.reference.file.clone(),
            line: resolved.reference.line,
            column: resolved.reference.column,
            context: resolved.reference.context.clone(),
        });
    }

    let mut entries: Vec<CallerEntry> = by_caller.into_values().collect();
    entries.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    if args.json {
        print_json(&entries)?;
    } else {
        for e in &entries {
            println!(
                "{}:{}:{}  {}  {}  ({} call site(s), {} {})",
                e.file,
                e.line,
                e.column,
                e.kind,
                e.name,
                e.sites.len(),
                e.confidence,
                e.reason
            );
            for s in &e.sites {
                println!("    {}:{}  {}", s.file, s.line, s.context);
            }
        }
    }
    Ok(())
}

fn confidence_rank(c: &str) -> u8 {
    match c {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}
