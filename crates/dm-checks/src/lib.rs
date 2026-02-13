use std::path::PathBuf;

use chrono::NaiveDate;
use dm_meta::{Category, Severity};
use dm_scan::DocTree;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// The type of health check that produced an issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckType {
    Stale,
    Orphan,
    BrokenLink,
    MissingFrontmatter,
    InvalidMetadata,
}

impl std::fmt::Display for CheckType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckType::Stale => write!(f, "stale"),
            CheckType::Orphan => write!(f, "orphan"),
            CheckType::BrokenLink => write!(f, "broken_link"),
            CheckType::MissingFrontmatter => write!(f, "missing_frontmatter"),
            CheckType::InvalidMetadata => write!(f, "invalid_metadata"),
        }
    }
}

/// A single issue found during a health check.
#[derive(Debug, Clone)]
pub struct CheckIssue {
    pub path: PathBuf,
    pub check_type: CheckType,
    pub severity: Severity,
    pub message: String,
}

/// Aggregated results from all health checks.
#[derive(Debug, Clone)]
pub struct CheckReport {
    pub issues: Vec<CheckIssue>,
    pub docs_checked: usize,
    pub timestamp: NaiveDate,
}

impl CheckReport {
    /// Returns true if any issue has Error severity.
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Error)
    }

    /// Returns true if any issue has Warning severity.
    pub fn has_warnings(&self) -> bool {
        self.issues.iter().any(|i| i.severity == Severity::Warning)
    }

    /// Count of issues with Error severity.
    pub fn error_count(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == Severity::Error).count()
    }

    /// Count of issues with Warning severity.
    pub fn warning_count(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == Severity::Warning).count()
    }

    /// Count of issues with Info severity.
    pub fn info_count(&self) -> usize {
        self.issues.iter().filter(|i| i.severity == Severity::Info).count()
    }
}

// ---------------------------------------------------------------------------
// Staleness detection
// ---------------------------------------------------------------------------

