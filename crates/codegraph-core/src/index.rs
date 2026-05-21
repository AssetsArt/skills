use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DefKind {
    Fn,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Type,
    Const,
    Method,
}

#[derive(Debug, Clone, Serialize)]
pub struct Definition {
    pub file: String,
    pub name: String,
    pub kind: DefKind,
    pub line: usize,
    pub column: usize,
    /// The byte range of the *body* of the definition. Used by `callers`/`callees`
    /// to decide whether a reference site sits inside this definition.
    pub body_start_byte: usize,
    pub body_end_byte: usize,
    /// True if the definition is module-public (Rust `pub`, TS `export`, Python module-level).
    /// Cross-file resolution only matches against exported definitions.
    pub exported: bool,
}

/// One concrete "this name in this file refers to that other module's symbol".
/// Glob imports leave `imported_name = "*"` and the resolver treats them as wildcards.
#[derive(Debug, Clone, Serialize)]
pub struct Import {
    pub file: String,
    pub line: usize,
    /// The local binding the rest of the file uses.
    pub local_name: String,
    /// The name as it exists in the source module (often equal to local_name).
    pub imported_name: String,
    /// The module path string as written in the source (e.g. `crate::auth`, `./util`).
    /// The resolver normalizes this against `file`'s location.
    pub module_path: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RefKind {
    Call,
    Reference,
}

#[derive(Debug, Clone, Serialize)]
pub struct Reference {
    pub file: String,
    pub name: String,
    pub kind: RefKind,
    pub line: usize,
    pub column: usize,
    /// Byte offset into the source — used to test "is this ref inside fn X's body?".
    pub byte_offset: usize,
    /// Line text trimmed to <= 200 chars — included in the output as `context`.
    pub context: String,
}

/// Snapshot of stat-time metadata for one source file. Populated during
/// `build_index`. Used by `astedit` for cheap drift detection between
/// index build and apply.
#[derive(Debug, Clone, Default, Serialize)]
#[non_exhaustive]
pub struct FileMeta {
    pub len: u64,
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct Index {
    pub definitions: Vec<Definition>,
    pub imports: Vec<Import>,
    pub references: Vec<Reference>,
    pub file_meta: std::collections::HashMap<String, FileMeta>,
}

impl Index {
    /// Look up the definition that *contains* `byte_offset` in `file` — i.e. the function
    /// that the byte_offset's reference sits inside. Returns the innermost match.
    pub fn enclosing_definition(&self, file: &str, byte_offset: usize) -> Option<&Definition> {
        self.definitions
            .iter()
            .filter(|d| {
                d.file == file && d.body_start_byte <= byte_offset && byte_offset < d.body_end_byte
            })
            .min_by_key(|d| d.body_end_byte - d.body_start_byte)
    }
}

use crate::lang::{Language, QueryKind};
use crate::walk::walk_sources;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

impl DefKind {
    fn from_capture_suffix(s: &str) -> Option<Self> {
        match s.strip_prefix("def.")? {
            "fn" => Some(DefKind::Fn),
            "struct" => Some(DefKind::Struct),
            "enum" => Some(DefKind::Enum),
            "trait" => Some(DefKind::Trait),
            "class" => Some(DefKind::Class),
            "interface" => Some(DefKind::Interface),
            "type" => Some(DefKind::Type),
            "const" => Some(DefKind::Const),
            "method" => Some(DefKind::Method),
            _ => None,
        }
    }
}

pub fn build_index(root: &Path) -> Result<Index> {
    let mut idx = Index::default();
    for f in walk_sources(root)? {
        let source = match fs::read_to_string(&f.path) {
            Ok(s) => s,
            Err(_) => continue, // unreadable file — skip silently
        };
        let rel = f
            .path
            .strip_prefix(root)
            .unwrap_or(&f.path)
            .to_string_lossy()
            .into_owned();
        idx.file_meta.insert(
            rel.clone(),
            FileMeta { len: source.len() as u64 },
        );
        if let Some(q) = f.language.query_source(QueryKind::Defs) {
            index_defs(&mut idx, &source, &rel, f.language, q)?;
        }
        if let Some(q) = f.language.query_source(QueryKind::Imports) {
            // Best-effort: if the query fails to compile against the live grammar
            // (node kinds drift across tree-sitter releases), skip imports for this
            // file rather than abort the whole index.
            if let Err(e) = index_imports(&mut idx, &source, &rel, f.language, q) {
                eprintln!("codegraph: imports query skipped for {rel}: {e:#}");
            }
        }
        if let Some(q) = f.language.query_source(QueryKind::Refs) {
            if let Err(e) = index_refs(&mut idx, &source, &rel, f.language, q) {
                eprintln!("codegraph: refs query skipped for {rel}: {e}");
            }
        }
    }
    Ok(idx)
}

fn index_defs(
    idx: &mut Index,
    source: &str,
    rel: &str,
    lang: Language,
    query_src: &str,
) -> Result<()> {
    let mut parser = Parser::new();
    let ts = lang.ts_language();
    parser
        .set_language(&ts)
        .with_context(|| format!("set language {}", lang.name()))?;
    let tree = parser
        .parse(source, None)
        .with_context(|| format!("parse {rel}"))?;
    let query = Query::new(&ts, query_src)
        .with_context(|| format!("compile defs query for {}", lang.name()))?;
    let names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut def_node = None;
        let mut def_kind = None;
        let mut name = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            if cname == "name" {
                name = Some(cap.node.utf8_text(bytes).unwrap_or("").to_string());
            } else if let Some(k) = DefKind::from_capture_suffix(cname) {
                def_node = Some(cap.node);
                def_kind = Some(k);
            }
        }
        let (Some(n), Some(node), Some(kind)) = (name, def_node, def_kind) else {
            continue;
        };
        let exported = is_exported(node, bytes, lang);
        idx.definitions.push(Definition {
            file: rel.to_string(),
            name: n,
            kind,
            line: node.start_position().row + 1,
            column: node.start_position().column + 1,
            body_start_byte: node.start_byte(),
            body_end_byte: node.end_byte(),
            exported,
        });
    }
    Ok(())
}

