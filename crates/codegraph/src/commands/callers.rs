use crate::cli::CallersArgs;
use crate::output::print_json;
use codegraph_core::index::{build_index, DefKind, RefKind};
use codegraph_core::resolve::resolve_refs;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};

const HARD_CAP: usize = 8;

#[derive(Serialize, Clone)]
struct CallerEntry {
    file: String,
    line: usize,
    column: usize,
    name: String,
    kind: &'static str,
    distance: usize,
    confidence: &'static str,
    reason: &'static str,
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
    let depth_limit = args.depth.min(HARD_CAP);
    let idx = build_index(&args.path)?;

    let mut by_caller: BTreeMap<(String, String, usize), CallerEntry> = BTreeMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((args.name.clone(), 0));
    visited.insert(args.name.clone());

    while let Some((current, dist)) = queue.pop_front() {
        if dist >= depth_limit {
            continue;
        }
        for r in resolve_refs(&idx, &current) {
            if r.reference.kind != RefKind::Call {
                continue;
            }
            let Some(enclosing) =
                idx.enclosing_definition(&r.reference.file, r.reference.byte_offset)
            else {
                continue;
            };
            if !matches!(enclosing.kind, DefKind::Fn | DefKind::Method) {
                continue;
            }
            let key = (
                enclosing.name.clone(),
                enclosing.file.clone(),
                enclosing.line,
            );
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
                distance: dist + 1,
                confidence: r.confidence.as_str(),
                reason: r.reason.as_str(),
                sites: Vec::new(),
            });
            entry.sites.push(CallSite {
                file: r.reference.file.clone(),
                line: r.reference.line,
                column: r.reference.column,
                context: r.reference.context.clone(),
            });
            if visited.insert(enclosing.name.clone()) {
                queue.push_back((enclosing.name.clone(), dist + 1));
            }
        }
    }

    let mut entries: Vec<CallerEntry> = by_caller.into_values().collect();
    entries.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });
    if args.json {
        print_json(&entries)?;
    } else {
        for e in &entries {
            println!(
                "d={}  {}:{}  {}  {}  ({} call site(s), {} {})",
                e.distance,
                e.file,
                e.line,
                e.kind,
                e.name,
                e.sites.len(),
                e.confidence,
                e.reason
            );
        }
    }
    Ok(())
}
