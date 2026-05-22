use std::path::Path;

use codegraph_core::lang::Language as CgLang;
use codegraph_core::walk::walk_sources;

use crate::cli::RewriteArgs;
use crate::error::AstEditError;
use crate::output::print_json;
use crate::rewrite::{rewrite_file, RewriteSite};
use crate::serialize::{ErrorEntry, RewriteAppliedFile, RewriteData, RewriteEdit};

pub fn run(args: RewriteArgs) -> anyhow::Result<i32> {
    let root = args.path.as_path();
    let lang_filter = parse_lang_filter(args.lang.as_deref())?;

    let mut applied: Vec<RewriteAppliedFile> = Vec::new();
    let mut errors: Vec<ErrorEntry> = Vec::new();

    let sources = walk_sources(root)?;

    // Pass 1: discover which languages we'd process. `Language` lacks Ord/Hash,
    // so dedup against a Vec — uniqueness is the only property we need.
    let mut langs_to_process: Vec<CgLang> = Vec::new();
    for src in &sources {
        if let Some(target) = lang_filter {
            if src.language != target {
                continue;
            }
        }
        if !langs_to_process.contains(&src.language) {
            langs_to_process.push(src.language);
        }
    }

    // Compile (pattern, rewrite) once per language. Empty source string is a
    // pure compile-only smoke test — ast-grep's `Pattern::try_new` and
    // `TemplateFix::try_new` run before any source is parsed.
    let mut had_compile_failure = false;
    for &lang in &langs_to_process {
        if let Err(e) = rewrite_file("", &args.pattern, &args.rewrite, lang) {
            if matches!(e, AstEditError::PatternCompile { .. }) {
                had_compile_failure = true;
            }
            errors.push(ErrorEntry::from(&e));
        }
    }

    // Pass 2: only walk + match + apply when all languages compiled cleanly.
    if !had_compile_failure {
        for src in sources {
            if let Some(target) = lang_filter {
                if src.language != target {
                    continue;
                }
            }

            let rel = relative_path(&src.path, root);
            let source_text = match std::fs::read_to_string(&src.path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(ErrorEntry::from(&AstEditError::WriteFailed {
                        file: rel.clone(),
                        os_code: e.raw_os_error(),
                        message: e.to_string(),
                    }));
                    continue;
                }
            };

            let sites = match rewrite_file(&source_text, &args.pattern, &args.rewrite, src.language)
            {
                Ok(sites) => sites,
                Err(e) => {
                    errors.push(ErrorEntry::from(&e));
                    continue;
                }
            };

            if sites.is_empty() {
                continue;
            }

            match apply_or_dry_run(&src.path, &rel, &source_text, &sites, args.apply) {
                Ok(entry) => applied.push(entry),
                Err(e) => errors.push(ErrorEntry::from(&e)),
            }
        }
    }

    let data = RewriteData {
        subcommand: "rewrite",
        dry_run: !args.apply,
        applied: Some(applied),
        errors: Some(errors),
    };

    if args.json {
        print_json(data)?;
    }

    Ok(if had_compile_failure { 2 } else { 0 })
}

fn parse_lang_filter(lang: Option<&str>) -> anyhow::Result<Option<CgLang>> {
    match lang {
        None => Ok(None),
        Some(s) => match s {
            "rust" => Ok(Some(CgLang::Rust)),
            "typescript" => Ok(Some(CgLang::TypeScript)),
            "tsx" => Ok(Some(CgLang::Tsx)),
            "javascript" => Ok(Some(CgLang::JavaScript)),
            "python" => Ok(Some(CgLang::Python)),
            other => Err(anyhow::anyhow!(
                "--lang {other:?} not supported; valid: rust, typescript, tsx, javascript, python"
            )),
        },
    }
}

fn relative_path(abs: &Path, root: &Path) -> String {
    abs.strip_prefix(root)
        .unwrap_or(abs)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Materialise `sites` into a `RewriteAppliedFile`. When `apply` is true,
/// also splice the bytes (reverse byte order), guard the race window, and
/// atomically write via `crate::apply::write_atomic`.
fn apply_or_dry_run(
    abs: &Path,
    rel: &str,
    source: &str,
    sites: &[RewriteSite],
    apply: bool,
) -> Result<RewriteAppliedFile, AstEditError> {
    let mut edits: Vec<RewriteEdit> = sites
        .iter()
        .map(|s| RewriteEdit {
            line: s.line,
            col: s.col,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
            old: s.old.clone(),
            new: s.new.clone(),
        })
        .collect();
    edits.sort_by_key(|e| std::cmp::Reverse(e.start_byte));

    let bytes_changed: i64 = edits
        .iter()
        .map(|e| e.new.len() as i64 - e.old.len() as i64)
        .sum();

    if apply {
        let original_len = source.len() as u64;
        let mut bytes = source.as_bytes().to_vec();
        for e in &edits {
            if e.end_byte > bytes.len() || &bytes[e.start_byte..e.end_byte] != e.old.as_bytes() {
                return Err(AstEditError::NodeKindMismatch {
                    file: rel.to_string(),
                    line: e.line,
                    col: e.col,
                });
            }
            bytes.splice(e.start_byte..e.end_byte, e.new.bytes());
        }

        let current = crate::apply::current_len(abs, rel)?;
        if current != original_len {
            return Err(AstEditError::ConcurrentWrite {
                file: rel.to_string(),
            });
        }
        crate::apply::write_atomic(abs, &bytes)?;
    }

    Ok(RewriteAppliedFile {
        file: rel.to_string(),
        bytes_changed,
        edits,
    })
}
