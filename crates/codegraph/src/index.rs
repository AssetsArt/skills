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
#[allow(dead_code)] // variants wired in Tasks 6/7
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

#[derive(Debug, Default)]
pub struct Index {
    pub definitions: Vec<Definition>,
    #[allow(dead_code)] // wired in Task 6
    pub imports: Vec<Import>,
    #[allow(dead_code)] // wired in Task 7
    pub references: Vec<Reference>,
}

impl Index {
    /// Look up the definition that *contains* `byte_offset` in `file` — i.e. the function
    /// that the byte_offset's reference sits inside. Returns the innermost match.
    #[allow(dead_code)] // used in Tasks 8+
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
        if let Some(q) = f.language.query_source(QueryKind::Defs) {
            index_defs(&mut idx, &source, &rel, f.language, q)?;
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
        // TS/JS/Python wired in later tasks; default to exported=true for now so
        // cross-file resolution does not silently miss things in those languages.
        _ => true,
    }
}
