use std::collections::HashMap;
use std::path::{Path, PathBuf};

use dm_meta::{Category, Document};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ScanError {
    pub path: PathBuf,
    pub message: String,
}

impl std::fmt::Display for ScanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

// ---------------------------------------------------------------------------
// ScanFilter
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ScanFilter {
    pub categories: Option<Vec<Category>>,
    pub tags: Option<Vec<String>>,
    pub status: Option<String>,
    pub author: Option<String>,
}

impl ScanFilter {
    pub fn matches(&self, doc: &Document) -> bool {
        if let Some(ref cats) = self.categories {
            if !cats.contains(&doc.category) {
                return false;
            }
        }
        if let Some(ref tags) = self.tags {
            let doc_tags = doc.frontmatter.tags.as_deref().unwrap_or(&[]);
            if !tags.iter().any(|t| doc_tags.iter().any(|dt| dt.eq_ignore_ascii_case(t))) {
                return false;
            }
        }
        if let Some(ref status) = self.status {
            let doc_status = doc.frontmatter.status.as_deref().unwrap_or("");
            if !doc_status.eq_ignore_ascii_case(status) {
                return false;
            }
        }
        if let Some(ref author) = self.author {
            let doc_author = doc.frontmatter.author.as_deref().unwrap_or("");
            if !doc_author.eq_ignore_ascii_case(author) {
                return false;
            }
        }
        true
    }
}

// ---------------------------------------------------------------------------
// DocTree
// ---------------------------------------------------------------------------

pub struct DocTree {
    pub docs: Vec<Document>,
    pub errors: Vec<ScanError>,
    pub root: PathBuf,
}

impl DocTree {
    /// Scan a directory for all markdown files and parse them.
    pub fn scan(root: &Path) -> Self {
        Self::scan_filtered(root, &ScanFilter::default())
    }

    /// Scan with a filter applied.
    pub fn scan_filtered(root: &Path, filter: &ScanFilter) -> Self {
        let mut docs = Vec::new();
        let mut errors = Vec::new();

        let pattern = format!("{}/**/*.md", root.display());
        let entries = match glob::glob(&pattern) {
            Ok(paths) => paths,
            Err(e) => {
                errors.push(ScanError {
                    path: root.to_path_buf(),
                    message: format!("glob error: {e}"),
                });
                return DocTree { docs, errors, root: root.to_path_buf() };
            }
        };

        for entry in entries {
            match entry {
                Ok(path) => {
                    match dm_meta::parse_document(&path) {
                        Ok(doc) => {
                            if filter.matches(&doc) {
                                docs.push(doc);
                            }
                        }
                        Err(e) => {
                            errors.push(ScanError {
                                path,
                                message: e.to_string(),
                            });
                        }
                    }
                }
                Err(e) => {
                    errors.push(ScanError {
                        path: PathBuf::from(e.path().display().to_string()),
                        message: e.error().to_string(),
                    });
                }
            }
        }

        docs.sort_by(|a, b| a.path.cmp(&b.path));

        DocTree { docs, errors, root: root.to_path_buf() }
    }

    /// Get all documents.
    pub fn all(&self) -> &[Document] {
        &self.docs
    }

    /// Get documents by category.
    pub fn by_category(&self, category: Category) -> Vec<&Document> {
        self.docs.iter().filter(|d| d.category == category).collect()
    }

    /// Get documents matching a tag.
    pub fn by_tag(&self, tag: &str) -> Vec<&Document> {
        self.docs.iter().filter(|d| {
            d.frontmatter.tags.as_deref().unwrap_or(&[])
                .iter()
                .any(|t| t.eq_ignore_ascii_case(tag))
        }).collect()
    }

    /// Search documents by title or body content (case-insensitive substring match).
    pub fn search(&self, query: &str) -> Vec<&Document> {
        let q = query.to_lowercase();
        self.docs.iter().filter(|d| {
            let title_match = d.frontmatter.title.as_deref()
                .map(|t| t.to_lowercase().contains(&q))
                .unwrap_or(false);
            let body_match = d.body.to_lowercase().contains(&q);
            title_match || body_match
        }).collect()
    }

