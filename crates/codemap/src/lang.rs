use tree_sitter::Language as TsLanguage;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
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
            Language::Rust => tree_sitter_rust::LANGUAGE.into(),
            Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Language::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Language::Python => tree_sitter_python::LANGUAGE.into(),
        }
    }

    /// Returns `None` for languages whose symbol query hasn't been wired up yet.
    /// `Some("")` is intentionally not a valid state.
    pub fn query_source(self) -> Option<&'static str> {
        match self {
            Language::Rust => Some(include_str!("queries/rust.scm")),
            // The remaining branches are wired up in Tasks 9–11.
            Language::TypeScript | Language::Tsx => Some(include_str!("queries/typescript.scm")),
            Language::JavaScript => Some(include_str!("queries/javascript.scm")),
            Language::Python => Some(include_str!("queries/python.scm")),
        }
    }
}
