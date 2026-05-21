use tree_sitter::Language as TsLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryKind {
    Defs,
    Imports,
    Refs,
}

impl Language {
    pub fn name(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::TypeScript => "typescript",
            Language::Tsx => "tsx",
            Language::JavaScript => "javascript",
            Language::Python => "python",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Language::Rust),
            "ts" => Some(Language::TypeScript),
            "tsx" => Some(Language::Tsx),
            "js" | "mjs" | "cjs" => Some(Language::JavaScript),
            "py" => Some(Language::Python),
            _ => None,
        }
    }

    pub fn ts_language(self) -> TsLanguage {
        match self {
            Language::Rust => tree_sitter_rust::language(),
            Language::TypeScript => tree_sitter_typescript::language_typescript(),
            Language::Tsx => tree_sitter_typescript::language_tsx(),
            Language::JavaScript => tree_sitter_javascript::language(),
            Language::Python => tree_sitter_python::language(),
        }
    }

    /// Returns `None` when the query for `(language, kind)` is not wired up yet.
    /// Tasks 3, 6, 8, 16, 17, 18 fill these in.
    pub fn query_source(self, kind: QueryKind) -> Option<&'static str> {
        match (self, kind) {
            (Language::Rust, QueryKind::Defs) => Some(include_str!("queries/rust_defs.scm")),
            (Language::Rust, QueryKind::Imports) => Some(include_str!("queries/rust_imports.scm")),
            (Language::Rust, QueryKind::Refs) => Some(include_str!("queries/rust_refs.scm")),
            (Language::TypeScript | Language::Tsx, QueryKind::Defs) => {
                Some(include_str!("queries/typescript_defs.scm"))
            }
            (Language::TypeScript | Language::Tsx, QueryKind::Imports) => {
                Some(include_str!("queries/typescript_imports.scm"))
            }
            (Language::TypeScript | Language::Tsx, QueryKind::Refs) => {
                Some(include_str!("queries/typescript_refs.scm"))
            }
            _ => None,
        }
    }
}
