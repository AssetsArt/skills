use crate::index::{Definition, Import, Index, Reference};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    High,
    Medium,
    Low,
}

impl Confidence {
    pub fn as_str(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Medium => "medium",
            Confidence::Low => "low",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveReason {
    SameFileScope,
    ImportResolved,
    NameOnly,
}

impl ResolveReason {
    pub fn as_str(self) -> &'static str {
        match self {
            ResolveReason::SameFileScope => "same-file-scope",
            ResolveReason::ImportResolved => "import-resolved",
            ResolveReason::NameOnly => "name-only",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Resolved<'a> {
    pub reference: &'a Reference,
    /// Matched definition (used by callers/callees commands; not yet consumed by find-refs).
    pub definition: Option<&'a Definition>,
    pub confidence: Confidence,
    pub reason: ResolveReason,
}

/// Resolve every reference whose `name == target` against the index.
/// Definitions are matched on `name` only (no signature equality, no generics).
pub fn resolve_refs<'a>(idx: &'a Index, target: &str) -> Vec<Resolved<'a>> {
    let defs_by_name: Vec<&Definition> = idx
        .definitions
        .iter()
        .filter(|d| d.name == target)
        .collect();
    let mut out = Vec::new();
    for r in &idx.references {
        if r.name != target {
            continue;
        }

        // Rule 1: same-file definition?
        if let Some(d) = defs_by_name.iter().find(|d| d.file == r.file) {
            out.push(Resolved {
                reference: r,
                definition: Some(d),
                confidence: Confidence::High,
                reason: ResolveReason::SameFileScope,
            });
            continue;
        }

        // Rule 2: named import in r.file whose local_name == target pointing at a defining file?
        let imports_in_file: Vec<&Import> =
            idx.imports.iter().filter(|i| i.file == r.file).collect();
        let named_import = imports_in_file
            .iter()
            .find(|i| i.local_name == target && i.imported_name != "*");
        if let Some(imp) = named_import {
            if let Some(d) = defs_by_name.iter().find(|d| module_matches(d, imp)) {
                out.push(Resolved {
                    reference: r,
                    definition: Some(d),
                    confidence: Confidence::High,
                    reason: ResolveReason::ImportResolved,
                });
                continue;
            }
        }

        // Rule 3: glob import from a defining file?
        let glob_resolution = imports_in_file
            .iter()
            .filter(|i| i.imported_name == "*")
            .find_map(|imp| {
                defs_by_name
                    .iter()
                    .find(|d| module_matches(d, imp))
                    .copied()
            });
        if let Some(d) = glob_resolution {
            out.push(Resolved {
                reference: r,
                definition: Some(d),
                confidence: Confidence::Medium,
                reason: ResolveReason::ImportResolved,
            });
            continue;
        }

        // Rule 4/5: name-only.
        let conf = if defs_by_name.len() == 1 {
            Confidence::Medium
        } else {
            Confidence::Low
        };
        out.push(Resolved {
            reference: r,
            definition: defs_by_name.first().copied(),
            confidence: conf,
            reason: ResolveReason::NameOnly,
        });
    }
    out
}

/// Heuristic: does `imp.module_path` (e.g. "crate::auth") plausibly point at the file
/// containing `def` (e.g. "src/auth.rs")?
fn module_matches(def: &Definition, imp: &Import) -> bool {
    if imp.module_path.is_empty() {
        return false;
    }
    // Strip the crate root prefix.
    let path = imp
        .module_path
        .trim_start_matches("crate::")
        .trim_start_matches("self::")
        .replace("::", "/");
    let def_file = def.file.replace('\\', "/");
    if path.is_empty() || path == "crate" {
        // `use crate::*` — module_path was just "crate", treat it as the lib root.
        return def_file == "src/lib.rs" || def_file == "src/main.rs" || def_file == "lib.rs";
    }
    let stem_variants = [
        format!("src/{path}.rs"),
        format!("src/{path}/mod.rs"),
        format!("{path}.rs"),
        format!("{path}/mod.rs"),
    ];
    stem_variants.iter().any(|s| s == &def_file)
}
