use crate::cli::CalleesArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
struct CalleeEntry {
    name: String,
    kind: &'static str,
    /// Where the callee is defined (empty when we couldn't resolve a definition).
    def_file: Option<String>,
    def_line: Option<usize>,
    confidence: &'static str,
    reason: &'static str,
    sites: Vec<CallSite>,
}

#[derive(Serialize)]
struct CallSite {
    file: String,
    line: usize,
    column: usize,
    context: String,
}

pub fn run(args: CalleesArgs) -> anyhow::Result<()> {
    if args.depth != 1 {
        anyhow::bail!("callees: --depth > 1 is not implemented yet (tracked in Task 16)");
    }
    let idx = build_index(&args.path)?;

    let outer: Vec<_> = idx
        .definitions
        .iter()
        .filter(|d| d.name == args.name && matches!(d.kind, DefKind::Fn | DefKind::Method))
        .collect();
    if outer.is_empty() {
        return emit(&[], args.json);
    }

    // Collect calls that live inside any matching outer's body.
    let mut by_callee: BTreeMap<String, CalleeEntry> = BTreeMap::new();
    for r in &idx.references {
        if r.kind != RefKind::Call {
            continue;
        }
        if !outer.iter().any(|o| {
            o.file == r.file
                && o.body_start_byte <= r.byte_offset
                && r.byte_offset < o.body_end_byte
        }) {
            continue;
        }
        // Resolve THIS reference (cheap: re-use resolve_refs with that specific name).
        let resolutions = resolve_refs(&idx, &r.name);
        let chosen = resolutions
            .iter()
            .find(|res| res.reference.byte_offset == r.byte_offset && res.reference.file == r.file);
        let (confidence, reason, def_file, def_line) = match chosen {
            Some(res) => (
                res.confidence.as_str(),
                res.reason.as_str(),
                res.definition.map(|d| d.file.clone()),
                res.definition.map(|d| d.line),
            ),
            None => ("low", "name-only", None, None),
        };
        let entry = by_callee
            .entry(r.name.clone())
            .or_insert_with(|| CalleeEntry {
                name: r.name.clone(),
                kind: "fn",
                def_file: def_file.clone(),
                def_line,
                confidence,
                reason,
                sites: Vec::new(),
            });
        entry.sites.push(CallSite {
            file: r.file.clone(),
            line: r.line,
            column: r.column,
            context: r.context.clone(),
        });
    }
    let entries: Vec<CalleeEntry> = by_callee.into_values().collect();
    emit(&entries, args.json)
}

fn emit(entries: &[CalleeEntry], json: bool) -> anyhow::Result<()> {
    if json {
        print_json(entries)?;
    } else {
        for e in entries {
            let target = match (&e.def_file, e.def_line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                _ => "<unresolved>".to_string(),
            };
            println!(
                "{}  -> {}  ({} {}; {} site(s))",
                e.name,
                target,
                e.confidence,
                e.reason,
                e.sites.len()
            );
        }
    }
    Ok(())
}
