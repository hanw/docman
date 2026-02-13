use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum MetaError {
    #[error("YAML parse error: {0}")]
    Yaml(#[from] serde_yaml::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("missing frontmatter in {path}")]
    MissingFrontmatter { path: String },
}

// ---------------------------------------------------------------------------
// Raw frontmatter
// ---------------------------------------------------------------------------

/// Raw frontmatter deserialized from YAML. All fields optional to handle
/// any document category (active, design, research, archive).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RawFrontmatter {
    pub title: Option<String>,
    pub version: Option<f64>,
    pub status: Option<String>,
    pub created: Option<NaiveDate>,
    pub last_updated: Option<NaiveDate>,
    pub author: Option<String>,
    pub owner: Option<String>,
    pub reviewers: Option<Vec<String>>,
    pub next_review: Option<NaiveDate>,
    pub tags: Option<Vec<String>>,
    pub related_docs: Option<Vec<String>>,
    pub supersedes: Option<String>,
    pub superseded_by: Option<String>,
    // Design doc specific
    pub doc_id: Option<u32>,
    pub decision_date: Option<NaiveDate>,
    pub implementation_pr: Option<u32>,
    pub related_issues: Option<Vec<u32>>,
    // Research specific
    #[serde(rename = "type")]
    pub doc_type: Option<String>,
    pub may_become_design_doc: Option<bool>,
    // Archive specific
    pub archived_date: Option<NaiveDate>,
    pub archived_reason: Option<String>,
    pub historical_value: Option<String>,
}

// ---------------------------------------------------------------------------
// Category
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Active,
    Design,
    Research,
    Archive,
}

impl std::fmt::Display for Category {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Category::Active => write!(f, "active"),
            Category::Design => write!(f, "design"),
            Category::Research => write!(f, "research"),
            Category::Archive => write!(f, "archive"),
        }
    }
}

// ---------------------------------------------------------------------------
// Status enums
// ---------------------------------------------------------------------------

/// Status for active/living documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocStatus {
    Active,
    Deprecated,
    Draft,
}

/// Status for design documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DesignStatus {
    Proposed,
    Accepted,
    Implemented,
    Rejected,
}

/// Status for research documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResearchStatus {
    Draft,
    Published,
    Obsolete,
}