fn index_imports(
    idx: &mut Index,
    source: &str,
    rel: &str,
    lang: Language,
    query_src: &str,
) -> Result<()> {
    let ts = lang.ts_language();
    let mut parser = Parser::new();
    parser.set_language(&ts)?;
    let tree = parser.parse(source, None).context("parse")?;
    let query = Query::new(&ts, query_src).context("compile imports query")?;
    let names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut path_text: Option<String> = None;
        let mut single_name: Option<String> = None;
        let mut alias: Option<String> = None;
        let mut group_node: Option<tree_sitter::Node<'_>> = None;
        let mut import_node: Option<tree_sitter::Node<'_>> = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            let text = cap.node.utf8_text(bytes).unwrap_or("").to_string();
            match cname {
                "path" => path_text = Some(text),
                "name" => single_name = Some(text),
                "alias" => alias = Some(text),
                "group" => group_node = Some(cap.node),
                "import" => import_node = Some(cap.node),
                _ => {}
            }
        }
        let line = import_node.map(|n| n.start_position().row + 1).unwrap_or(0);
        let module_path = path_text.unwrap_or_default();

        match (single_name, alias, group_node) {
            (Some(n), _, _) => {
                idx.imports.push(Import {
                    file: rel.to_string(),
                    line,
                    local_name: n.clone(),
                    imported_name: n,
                    module_path: module_path.clone(),
                });
            }
            (_, Some(a), _) => {
                // `use foo::bar as a;` — alias is the local name, the last segment of path is imported_name.
                let imported = module_path
                    .rsplit("::")
                    .next()
                    .unwrap_or(&module_path)
                    .to_string();
                idx.imports.push(Import {
                    file: rel.to_string(),
                    line,
                    local_name: a,
                    imported_name: imported,
                    module_path,
                });
            }
            (_, _, Some(group)) => {
                // Walk the group node's `(identifier)` and `(use_as_clause)` children.
                let mut cur = group.walk();
                for child in group.children(&mut cur) {
                    match child.kind() {
                        "identifier" => {
                            let nm = child.utf8_text(bytes).unwrap_or("").to_string();
                            idx.imports.push(Import {
                                file: rel.to_string(),
                                line,
                                local_name: nm.clone(),
                                imported_name: nm,
                                module_path: module_path.clone(),
                            });
                        }
                        "use_as_clause" => {
                            // First identifier child = imported, alias child = local.
                            let mut sub = child.walk();
                            let mut kids = child
                                .children(&mut sub)
                                .filter(|n| n.kind() == "identifier");
                            let imp = kids
                                .next()
                                .map(|n| n.utf8_text(bytes).unwrap_or("").to_string())
                                .unwrap_or_default();
                            let als = kids
                                .next()
                                .map(|n| n.utf8_text(bytes).unwrap_or("").to_string())
                                .unwrap_or_else(|| imp.clone());
                            idx.imports.push(Import {
                                file: rel.to_string(),
                                line,
                                local_name: als,
                                imported_name: imp,
                                module_path: module_path.clone(),
                            });
                        }
                        _ => {}
                    }
                }
            }
            _ => {
                // Glob `use foo::*;` — record one wildcard entry.
                idx.imports.push(Import {
                    file: rel.to_string(),
                    line,
                    local_name: "*".to_string(),
                    imported_name: "*".to_string(),
                    module_path,
                });
            }
        }
    }
    Ok(())
}

