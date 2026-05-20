use crate::cli::SymbolsArgs;
use crate::lang::Language;
use crate::output::print_json;
use crate::symbols::{extract_file, Symbol, SymbolKind};
use crate::walk::walk_sources;
use anyhow::{anyhow, Result};
use std::path::Path;

pub fn run(args: SymbolsArgs) -> Result<()> {
    let filter = parse_kind_filter(&args.kind)?;
    let target = args.target.as_deref().unwrap_or(".");
    let whole_project = args.all || target == ".";

    let symbols = if whole_project {
        let root = &args.path;
        let mut all = Vec::new();
        for f in walk_sources(root)? {
            let rel = relative_to(root, &f.path);
            all.extend(extract_file(&f.path, &rel, f.language)?);
        }
        all
    } else {
        // Resolve `target` against `--path` so `codemap symbols src/lib.rs --path /repo`
        // looks at /repo/src/lib.rs, not CWD/src/lib.rs. Absolute targets pass through.
        let path = args.path.join(target);
        let language = path
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Language::from_extension)
            .ok_or_else(|| anyhow!("unsupported file: {}", path.display()))?;
        let rel = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        extract_file(&path, &rel, language)?
    };

    let symbols: Vec<Symbol> = symbols
        .into_iter()
        .filter(|s| filter.is_empty() || filter.contains(&s.kind))
        .collect();

    if args.json {
        print_json(&symbols)?;
    } else {
        for s in &symbols {
            println!(
                "{:>4}-{:<4} {:<9} {}{}",
                s.start_line,
                s.end_line,
                format!("{:?}", s.kind).to_ascii_lowercase(),
                s.name,
                s.signature
                    .as_ref()
                    .map(|sig| format!("    {sig}"))
                    .unwrap_or_default()
            );
        }
    }
    Ok(())
}

fn parse_kind_filter(raw: &[String]) -> Result<Vec<SymbolKind>> {
    let mut out = Vec::new();
    for k in raw {
        out.push(
            SymbolKind::parse(k).ok_or_else(|| anyhow!("unknown --kind value: {k}"))?,
        );
    }
    Ok(out)
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}