// ---------------------------------------------------------------------------
// Document
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Document {
    pub path: PathBuf,
    pub frontmatter: RawFrontmatter,
    pub category: Category,
    pub body: String,
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub path: PathBuf,
    pub severity: Severity,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Frontmatter extraction
// ---------------------------------------------------------------------------

/// Extract YAML frontmatter and body from markdown content.
///
/// Returns `Some((yaml_str, body_str))` if frontmatter delimiters are found,
/// `None` otherwise.
pub fn extract_frontmatter(content: &str) -> Option<(&str, &str)> {
    // Must start with "---" followed by a newline.
    let rest = content.strip_prefix("---\n")
        .or_else(|| content.strip_prefix("---\r\n"))?;

    // Find the closing "---" on its own line.
    let close = find_closing_delimiter(rest)?;
    let yaml = &rest[..close];
    let after = &rest[close + 3..]; // skip "---"
    // Skip the newline after the closing delimiter.
    let body = after.strip_prefix('\n')
        .or_else(|| after.strip_prefix("\r\n"))
        .unwrap_or(after);
    Some((yaml, body))
}

/// Find the byte offset of a closing `---` that sits on its own line.
fn find_closing_delimiter(s: &str) -> Option<usize> {
    let mut search_from = 0;
    while search_from < s.len() {
        let idx = s[search_from..].find("---")?;
        let abs = search_from + idx;
        // Must be at start of a line (position 0 or preceded by '\n').
        let at_line_start = abs == 0 || s.as_bytes()[abs - 1] == b'\n';
        // Must be followed by newline or EOF.
        let after = abs + 3;
        let at_line_end = after >= s.len()
            || s.as_bytes()[after] == b'\n'
            || s.as_bytes()[after] == b'\r';
        if at_line_start && at_line_end {
            return Some(abs);
        }
        search_from = abs + 3;
    }
    None
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parse a YAML string into `RawFrontmatter`.
pub fn parse_frontmatter(yaml_str: &str) -> Result<RawFrontmatter, MetaError> {
    let fm: RawFrontmatter = serde_yaml::from_str(yaml_str)?;
    Ok(fm)
}

/// Infer document category from its file path.
pub fn infer_category(path: &Path) -> Category {
    let s = path.to_string_lossy();
    // Normalise backslashes for Windows compatibility.
    let norm = s.replace('\\', "/");
    if norm.contains("/active/") || norm.starts_with("active/") {
        Category::Active
    } else if norm.contains("/design/") || norm.starts_with("design/") {
        Category::Design
    } else if norm.contains("/research/") || norm.starts_with("research/") {
        Category::Research
    } else if norm.contains("/archive/") || norm.starts_with("archive/") {
        Category::Archive
    } else {
        Category::Active
    }
}

/// Return a normalised status string for the document given its category.
pub fn resolve_status(raw: &RawFrontmatter, category: Category) -> String {
    let status = raw.status.as_deref().unwrap_or("").to_lowercase();
    match category {
        Category::Active => {
            match status.as_str() {
                "active" | "deprecated" | "draft" => status,
                _ => "active".to_string(),
            }
        }
        Category::Design => {
            match status.as_str() {
                "proposed" | "accepted" | "implemented" | "rejected" => status,
                _ => "proposed".to_string(),
            }
        }
        Category::Research => {
            match status.as_str() {
                "draft" | "published" | "obsolete" => status,
                _ => "draft".to_string(),
            }
        }
        Category::Archive => "archived".to_string(),
    }
}

/// Read a file, parse its frontmatter, and return a `Document`.
pub fn parse_document(path: &Path) -> Result<Document, MetaError> {
    let content = std::fs::read_to_string(path)?;
    let category = infer_category(path);

    let (frontmatter, body) = match extract_frontmatter(&content) {
        Some((yaml, body)) => (parse_frontmatter(yaml)?, body.to_string()),
        None => (RawFrontmatter::default(), content),
    };

    Ok(Document {
        path: path.to_path_buf(),
        frontmatter,
        category,
        body,
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

const VALID_ACTIVE_STATUSES: &[&str] = &["active", "deprecated", "draft"];
const VALID_DESIGN_STATUSES: &[&str] = &["proposed", "accepted", "implemented", "rejected"];
const VALID_RESEARCH_STATUSES: &[&str] = &["draft", "published", "obsolete"];

pub fn validate_frontmatter(doc: &Document) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let p = &doc.path;
    let fm = &doc.frontmatter;

    // No frontmatter at all (title is the simplest sentinel — a truly empty
    // RawFrontmatter has every field as None).
    let all_none = fm.title.is_none()
        && fm.author.is_none()
        && fm.status.is_none()
        && fm.created.is_none()
        && fm.tags.is_none();
    if all_none && !doc.body.is_empty() {
        issues.push(ValidationIssue {
            path: p.clone(),
            severity: Severity::Error,
            message: "no frontmatter found".into(),
        });
        return issues;
    }

    // Missing title
    if fm.title.is_none() {
        issues.push(ValidationIssue {
            path: p.clone(),
            severity: Severity::Error,
            message: "missing title".into(),
        });
    }

    // Missing author
    if fm.author.is_none() {
        issues.push(ValidationIssue {
            path: p.clone(),
            severity: Severity::Warning,
            message: "missing author".into(),
        });
    }

    // Missing created date
    if fm.created.is_none() {
        issues.push(ValidationIssue {
            path: p.clone(),
            severity: Severity::Warning,
            message: "missing created date".into(),
        });
    }

    // Design docs must have doc_id
    if doc.category == Category::Design && fm.doc_id.is_none() {
        issues.push(ValidationIssue {
            path: p.clone(),
            severity: Severity::Error,
            message: "design doc missing doc_id".into(),
        });
    }

    // Active docs should have next_review
    if doc.category == Category::Active && fm.next_review.is_none() {
        issues.push(ValidationIssue {
            path: p.clone(),
            severity: Severity::Warning,
            message: "active doc missing next_review".into(),
        });
    }

    // Invalid status for category
    if let Some(ref status) = fm.status {
        let s = status.to_lowercase();
        let valid = match doc.category {
            Category::Active => VALID_ACTIVE_STATUSES.contains(&s.as_str()),
            Category::Design => VALID_DESIGN_STATUSES.contains(&s.as_str()),
            Category::Research => VALID_RESEARCH_STATUSES.contains(&s.as_str()),
            Category::Archive => true, // any status is fine for archived docs
        };
        if !valid {
            issues.push(ValidationIssue {
                path: p.clone(),
                severity: Severity::Error,
                message: format!("invalid status '{}' for {} category", s, doc.category),
            });
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn extract_frontmatter_returns_yaml_and_body() {
        let content = "---\ntitle: Hello\n---\n\n# Body\n";
        let (yaml, body) = extract_frontmatter(content).unwrap();
        assert_eq!(yaml, "title: Hello\n");
        assert_eq!(body, "\n# Body\n");
    }

    #[test]
    fn extract_frontmatter_returns_none_without_delimiters() {
        let content = "# No frontmatter\nJust text.\n";
        assert!(extract_frontmatter(content).is_none());
    }

    #[test]
    fn extract_frontmatter_handles_crlf() {
        let content = "---\r\ntitle: Hi\r\n---\r\nBody\r\n";
        let (yaml, body) = extract_frontmatter(content).unwrap();
        assert_eq!(yaml, "title: Hi\r\n");
        assert_eq!(body, "Body\r\n");
    }

    #[test]
    fn parse_frontmatter_deserializes_all_fields() {
        let yaml = r#"
title: "Test"
version: 1.5
status: active
created: 2025-06-01
last_updated: 2026-01-15
author: alice
owner: alice
reviewers: [bob, charlie]
next_review: 2026-04-15
tags: [arch, core]
related_docs:
  - some/path.md
doc_id: 42
decision_date: 2026-01-25
implementation_pr: 100
related_issues: [1, 2]
type: research
may_become_design_doc: true
archived_date: 2026-01-01
archived_reason: "old"
historical_value: high
supersedes: old.md
superseded_by: new.md
"#;
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Test"));
        assert_eq!(fm.version, Some(1.5));
        assert_eq!(fm.doc_id, Some(42));
        assert_eq!(fm.implementation_pr, Some(100));
        assert_eq!(fm.reviewers.as_ref().unwrap().len(), 2);
        assert_eq!(fm.doc_type.as_deref(), Some("research"));
        assert_eq!(fm.may_become_design_doc, Some(true));
        assert_eq!(fm.historical_value.as_deref(), Some("high"));
        assert_eq!(fm.supersedes.as_deref(), Some("old.md"));
        assert_eq!(fm.superseded_by.as_deref(), Some("new.md"));
    }

    #[test]
    fn parse_frontmatter_handles_optional_fields() {
        let yaml = "title: Minimal\n";
        let fm = parse_frontmatter(yaml).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Minimal"));
        assert!(fm.version.is_none());
        assert!(fm.doc_id.is_none());
        assert!(fm.tags.is_none());
    }

    #[test]
    fn infer_category_active() {
        assert_eq!(infer_category(Path::new("docs/active/architecture/FOO.md")), Category::Active);
        assert_eq!(infer_category(Path::new("active/FOO.md")), Category::Active);
    }

    #[test]
    fn infer_category_design() {
        assert_eq!(infer_category(Path::new("docs/design/2026/proposed/001.md")), Category::Design);
        assert_eq!(infer_category(Path::new("design/001.md")), Category::Design);
    }

    #[test]
    fn infer_category_research() {
        assert_eq!(infer_category(Path::new("docs/research/2026/survey.md")), Category::Research);
        assert_eq!(infer_category(Path::new("research/survey.md")), Category::Research);
    }

    #[test]
    fn infer_category_archive() {
        assert_eq!(infer_category(Path::new("docs/archive/2025/old.md")), Category::Archive);
        assert_eq!(infer_category(Path::new("archive/old.md")), Category::Archive);
    }

    #[test]
    fn infer_category_defaults_to_active() {
        assert_eq!(infer_category(Path::new("random/path.md")), Category::Active);
        assert_eq!(infer_category(Path::new("README.md")), Category::Active);
    }

    #[test]
    fn resolve_status_per_category() {
        let mut fm = RawFrontmatter::default();
        fm.status = Some("active".into());
        assert_eq!(resolve_status(&fm, Category::Active), "active");

        fm.status = Some("accepted".into());
        assert_eq!(resolve_status(&fm, Category::Design), "accepted");

        fm.status = Some("published".into());
        assert_eq!(resolve_status(&fm, Category::Research), "published");

        fm.status = Some("anything".into());
        assert_eq!(resolve_status(&fm, Category::Archive), "archived");
    }

    #[test]
    fn resolve_status_defaults() {
        let fm = RawFrontmatter::default();
        assert_eq!(resolve_status(&fm, Category::Active), "active");
        assert_eq!(resolve_status(&fm, Category::Design), "proposed");
        assert_eq!(resolve_status(&fm, Category::Research), "draft");
        assert_eq!(resolve_status(&fm, Category::Archive), "archived");
    }

    #[test]
    fn parse_document_reads_fixture() {
        let path = Path::new("tests/fixtures/docs/active/architecture/CORE_CONCEPTS.md");
        // Resolve relative to workspace root.
        let abs = std::env::current_dir().unwrap().join(path);
        // Only run if fixture exists (CI-friendly).
        if !abs.exists() {
            return;
        }
        let doc = parse_document(&abs).unwrap();
        assert_eq!(doc.category, Category::Active);
        assert_eq!(doc.frontmatter.title.as_deref(), Some("Core Concepts"));
        assert!(doc.body.contains("# Core Concepts"));
    }

    #[test]
    fn parse_document_handles_no_frontmatter() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("random").join("bare.md");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        {
            let mut f = std::fs::File::create(&file).unwrap();
            write!(f, "# Just a heading\nSome text.\n").unwrap();
        }
        let doc = parse_document(&file).unwrap();
        assert!(doc.frontmatter.title.is_none());
        assert!(doc.body.contains("# Just a heading"));
    }

    #[test]
    fn validate_detects_missing_title() {
        let doc = Document {
            path: PathBuf::from("docs/active/x.md"),
            frontmatter: RawFrontmatter {
                author: Some("a".into()),
                status: Some("active".into()),
                created: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
                ..Default::default()
            },
            category: Category::Active,
            body: "text".into(),
        };
        let issues = validate_frontmatter(&doc);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("title")));
    }

    #[test]
    fn validate_detects_design_missing_doc_id() {
        let doc = Document {
            path: PathBuf::from("docs/design/x.md"),
            frontmatter: RawFrontmatter {
                title: Some("D".into()),
                author: Some("a".into()),
                status: Some("proposed".into()),
                created: Some(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()),
                ..Default::default()
            },
            category: Category::Design,
            body: "text".into(),
        };
        let issues = validate_frontmatter(&doc);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("doc_id")));
    }

    #[test]
    fn validate_detects_active_missing_next_review() {
        let doc = Document {
            path: PathBuf::from("docs/active/x.md"),
            frontmatter: RawFrontmatter {
                title: Some("A".into()),
                author: Some("a".into()),
                status: Some("active".into()),
                created: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
                ..Default::default()
            },
            category: Category::Active,
            body: "text".into(),
        };
        let issues = validate_frontmatter(&doc);
        assert!(issues.iter().any(|i| i.severity == Severity::Warning && i.message.contains("next_review")));
    }

    #[test]
    fn validate_detects_invalid_status() {
        let doc = Document {
            path: PathBuf::from("docs/active/x.md"),
            frontmatter: RawFrontmatter {
                title: Some("T".into()),
                author: Some("a".into()),
                status: Some("bogus".into()),
                created: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
                next_review: Some(NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()),
                ..Default::default()
            },
            category: Category::Active,
            body: "text".into(),
        };
        let issues = validate_frontmatter(&doc);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("invalid status")));
    }

    #[test]
    fn validate_no_frontmatter_error() {
        let doc = Document {
            path: PathBuf::from("docs/bare.md"),
            frontmatter: RawFrontmatter::default(),
            category: Category::Active,
            body: "# Heading\nSome text".into(),
        };
        let issues = validate_frontmatter(&doc);
        assert!(issues.iter().any(|i| i.severity == Severity::Error && i.message.contains("no frontmatter")));
    }

    #[test]
    fn roundtrip_serialization() {
        let fm = RawFrontmatter {
            title: Some("Round Trip".into()),
            version: Some(1.0),
            status: Some("active".into()),
            tags: Some(vec!["a".into(), "b".into()]),
            ..Default::default()
        };
        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: RawFrontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.title, fm.title);
        assert_eq!(parsed.version, fm.version);
        assert_eq!(parsed.tags, fm.tags);
    }
}
