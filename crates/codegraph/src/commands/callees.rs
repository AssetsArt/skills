use crate::cli::CalleesArgs;
use crate::output::print_json;
use codegraph_core::index::{build_index, DefKind, RefKind};
use codegraph_core::resolve::resolve_refs;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};

const HARD_CAP: usize = 8;

#[derive(Serialize, Clone)]
struct CalleeEntry {
    name: String,
    kind: &'static str,
    def_file: Option<String>,
    def_line: Option<usize>,
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

pub fn run(args: CalleesArgs) -> anyhow::Result<()> {
    let depth_limit = args.depth.min(HARD_CAP);
    let idx = build_index(&args.path)?;

    let mut entries: BTreeMap<String, CalleeEntry> = BTreeMap::new();
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    queue.push_back((args.name.clone(), 0));
    visited.insert(args.name.clone());

    while let Some((current, dist)) = queue.pop_front() {
        if dist >= depth_limit {
            continue;
        }
        let outer: Vec<_> = idx
            .definitions
            .iter()
            .filter(|d| d.name == current && matches!(d.kind, DefKind::Fn | DefKind::Method))
            .collect();
        if outer.is_empty() {
            continue;
        }
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
            let resolutions = resolve_refs(&idx, &r.name);
            let chosen = resolutions.iter().find(|res| {
                res.reference.byte_offset == r.byte_offset && res.reference.file == r.file
            });
            let (confidence, reason, def_file, def_line) = match chosen {
                Some(res) => (
                    res.confidence.as_str(),
                    res.reason.as_str(),
                    res.definition.map(|d| d.file.clone()),
                    res.definition.map(|d| d.line),
                ),
                None => ("low", "name-only", None, None),
            };
            let entry = entries.entry(r.name.clone()).or_insert(CalleeEntry {
                name: r.name.clone(),
                kind: "fn",
                def_file: def_file.clone(),
                def_line,
                distance: dist + 1,
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
            if visited.insert(r.name.clone()) {
                queue.push_back((r.name.clone(), dist + 1));
            }
        }
    }
    let mut out: Vec<CalleeEntry> = entries.into_values().collect();
    out.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.name.cmp(&b.name)));
    if args.json {
        print_json(&out)?;
    } else {
        for e in &out {
            let target = match (&e.def_file, e.def_line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                _ => "<unresolved>".to_string(),
            };
            println!(
                "d={}  {}  -> {}  ({} {}; {} site(s))",
                e.distance,
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
