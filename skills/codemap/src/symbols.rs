use crate::lang::Language;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;
use tree_sitter::{Parser, Query, QueryCursor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Fn,
    Struct,
    Enum,
    Trait,
    Class,
    Interface,
    Type,
    Const,
}

impl SymbolKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fn" | "function" => Some(SymbolKind::Fn),
            "struct" => Some(SymbolKind::Struct),
            "enum" => Some(SymbolKind::Enum),
            "trait" => Some(SymbolKind::Trait),
            "class" => Some(SymbolKind::Class),
            "interface" => Some(SymbolKind::Interface),
            "type" => Some(SymbolKind::Type),
            "const" => Some(SymbolKind::Const),
            _ => None,
        }
    }

    fn from_capture_suffix(s: &str) -> Option<Self> {
        let suffix = s.strip_prefix("symbol.")?;
        Self::parse(suffix)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Symbol {
    pub file: String,
    pub name: String,
    pub kind: SymbolKind,
    pub start_line: usize,
    pub end_line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Extract symbols from one file. `rel_path` is stored on each `Symbol.file`.
/// Returns an empty vec when the language has no wired-up query (= unsupported here, not just no matches).
pub fn extract_file(path: &Path, rel_path: &str, language: Language) -> Result<Vec<Symbol>> {
    let Some(query_src) = language.query_source() else {
        return Ok(Vec::new());
    };
    let source = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let mut parser = Parser::new();
    parser
        .set_language(&language.ts_language())
        .with_context(|| format!("set language {}", language.name()))?;
    let tree = parser
        .parse(&source, None)
        .with_context(|| format!("parse {}", path.display()))?;
    let query = Query::new(&language.ts_language(), query_src)
        .with_context(|| format!("compile query for {}", language.name()))?;
    let capture_names = query.capture_names();

    let mut cursor = QueryCursor::new();
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    for m in cursor.matches(&query, tree.root_node(), bytes) {
        let mut symbol_node = None;
        let mut symbol_kind = None;
        let mut name = None;
        for cap in m.captures {
            let cname = capture_names[cap.index as usize];
            if cname == "name" {
                name = Some(cap.node.utf8_text(bytes).unwrap_or("").to_string());
            } else if let Some(k) = SymbolKind::from_capture_suffix(cname) {
                symbol_node = Some(cap.node);
                symbol_kind = Some(k);
            }
        }
        let (Some(n), Some(node), Some(kind)) = (name, symbol_node, symbol_kind) else {
            continue;
        };
        let start_line = node.start_position().row + 1;
        let end_line = node.end_position().row + 1;
        let signature = first_line_of(node, bytes).map(|s| truncate(s, 120));
        out.push(Symbol {
            file: rel_path.to_string(),
            name: n,
            kind,
            start_line,
            end_line,
            signature,
        });
    }
    out.sort_by_key(|s| s.start_line);
    Ok(out)
}

fn first_line_of(node: tree_sitter::Node<'_>, src: &[u8]) -> Option<String> {
    let text = node.utf8_text(src).ok()?;
    Some(text.lines().next()?.trim().to_string())
}

fn truncate(mut s: String, max: usize) -> String {
    if s.chars().count() > max {
        s = s.chars().take(max).collect::<String>() + "…";
    }
    s
}
