use serde::Serialize;

/// The wire-format payload for `astedit rename`. Wrapped by `output::print_json`
/// in `{schema_version: 1, data: <RenameData>}`.
#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct RenameData {
    pub subcommand: &'static str, // always "rename"
    pub dry_run: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_anchor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidates: Option<Vec<Candidate>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied: Option<Vec<AppliedFile>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<Vec<SkippedSite>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<Vec<ErrorEntry>>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct Candidate {
    pub file: String,
    pub line: usize,
    pub kind: String, // serialized DefKind ("fn", "struct", ...)
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct AppliedFile {
    pub file: String,
    pub bytes_changed: i64,
    pub edits: Vec<AppliedEdit>,
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct AppliedEdit {
    pub line: usize,
    pub col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub old: String,
    pub new: String,
    pub confidence: &'static str, // "high" | "medium"
    pub reason: &'static str,     // ResolveReason::as_str()
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct SkippedSite {
    pub file: String,
    pub line: usize,
    pub col: usize,
    pub start_byte: usize,
    pub end_byte: usize,
    pub name: String,
    pub confidence: &'static str,
    pub reason: &'static str,
    pub skip_reason: &'static str, // "low-confidence" | "re-export-alias" | "wildcard-reexport"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_alias: Option<String>, // only for skip_reason == "re-export-alias"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub via_module: Option<String>, // only for skip_reason == "wildcard-reexport"
}

#[derive(Debug, Serialize)]
#[allow(dead_code)]
pub struct ErrorEntry {
    pub error_kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl From<&crate::error::AstEditError> for ErrorEntry {
    fn from(e: &crate::error::AstEditError) -> Self {
        use crate::error::AstEditError as E;
        let mut entry = ErrorEntry {
            error_kind: e.kind(),
            file: e.file().map(|s| s.to_string()),
            message: None,
            os_code: None,
            line: None,
            col: None,
            lang: None,
        };
        match e {
            E::ParseError { message, .. } => entry.message = Some(message.clone()),
            E::HashMismatch { .. } => {}
            E::ConcurrentWrite { .. } => {}
            E::NodeKindMismatch { line, col, .. } => {
                entry.line = Some(*line);
                entry.col = Some(*col);
            }
            E::WriteFailed {
                os_code, message, ..
            } => {
                entry.os_code = *os_code;
                entry.message = Some(message.clone());
            }
            E::PatternCompile { lang, message } => {
                entry.lang = Some(lang.clone());
                entry.message = Some(message.clone());
            }
        }
        entry
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn rename_data_serializes_omitting_none_fields() {
        let data = RenameData {
            subcommand: "rename",
            dry_run: true,
            needs_anchor: None,
            candidates: None,
            applied: Some(vec![]),
            skipped: Some(vec![]),
            errors: Some(vec![]),
        };
        let v: Value = serde_json::to_value(&data).unwrap();
        assert_eq!(v["subcommand"], "rename");
        assert_eq!(v["dry_run"], true);
        assert!(v.get("needs_anchor").is_none());
        assert!(v.get("candidates").is_none());
        assert!(v["applied"].is_array());
        assert!(v["skipped"].is_array());
        assert!(v["errors"].is_array());
    }

    #[test]
    fn error_entry_from_write_failed_carries_os_code() {
        let err = crate::error::AstEditError::WriteFailed {
            file: "src/x.rs".into(),
            os_code: Some(13),
            message: "permission denied".into(),
        };
        let entry: ErrorEntry = (&err).into();
        let v = serde_json::to_value(&entry).unwrap();
        assert_eq!(v["error_kind"], "write-failed");
        assert_eq!(v["file"], "src/x.rs");
        assert_eq!(v["os_code"], 13);
        assert_eq!(v["message"], "permission denied");
    }

    #[test]
    fn skipped_site_alias_carries_via_alias_only() {
        let s = SkippedSite {
            file: "src/lib.rs".into(),
            line: 4,
            col: 0,
            start_byte: 10,
            end_byte: 14,
            name: "User".into(),
            confidence: "high",
            reason: "same-file-scope",
            skip_reason: "re-export-alias",
            via_alias: Some("Account".into()),
            via_module: None,
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["skip_reason"], "re-export-alias");
        assert_eq!(v["via_alias"], "Account");
        assert!(v.get("via_module").is_none());
    }
}
