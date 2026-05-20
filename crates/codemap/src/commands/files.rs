use crate::cli::FilesArgs;
use crate::output::print_json;
use crate::walk::walk_sources;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;

#[derive(Serialize)]
struct FileEntry {
    path: String,
    language: &'static str,
    lines: usize,
    size_bytes: u64,
}

pub fn run(args: FilesArgs) -> anyhow::Result<()> {
    let files = walk_sources(&args.path)?;
    let mut entries = Vec::with_capacity(files.len());
    for f in &files {
        let rel = f
            .path
            .strip_prefix(&args.path)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        let meta = fs::metadata(&f.path)?;
        let lines = fs::read_to_string(&f.path)
            .map(|s| s.lines().count())
            .unwrap_or(0);
        entries.push(FileEntry {
            path: rel,
            language: f.language.name(),
            lines,
            size_bytes: meta.len(),
        });
    }

    if args.json {
        print_json(&entries)?;
    } else {
        let mut by_lang: BTreeMap<&'static str, Vec<&FileEntry>> = BTreeMap::new();
        for e in &entries {
            by_lang.entry(e.language).or_default().push(e);
        }
        for (lang, group) in by_lang {
            println!("{lang} ({} files)", group.len());
            for e in group {
                println!("  {}  {} lines  {} B", e.path, e.lines, e.size_bytes);
            }
        }
    }
    Ok(())
}
