use crate::cli::StatsArgs;
use crate::output::print_json;
use crate::symbols::{extract_file, SymbolKind};
use crate::walk::walk_sources;
use anyhow::Result;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;

#[derive(Serialize, Default)]
struct LangStats {
    files: usize,
    lines: usize,
}

#[derive(Serialize)]
struct Report {
    total_files: usize,
    total_lines: usize,
    languages: BTreeMap<&'static str, LangStats>,
    symbols: BTreeMap<&'static str, usize>,
}

pub fn run(args: StatsArgs) -> Result<()> {
    let files = walk_sources(&args.path)?;
    let mut total_files = 0usize;
    let mut total_lines = 0usize;
    let mut languages: BTreeMap<&'static str, LangStats> = BTreeMap::new();
    let mut symbols: BTreeMap<&'static str, usize> = BTreeMap::new();

    for f in &files {
        total_files += 1;
        let lines = fs::read_to_string(&f.path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        total_lines += lines;
        let entry = languages.entry(f.language.name()).or_default();
        entry.files += 1;
        entry.lines += lines;

        let rel = f
            .path
            .strip_prefix(&args.path)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        for s in extract_file(&f.path, &rel, f.language)? {
            let key: &'static str = match s.kind {
                SymbolKind::Fn => "fn",
                SymbolKind::Struct => "struct",
                SymbolKind::Enum => "enum",
                SymbolKind::Trait => "trait",
                SymbolKind::Class => "class",
                SymbolKind::Interface => "interface",
                SymbolKind::Type => "type",
                SymbolKind::Const => "const",
            };
            *symbols.entry(key).or_insert(0) += 1;
        }
    }

    let report = Report {
        total_files,
        total_lines,
        languages,
        symbols,
    };
    if args.json {
        print_json(&report)?;
    } else {
        println!("files: {}", report.total_files);
        println!("lines: {}", report.total_lines);
        println!("languages:");
        for (lang, ls) in &report.languages {
            println!("  {lang:<12} {} files  {} lines", ls.files, ls.lines);
        }
        println!("symbols:");
        for (k, n) in &report.symbols {
            println!("  {k:<10} {n}");
        }
    }
    Ok(())
}
