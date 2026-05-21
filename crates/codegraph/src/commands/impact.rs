use crate::cli::ImpactArgs;
use crate::index::{build_index, DefKind, RefKind};
use crate::output::print_json;
use crate::resolve::resolve_refs;
use serde::Serialize;
use std::collections::{BTreeMap, HashSet, VecDeque};

const MAX_DEPTH: usize = 6;

#[derive(Serialize, Clone)]
struct ImpactEntry {
    name: String,
    kind: &'static str,
    file: String,
    line: usize,
    distance: usize,
    confidence: &'static str,
    reason: &'static str,
}

pub fn run(args: ImpactArgs) -> anyhow::Result<()> {
    let idx = build_index(&args.path)?;

    let mut entries: BTreeMap<(String, String, usize), ImpactEntry> = BTreeMap::new();
    // Seed: the symbol itself (every matching definition, distance=0).
    for d in idx.definitions.iter().filter(|d| d.name == args.name) {
        entries.insert(
            (d.name.clone(), d.file.clone(), d.line),
            ImpactEntry {
                name: d.name.clone(),
                kind: kind_label(d.kind),
                file: d.file.clone(),
                line: d.line,
                distance: 0,
                confidence: "high",
                reason: "same-file-scope",
            },
        );
    }

    let mut queue: VecDeque<(String, usize)> = VecDeque::new();
    let mut visited: HashSet<String> = HashSet::new();
    queue.push_back((args.name.clone(), 0));
    visited.insert(args.name.clone());

    while let Some((current, dist)) = queue.pop_front() {
        if dist >= MAX_DEPTH {
            continue;
        }
        for resolved in resolve_refs(&idx, &current) {
            let Some(enclosing) =
                idx.enclosing_definition(&resolved.reference.file, resolved.reference.byte_offset)
            else {
                continue;
            };
            let key = (
                enclosing.name.clone(),
                enclosing.file.clone(),
                enclosing.line,
            );
            entries.entry(key).or_insert(ImpactEntry {
                name: enclosing.name.clone(),
                kind: kind_label(enclosing.kind),
                file: enclosing.file.clone(),
                line: enclosing.line,
                distance: dist + 1,
                confidence: resolved.confidence.as_str(),
                reason: resolved.reason.as_str(),
            });
            // Recurse on call-kind references only — type-position uses don't propagate impact.
            if resolved.reference.kind == RefKind::Call
                && matches!(enclosing.kind, DefKind::Fn | DefKind::Method)
                && visited.insert(enclosing.name.clone())
            {
                queue.push_back((enclosing.name.clone(), dist + 1));
            }
        }
    }

    let mut out: Vec<ImpactEntry> = entries.into_values().collect();
    out.sort_by(|a, b| {
        a.distance
            .cmp(&b.distance)
            .then(a.file.cmp(&b.file))
            .then(a.line.cmp(&b.line))
    });
    if args.json {
        print_json(&out)?;
    } else {
        for e in &out {
            println!(
                "{:>2}  {}:{}  {:<10} {}  ({} {})",
                e.distance, e.file, e.line, e.kind, e.name, e.confidence, e.reason
            );
        }
    }
    Ok(())
}

fn kind_label(k: DefKind) -> &'static str {
    match k {
        DefKind::Fn => "fn",
        DefKind::Struct => "struct",
        DefKind::Enum => "enum",
        DefKind::Trait => "trait",
        DefKind::Class => "class",
        DefKind::Interface => "interface",
        DefKind::Type => "type",
        DefKind::Const => "const",
        DefKind::Method => "method",
    }
}