    /// Get a document by its path (relative to root).
    pub fn get(&self, rel_path: &str) -> Option<&Document> {
        let target = self.root.join(rel_path);
        self.docs.iter().find(|d| d.path == target)
    }

    /// Count documents by category.
    pub fn counts(&self) -> HashMap<Category, usize> {
        let mut map = HashMap::new();
        for doc in &self.docs {
            *map.entry(doc.category).or_insert(0) += 1;
        }
        map
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures_root() -> PathBuf {
        // CARGO_MANIFEST_DIR points to the crate dir; fixtures are at workspace root.
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace = manifest.parent().unwrap().parent().unwrap();
        let root = workspace.join("tests/fixtures/docs");
        assert!(root.exists(), "fixtures dir not found at {}", root.display());
        root
    }

    #[test]
    fn scan_finds_all_markdown_files() {
        let tree = DocTree::scan(&fixtures_root());
        // 9 files with valid frontmatter + no_frontmatter.md (parsed with default fm)
        assert!(tree.docs.len() >= 9, "expected >= 9 docs, got {}", tree.docs.len());
    }

    #[test]
    fn scan_continues_on_errors() {
        let tree = DocTree::scan(&fixtures_root());
        // no_frontmatter.md parses successfully (with default RawFrontmatter),
        // so it shows up in docs not errors. Verify scan didn't abort.
        let total = tree.docs.len() + tree.errors.len();
        assert!(total >= 10, "expected >= 10 total entries, got {total}");
    }

    #[test]
    fn by_category_active() {
        let tree = DocTree::scan(&fixtures_root());
        let active = tree.by_category(Category::Active);
        assert!(active.len() >= 4, "expected >= 4 active docs, got {}", active.len());
        for doc in &active {
            assert_eq!(doc.category, Category::Active);
        }
    }

    #[test]
    fn by_category_design() {
        let tree = DocTree::scan(&fixtures_root());
        let design = tree.by_category(Category::Design);
        assert_eq!(design.len(), 2, "expected 2 design docs, got {}", design.len());
        for doc in &design {
            assert_eq!(doc.category, Category::Design);
        }
    }

    #[test]
    fn by_tag_architecture() {
        let tree = DocTree::scan(&fixtures_root());
        let arch = tree.by_tag("architecture");
        assert!(arch.len() >= 2, "expected >= 2 docs with 'architecture' tag, got {}", arch.len());
        for doc in &arch {
            let tags = doc.frontmatter.tags.as_deref().unwrap();
            assert!(tags.iter().any(|t| t == "architecture"));
        }
    }

    #[test]
    fn search_finds_execution_engine() {
        let tree = DocTree::scan(&fixtures_root());
        let results = tree.search("execution");
        assert!(!results.is_empty(), "search for 'execution' should find results");
        assert!(results.iter().any(|d| {
            d.frontmatter.title.as_deref()
                .map(|t| t.contains("Execution Engine"))
                .unwrap_or(false)
        }));
    }

    #[test]
    fn search_is_case_insensitive() {
        let tree = DocTree::scan(&fixtures_root());
        let lower = tree.search("core concepts");
        let upper = tree.search("CORE CONCEPTS");
        assert_eq!(lower.len(), upper.len());
        assert!(!lower.is_empty());
    }

    #[test]
    fn scan_filter_by_category() {
        let filter = ScanFilter {
            categories: Some(vec![Category::Research]),
            ..Default::default()
        };
        let tree = DocTree::scan_filtered(&fixtures_root(), &filter);
        assert_eq!(tree.docs.len(), 2, "expected 2 research docs, got {}", tree.docs.len());
        for doc in &tree.docs {
            assert_eq!(doc.category, Category::Research);
        }
    }

    #[test]
    fn counts_returns_correct_values() {
        let tree = DocTree::scan(&fixtures_root());
        let counts = tree.counts();
        assert!(counts.get(&Category::Active).copied().unwrap_or(0) >= 4);
        assert_eq!(counts.get(&Category::Design).copied().unwrap_or(0), 2);
        assert_eq!(counts.get(&Category::Research).copied().unwrap_or(0), 2);
        assert_eq!(counts.get(&Category::Archive).copied().unwrap_or(0), 1);
    }
}