/// Detect stale documents: overdue reviews, old last-updated dates, missing review dates.
pub fn check_stale(tree: &DocTree, today: NaiveDate) -> Vec<CheckIssue> {
    let mut issues = Vec::new();

    for doc in tree.all() {
        // Review overdue
        if let Some(next_review) = doc.frontmatter.next_review {
            if today > next_review {
                issues.push(CheckIssue {
                    path: doc.path.clone(),
                    check_type: CheckType::Stale,
                    severity: Severity::Warning,
                    message: format!("Review overdue since {next_review}"),
                });
            }
        }

        // Not updated in >180 days
        if let Some(last_updated) = doc.frontmatter.last_updated {
            let days_since = (today - last_updated).num_days();
            if days_since > 180 {
                issues.push(CheckIssue {
                    path: doc.path.clone(),
                    check_type: CheckType::Stale,
                    severity: Severity::Warning,
                    message: format!("Not updated in over 6 months (last: {last_updated})"),
                });
            }
        }

        // Active docs without next_review
        if doc.category == Category::Active && doc.frontmatter.next_review.is_none() {
            issues.push(CheckIssue {
                path: doc.path.clone(),
                check_type: CheckType::Stale,
                severity: Severity::Info,
                message: "No review date set".into(),
            });
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Orphan detection
// ---------------------------------------------------------------------------

/// Detect orphaned design documents: accepted without PRs, stale acceptances.
pub fn check_orphans(tree: &DocTree) -> Vec<CheckIssue> {
    let today = chrono::Local::now().date_naive();
    check_orphans_with_date(tree, today)
}

fn check_orphans_with_date(tree: &DocTree, today: NaiveDate) -> Vec<CheckIssue> {
    let mut issues = Vec::new();
    let design_docs = tree.by_category(Category::Design);

    for doc in &design_docs {
        let status = doc.frontmatter.status.as_deref().unwrap_or("").to_lowercase();

        if status == "accepted" {
            // Missing implementation PR
            if doc.frontmatter.implementation_pr.is_none() {
                issues.push(CheckIssue {
                    path: doc.path.clone(),
                    check_type: CheckType::Orphan,
                    severity: Severity::Warning,
                    message: "Accepted design doc has no implementation PR".into(),
                });
            }

            // Accepted >90 days without implementation
            if let Some(decision_date) = doc.frontmatter.decision_date {
                let days = (today - decision_date).num_days();
                if days > 90 {
                    issues.push(CheckIssue {
                        path: doc.path.clone(),
                        check_type: CheckType::Orphan,
                        severity: Severity::Warning,
                        message: "Accepted >90 days without implementation".into(),
                    });
                }
            }
        }

        if status == "implemented" {
            // Check if referenced by any active document
            let active_docs = tree.by_category(Category::Active);
            let doc_path_str = doc.path.to_string_lossy();
            let is_referenced = active_docs.iter().any(|active| {
                active.frontmatter.related_docs.as_deref().unwrap_or(&[])
                    .iter()
                    .any(|rd| doc_path_str.contains(rd) || rd.contains(&*doc_path_str))
            });
            if !is_referenced {
                issues.push(CheckIssue {
                    path: doc.path.clone(),
                    check_type: CheckType::Orphan,
                    severity: Severity::Info,
                    message: "Implemented design doc not referenced by any active doc".into(),
                });
            }
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Broken link detection
// ---------------------------------------------------------------------------

/// Detect broken cross-references in related_docs, supersedes, and superseded_by fields.
pub fn check_broken_links(tree: &DocTree) -> Vec<CheckIssue> {
    let mut issues = Vec::new();

    // Collect all relative paths present in the tree for lookup.
    let known_paths: Vec<String> = tree.all().iter()
        .map(|d| {
            d.path
                .strip_prefix(&tree.root)
                .unwrap_or(&d.path)
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();

    // Also check by absolute path existence.
    let path_exists = |link: &str| -> bool {
        // Check in known relative paths.
        if known_paths.iter().any(|p| p == link || p.ends_with(link) || link.ends_with(p.as_str())) {
            return true;
        }
        // Check as path relative to docs root.
        let candidate = tree.root.join(link);
        if candidate.exists() {
            return true;
        }
        // Try stripping common prefixes like "docs/"
        if let Some(stripped) = link.strip_prefix("docs/") {
            if known_paths.iter().any(|p| p == stripped || p.ends_with(stripped)) {
                return true;
            }
            if tree.root.join(stripped).exists() {
                return true;
            }
        }
        false
    };

    for doc in tree.all() {
        // Check related_docs
        if let Some(ref related) = doc.frontmatter.related_docs {
            for link in related {
                if !path_exists(link) {
                    issues.push(CheckIssue {
                        path: doc.path.clone(),
                        check_type: CheckType::BrokenLink,
                        severity: Severity::Error,
                        message: format!("Broken link: {link} does not exist"),
                    });
                }
            }
        }

        // Check supersedes
        if let Some(ref target) = doc.frontmatter.supersedes {
            if !path_exists(target) {
                issues.push(CheckIssue {
                    path: doc.path.clone(),
                    check_type: CheckType::BrokenLink,
                    severity: Severity::Error,
                    message: format!("Supersedes target not found: {target}"),
                });
            }
        }

        // Check superseded_by
        if let Some(ref target) = doc.frontmatter.superseded_by {
            if !path_exists(target) {
                issues.push(CheckIssue {
                    path: doc.path.clone(),
                    check_type: CheckType::BrokenLink,
                    severity: Severity::Error,
                    message: format!("Superseded_by target not found: {target}"),
                });
            }
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Frontmatter checks
// ---------------------------------------------------------------------------

/// Validate frontmatter for all documents and convert issues to CheckIssues.
pub fn check_frontmatter(tree: &DocTree) -> Vec<CheckIssue> {
    let mut issues = Vec::new();

    for doc in tree.all() {
        let validation_issues = dm_meta::validate_frontmatter(doc);
        for vi in validation_issues {
            let check_type = if vi.message.contains("no frontmatter") {
                CheckType::MissingFrontmatter
            } else {
                CheckType::InvalidMetadata
            };
            issues.push(CheckIssue {
                path: vi.path,
                check_type,
                severity: vi.severity,
                message: vi.message,
            });
        }
    }

    issues
}

// ---------------------------------------------------------------------------
// Combined check
// ---------------------------------------------------------------------------

/// Run all health checks and return an aggregated report.
pub fn run_all_checks(tree: &DocTree) -> CheckReport {
    let today = chrono::Local::now().date_naive();
    run_all_checks_with_date(tree, today)
}

fn run_all_checks_with_date(tree: &DocTree, today: NaiveDate) -> CheckReport {
    let mut issues = Vec::new();
    issues.extend(check_stale(tree, today));
    issues.extend(check_orphans_with_date(tree, today));
    issues.extend(check_broken_links(tree));
    issues.extend(check_frontmatter(tree));

    CheckReport {
        docs_checked: tree.all().len(),
        issues,
        timestamp: today,
    }
}

// ---------------------------------------------------------------------------
// Report formatting
// ---------------------------------------------------------------------------

/// Format a check report as human-readable text.
pub fn format_report(report: &CheckReport) -> String {
    let mut out = String::new();
    out.push_str("Document Health Check Report\n");
    out.push_str("===========================\n");
    out.push_str(&format!("Checked: {} documents\n", report.docs_checked));
    out.push_str(&format!(
        "Errors: {} | Warnings: {} | Info: {}\n",
        report.error_count(),
        report.warning_count(),
        report.info_count()
    ));

    let errors: Vec<&CheckIssue> = report.issues.iter()
        .filter(|i| i.severity == Severity::Error).collect();
    if !errors.is_empty() {
        out.push_str("\nERRORS:\n");
        for issue in errors {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                issue.check_type,
                issue.path.display(),
                issue.message
            ));
        }
    }

    let warnings: Vec<&CheckIssue> = report.issues.iter()
        .filter(|i| i.severity == Severity::Warning).collect();
    if !warnings.is_empty() {
        out.push_str("\nWARNINGS:\n");
        for issue in warnings {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                issue.check_type,
                issue.path.display(),
                issue.message
            ));
        }
    }

    let infos: Vec<&CheckIssue> = report.issues.iter()
        .filter(|i| i.severity == Severity::Info).collect();
    if !infos.is_empty() {
        out.push_str("\nINFO:\n");
        for issue in infos {
            out.push_str(&format!(
                "  [{}] {}: {}\n",
                issue.check_type,
                issue.path.display(),
                issue.message
            ));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use dm_meta::{Document, RawFrontmatter};

    fn fixtures_root() -> PathBuf {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest.parent().unwrap().parent().unwrap();
        let root = workspace.join("tests/fixtures/docs");
        assert!(root.exists(), "fixtures dir not found at {}", root.display());
        root
    }

    fn scan_fixtures() -> DocTree {
        DocTree::scan(&fixtures_root())
    }

    #[test]
    fn stale_detects_past_next_review() {
        let tree = scan_fixtures();
        // GETTING_STARTED has next_review: 2026-01-01
        let today = NaiveDate::from_ymd_opt(2026, 2, 12).unwrap();
        let issues = check_stale(&tree, today);
        assert!(
            issues.iter().any(|i| i.check_type == CheckType::Stale
                && i.message.contains("Review overdue")),
            "should detect overdue review"
        );
    }

    #[test]
    fn stale_detects_old_last_updated() {
        let tree = scan_fixtures();
        // CLI_REFERENCE last_updated: 2025-09-15, so >180 days from 2026-06-01
        let today = NaiveDate::from_ymd_opt(2026, 6, 1).unwrap();
        let issues = check_stale(&tree, today);
        assert!(
            issues.iter().any(|i| i.check_type == CheckType::Stale
                && i.message.contains("Not updated in over 6 months")),
            "should detect doc not updated in >180 days"
        );
    }

    #[test]
    fn stale_ignores_future_next_review() {
        let tree = scan_fixtures();
        // CORE_CONCEPTS has next_review: 2026-04-15
        let today = NaiveDate::from_ymd_opt(2026, 2, 12).unwrap();
        let issues = check_stale(&tree, today);
        // Should NOT flag CORE_CONCEPTS as overdue
        let core_stale = issues.iter().any(|i| {
            i.path.to_string_lossy().contains("CORE_CONCEPTS")
                && i.message.contains("Review overdue")
        });
        assert!(!core_stale, "should not flag doc with future next_review");
    }

    #[test]
    fn orphans_detects_accepted_without_pr() {
        let tree = scan_fixtures();
        let issues = check_orphans(&tree);
        // 002-context-fidelity is accepted with no implementation_pr
        assert!(
            issues.iter().any(|i| i.check_type == CheckType::Orphan
                && i.message.contains("no implementation PR")),
            "should detect accepted design doc without PR"
        );
    }

    #[test]
    fn orphans_ignores_proposed() {
        let tree = scan_fixtures();
        let issues = check_orphans(&tree);
        // 001-recursive-optimization is proposed — should NOT be flagged as orphan
        let proposed_flagged = issues.iter().any(|i| {
            i.path.to_string_lossy().contains("001-recursive-optimization")
                && i.check_type == CheckType::Orphan
        });
        assert!(!proposed_flagged, "should not flag proposed design docs");
    }

    #[test]
    fn broken_links_detects_nonexistent() {
        // Build a minimal DocTree with a broken related_docs link
        let tree = DocTree {
            docs: vec![Document {
                path: PathBuf::from("/tmp/test/active/x.md"),
                frontmatter: RawFrontmatter {
                    title: Some("X".into()),
                    related_docs: Some(vec!["nonexistent/foo.md".into()]),
                    ..Default::default()
                },
                category: Category::Active,
                body: String::new(),
            }],
            errors: vec![],
            root: PathBuf::from("/tmp/test"),
        };
        let issues = check_broken_links(&tree);
        assert!(
            issues.iter().any(|i| i.check_type == CheckType::BrokenLink
                && i.message.contains("does not exist")),
            "should detect broken link"
        );
    }

    #[test]
    fn broken_links_passes_when_valid() {
        let tree = scan_fixtures();
        let issues = check_broken_links(&tree);
        // All related_docs in fixtures use "docs/active/..." prefixed paths
        // which should resolve. Filter only actual broken links.
        let broken = issues.iter().filter(|i| i.check_type == CheckType::BrokenLink).count();
        // We expect 0 broken links in the fixtures (all cross-refs are valid)
        assert_eq!(broken, 0, "fixture cross-refs should all resolve, got {broken} broken");
    }

    #[test]
    fn frontmatter_detects_missing_title() {
        let tree = DocTree {
            docs: vec![Document {
                path: PathBuf::from("/tmp/test/active/notitle.md"),
                frontmatter: RawFrontmatter {
                    author: Some("a".into()),
                    status: Some("active".into()),
                    created: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
                    ..Default::default()
                },
                category: Category::Active,
                body: "some body".into(),
            }],
            errors: vec![],
            root: PathBuf::from("/tmp/test"),
        };
        let issues = check_frontmatter(&tree);
        assert!(
            issues.iter().any(|i| i.message.contains("missing title")),
            "should detect missing title"
        );
    }

    #[test]
    fn run_all_checks_combines_issues() {
        let tree = scan_fixtures();
        let today = NaiveDate::from_ymd_opt(2026, 2, 12).unwrap();
        let report = run_all_checks_with_date(&tree, today);
        assert!(report.docs_checked >= 9);
        // Should have at least one issue (stale review on GETTING_STARTED at minimum)
        assert!(!report.issues.is_empty(), "should find at least one issue");
    }

    #[test]
    fn report_counts_correct() {
        let report = CheckReport {
            issues: vec![
                CheckIssue {
                    path: PathBuf::from("a.md"),
                    check_type: CheckType::Stale,
                    severity: Severity::Error,
                    message: "err".into(),
                },
                CheckIssue {
                    path: PathBuf::from("b.md"),
                    check_type: CheckType::Orphan,
                    severity: Severity::Warning,
                    message: "warn".into(),
                },
                CheckIssue {
                    path: PathBuf::from("c.md"),
                    check_type: CheckType::Stale,
                    severity: Severity::Warning,
                    message: "warn2".into(),
                },
                CheckIssue {
                    path: PathBuf::from("d.md"),
                    check_type: CheckType::Stale,
                    severity: Severity::Info,
                    message: "info".into(),
                },
            ],
            docs_checked: 4,
            timestamp: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        };
        assert_eq!(report.error_count(), 1);
        assert_eq!(report.warning_count(), 2);
        assert_eq!(report.info_count(), 1);
        assert!(report.has_errors());
        assert!(report.has_warnings());
    }

    #[test]
    fn format_report_structure() {
        let report = CheckReport {
            issues: vec![
                CheckIssue {
                    path: PathBuf::from("test.md"),
                    check_type: CheckType::Stale,
                    severity: Severity::Error,
                    message: "test error".into(),
                },
                CheckIssue {
                    path: PathBuf::from("test2.md"),
                    check_type: CheckType::Orphan,
                    severity: Severity::Warning,
                    message: "test warning".into(),
                },
            ],
            docs_checked: 5,
            timestamp: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
        };
        let output = format_report(&report);
        assert!(output.contains("Document Health Check Report"));
        assert!(output.contains("Checked: 5 documents"));
        assert!(output.contains("Errors: 1"));
        assert!(output.contains("Warnings: 1"));
        assert!(output.contains("ERRORS:"));
        assert!(output.contains("[stale] test.md: test error"));
        assert!(output.contains("WARNINGS:"));
        assert!(output.contains("[orphan] test2.md: test warning"));
    }
}