impl RefKind {
    fn from_capture_suffix(s: &str) -> Option<Self> {
        match s.strip_prefix("ref.")? {
            "call" => Some(RefKind::Call),
            "reference" => Some(RefKind::Reference),
            _ => None,
        }
    }
}

fn index_refs(
    idx: &mut Index,
    source: &str,
    rel: &str,
    lang: Language,
    query_src: &str,
) -> Result<()> {
    let ts = lang.ts_language();
    let mut parser = Parser::new();
    parser.set_language(&ts)?;
    let tree = parser.parse(source, None).context("parse")?;
    let query = Query::new(&ts, query_src).context("compile refs query")?;
    let names = query.capture_names();
    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();

    // Dedupe key: (byte_offset, name). Calls and Reference captures often overlap at the same byte.
    let mut seen: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();

    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut name_node: Option<tree_sitter::Node<'_>> = None;
        let mut ref_kind: Option<RefKind> = None;
        for cap in m.captures {
            let cname = names[cap.index as usize];
            if cname == "name" {
                name_node = Some(cap.node);
            } else if let Some(k) = RefKind::from_capture_suffix(cname) {
                ref_kind = Some(k);
            }
        }
        let (Some(node), Some(kind)) = (name_node, ref_kind) else {
            continue;
        };
        let name = node.utf8_text(bytes).unwrap_or("").to_string();
        let byte_offset = node.start_byte();
        if !seen.insert((byte_offset, name.clone())) {
            // Same site already recorded — keep the first (Call wins over Reference because the query lists calls first).
            continue;
        }
        let line = node.start_position().row + 1;
        let column = node.start_position().column + 1;
        let context = line_at(source, line);
        idx.references.push(Reference {
            file: rel.to_string(),
            name,
            kind,
            line,
            column,
            byte_offset,
            context,
        });
    }
    Ok(())
}

fn line_at(source: &str, line: usize) -> String {
    let raw = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let trimmed = raw.trim();
    if trimmed.chars().count() > 200 {
        trimmed.chars().take(200).collect::<String>() + "…"
    } else {
        trimmed.to_string()
    }
}

fn is_exported(node: tree_sitter::Node<'_>, bytes: &[u8], lang: Language) -> bool {
    match lang {
        Language::Rust => {
            // Look for a `visibility_modifier` child whose text starts with `pub`.
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "visibility_modifier" {
                    let text = child.utf8_text(bytes).unwrap_or("");
                    if text.starts_with("pub") {
                        return true;
                    }
                }
            }
            false
        }
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            // Walk parents looking for an export_statement.
            let mut p = node.parent();
            while let Some(node) = p {
                if node.kind() == "export_statement" || node.kind() == "export_clause" {
                    return true;
                }
                p = node.parent();
            }
            false
        }
        Language::Python => {
            // Module-level definitions are conventionally "public" — we leave this true for now
            // and refine in Task 15 if necessary.
            true
        }
    }
}
