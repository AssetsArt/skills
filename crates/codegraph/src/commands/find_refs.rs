use crate::cli::FindRefsArgs;
use crate::index::{build_index, RefKind};
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
    for r in &idx.references {
        if r.name == args.name {
            let kind = match r.kind {
                RefKind::Call => "call",
                RefKind::Reference => "reference",
            };
            hits.push(Hit {
                file: r.file.clone(),
                line: r.line,
                column: r.column,
                kind,
                name: r.name.clone(),
                context: r.context.clone(),
                confidence: "low",
                reason: "name-only",
            });
        }
    }
    hits.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));
    if args.json {
        print_json(&hits)?;
    } else {
        for h in &hits {
            println!(
                "{}:{}:{}  {:<10} {:<6} {}",
                h.file, h.line, h.column, h.kind, h.confidence, h.context
            );
        }
    }
    Ok(())
}
