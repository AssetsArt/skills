use crate::cli::FindArgs;
use crate::output::print_json;
use crate::symbols::{extract_file, Symbol};
use crate::walk::walk_sources;
use anyhow::Result;

pub fn run(args: FindArgs) -> Result<()> {
    let mut hits: Vec<Symbol> = Vec::new();
    for f in walk_sources(&args.path)? {
        let rel = f
            .path
            .strip_prefix(&args.path)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        for s in extract_file(&f.path, &rel, f.language)? {
            let matches = if args.exact {
                s.name == args.name
            } else {
                s.name.contains(&args.name)
            };
            if matches {
                hits.push(s);
            }
        }
    }
    hits.sort_by(|a, b| a.file.cmp(&b.file).then(a.start_line.cmp(&b.start_line)));
    if args.json {
        print_json(&hits)?;
    } else {
        for s in &hits {
            println!(
                "{}:{}  {}  {}",
                s.file,
                s.start_line,
                format!("{:?}", s.kind).to_ascii_lowercase(),
                s.name
            );
        }
    }
    Ok(())
}
