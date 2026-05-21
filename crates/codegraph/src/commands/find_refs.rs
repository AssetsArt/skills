use crate::cli::FindRefsArgs;
use crate::output::print_json;
use codegraph_core::index::{build_index, RefKind};
use codegraph_core::resolve::resolve_refs;
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
    for r in resolve_refs(&idx, &args.name) {
        let kind = match r.reference.kind {
            RefKind::Call => "call",
            RefKind::Reference => "reference",
        };
        hits.push(Hit {
            file: r.reference.file.clone(),
            line: r.reference.line,
            column: r.reference.column,
            kind,
            name: r.reference.name.clone(),
            context: r.reference.context.clone(),
            confidence: r.confidence.as_str(),
            reason: r.reason.as_str(),
        });
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
