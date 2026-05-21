use crate::cli::FindRefsArgs;
use crate::index::{build_index, DefKind};
use crate::output::print_json;
use serde::Serialize;

#[derive(Serialize)]
struct Hit {
    file: String,
    line: usize,
    column: usize,
    kind: &'static str,
    name: String,
    context: String,
    confidence: &'static str,
    reason: &'static str,
}

pub fn run(args: FindRefsArgs) -> anyhow::Result<()> {
    let idx = build_index(&args.path)?;
    let mut hits = Vec::new();
    for d in &idx.definitions {
        if d.name == args.name {
            hits.push(Hit {
                file: d.file.clone(),
                line: d.line,
                column: d.column,
                kind: "definition",
                name: d.name.clone(),
                context: format!("{:?} {}", d.kind, d.name).to_lowercase(),
                confidence: "high",
                reason: "same-file-scope",
            });
        }
    }
    if args.json {
        print_json(&hits)?;
    } else {
        for h in &hits {
            println!(
                "{}:{}:{}  {}  {}  ({} {})",
                h.file, h.line, h.column, h.kind, h.name, h.confidence, h.reason
            );
        }
    }
    Ok(())
}

// Silence "unused" until we start reading non-`fn`/`struct` defs.
#[allow(dead_code)]
fn _kind_label(_: DefKind) {}
