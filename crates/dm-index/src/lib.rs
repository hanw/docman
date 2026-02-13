use std::collections::BTreeMap;
use std::path::Path;

use chrono::NaiveDate;
use dm_meta::{Category, Document};
use dm_scan::DocTree;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Compute a relative path from the doc root for display in generated markdown.
fn rel_path(doc: &Document, root: &Path) -> String {
    doc.path
        .strip_prefix(root)
        .unwrap_or(&doc.path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn title_or_filename(doc: &Document) -> String {
    doc.frontmatter
        .title
        .clone()
        .unwrap_or_else(|| {
            doc.path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".into())
        })
}

/// Extract the first path component after the category directory.
/// e.g. `active/architecture/FOO.md` -> `architecture`
fn subgroup(doc: &Document, root: &Path) -> String {
    let rp = rel_path(doc, root);
    let parts: Vec<&str> = rp.split('/').collect();
    // parts[0] = category dir (active/design/...), parts[1] = subgroup
    if parts.len() >= 3 {
        parts[1].to_string()
    } else {
        "other".to_string()
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
    }
}

// ---------------------------------------------------------------------------
// INDEX.md
// ---------------------------------------------------------------------------

/// Generate an INDEX.md table of contents grouped by category.
pub fn generate_index(tree: &DocTree) -> String {
    generate_index_with_date(tree, today())
}

fn generate_index_with_date(tree: &DocTree, date: NaiveDate) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Documentation Index\n\n*Auto-generated: {date}*\n"));

    // Active docs grouped by subdirectory
    let active = tree.by_category(Category::Active);
    if !active.is_empty() {
        out.push_str("\n## Active Documentation\n");
        let mut groups: BTreeMap<String, Vec<&Document>> = BTreeMap::new();
        for doc in &active {
            let sg = subgroup(doc, &tree.root);
            groups.entry(sg).or_default().push(doc);
        }
        for (group, mut docs) in groups {
            out.push_str(&format!("\n### {}\n\n", capitalize(&group)));
            docs.sort_by_key(|d| title_or_filename(d).to_lowercase());
            for doc in docs {
                let title = title_or_filename(doc);
                let rp = rel_path(doc, &tree.root);
                let updated = doc.frontmatter.last_updated
                    .map(|d| format!(" *(updated {d})*"))
                    .unwrap_or_default();
                out.push_str(&format!("- [{title}]({rp}){updated}\n"));
            }
        }
    }

    // Design docs grouped by status
    let design = tree.by_category(Category::Design);
    if !design.is_empty() {
        out.push_str("\n## Design Documents\n");
        let mut groups: BTreeMap<String, Vec<&Document>> = BTreeMap::new();
        for doc in &design {
            let status = doc.frontmatter.status.as_deref().unwrap_or("proposed").to_lowercase();
            groups.entry(status).or_default().push(doc);
        }
        for (status, mut docs) in groups {
            out.push_str(&format!("\n### {}\n\n", capitalize(&status)));
            docs.sort_by_key(|d| d.frontmatter.doc_id.unwrap_or(u32::MAX));
            for doc in docs {
                let title = title_or_filename(doc);
                let rp = rel_path(doc, &tree.root);
                let prefix = doc.frontmatter.doc_id
                    .map(|id| format!("{id:03}: "))
                    .unwrap_or_default();
                let meta = match status.as_str() {
                    "accepted" => doc.frontmatter.decision_date
                        .map(|d| format!(" *accepted {d}*"))
                        .unwrap_or_default(),
                    _ => {
                        let author = doc.frontmatter.author.as_deref();
                        let created = doc.frontmatter.created;
                        match (author, created) {
                            (Some(a), Some(d)) => format!(" *by {a}, {d}*"),
                            (Some(a), None) => format!(" *by {a}*"),
                            (None, Some(d)) => format!(" *{d}*"),
                            (None, None) => String::new(),
                        }
                    }
                };
                out.push_str(&format!("- [{prefix}{title}]({rp}){meta}\n"));
            }
        }
    }

    // Research docs
    let research = tree.by_category(Category::Research);
    if !research.is_empty() {
        out.push_str("\n## Research\n\n");
        let mut docs = research.to_vec();
        docs.sort_by_key(|d| title_or_filename(d).to_lowercase());
        for doc in docs {
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            let status = doc.frontmatter.status.as_deref().unwrap_or("draft");
            out.push_str(&format!("- [{title}]({rp}) *({status})*\n"));
        }
    }

    // Archive docs
    let archive = tree.by_category(Category::Archive);
    if !archive.is_empty() {
        out.push_str("\n## Archive\n\n");
        let mut docs = archive.to_vec();
        docs.sort_by_key(|d| title_or_filename(d).to_lowercase());
        for doc in docs {
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            let reason = doc.frontmatter.archived_reason.as_ref()
                .map(|r| format!(" *{r}*"))
                .unwrap_or_default();
            out.push_str(&format!("- [{title}]({rp}){reason}\n"));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// CHANGELOG.md
// ---------------------------------------------------------------------------

/// Generate a CHANGELOG.md listing recently updated, created, and archived documents.
pub fn generate_changelog(tree: &DocTree, since_days: u32) -> String {
    generate_changelog_with_date(tree, since_days, today())
}

fn generate_changelog_with_date(tree: &DocTree, since_days: u32, date: NaiveDate) -> String {
    let cutoff = date - chrono::Days::new(since_days as u64);
    let mut out = String::new();
    out.push_str(&format!(
        "# Documentation Changelog\n\n*Auto-generated: {date}*\n*Showing changes from the last {since_days} days.*\n"
    ));

    // Recently Updated
    out.push_str("\n## Recently Updated\n\n");
    let mut updated: Vec<&Document> = tree.all().iter()
        .filter(|d| d.frontmatter.last_updated.map(|u| u >= cutoff).unwrap_or(false))
        .collect();
    updated.sort_by(|a, b| b.frontmatter.last_updated.cmp(&a.frontmatter.last_updated));
    if updated.is_empty() {
        out.push_str("- No changes.\n");
    } else {
        for doc in updated {
            let date_str = doc.frontmatter.last_updated.unwrap();
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            let version_info = doc.frontmatter.version
                .map(|v| format!(" — updated to v{v}"))
                .unwrap_or_default();
            out.push_str(&format!("- **{date_str}** [{title}]({rp}){version_info}\n"));
        }
    }

    // Recently Created
    out.push_str("\n## Recently Created\n\n");
    let mut created: Vec<&Document> = tree.all().iter()
        .filter(|d| d.frontmatter.created.map(|c| c >= cutoff).unwrap_or(false))
        .collect();
    created.sort_by(|a, b| b.frontmatter.created.cmp(&a.frontmatter.created));
    if created.is_empty() {
        out.push_str("- No changes.\n");
    } else {
        for doc in created {
            let date_str = doc.frontmatter.created.unwrap();
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            out.push_str(&format!("- **{date_str}** [{title}]({rp})\n"));
        }
    }

    // Recently Archived
    out.push_str("\n## Recently Archived\n\n");
    let archive = tree.by_category(Category::Archive);
    let mut archived: Vec<&&Document> = archive.iter()
        .filter(|d| d.frontmatter.archived_date.map(|a| a >= cutoff).unwrap_or(false))
        .collect();
    archived.sort_by(|a, b| b.frontmatter.archived_date.cmp(&a.frontmatter.archived_date));
    if archived.is_empty() {
        out.push_str("- No changes.\n");
    } else {
        for doc in archived {
            let date_str = doc.frontmatter.archived_date.unwrap();
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            let reason = doc.frontmatter.archived_reason.as_ref()
                .map(|r| format!(" — {r}"))
                .unwrap_or_default();
            out.push_str(&format!("- **{date_str}** [{title}]({rp}){reason}\n"));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// ROADMAP.md
// ---------------------------------------------------------------------------

/// Generate a ROADMAP.md from proposed/accepted design docs and promising research.
pub fn generate_roadmap(tree: &DocTree) -> String {
    generate_roadmap_with_date(tree, today())
}

fn generate_roadmap_with_date(tree: &DocTree, date: NaiveDate) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Documentation Roadmap\n\n*Auto-generated: {date}*\n"));

    // Under Review (Proposed)
    out.push_str("\n## Under Review (Proposed)\n\n");
    let design = tree.by_category(Category::Design);
    let mut proposed: Vec<&&Document> = design.iter()
        .filter(|d| d.frontmatter.status.as_deref().map(|s| s.eq_ignore_ascii_case("proposed")).unwrap_or(false))
        .collect();
    proposed.sort_by_key(|d| d.frontmatter.doc_id.unwrap_or(u32::MAX));
    if proposed.is_empty() {
        out.push_str("- None.\n");
    } else {
        for doc in proposed {
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            let prefix = doc.frontmatter.doc_id
                .map(|id| format!("{id:03}: "))
                .unwrap_or_default();
            let author = doc.frontmatter.author.as_ref()
                .map(|a| format!(" *by {a}*"))
                .unwrap_or_default();
            out.push_str(&format!("- [{prefix}{title}]({rp}){author}\n"));
        }
    }

    // Ready for Implementation (Accepted)
    out.push_str("\n## Ready for Implementation (Accepted)\n\n");
    let mut accepted: Vec<&&Document> = design.iter()
        .filter(|d| d.frontmatter.status.as_deref().map(|s| s.eq_ignore_ascii_case("accepted")).unwrap_or(false))
        .collect();
    accepted.sort_by_key(|d| d.frontmatter.doc_id.unwrap_or(u32::MAX));
    if accepted.is_empty() {
        out.push_str("- None.\n");
    } else {
        for doc in accepted {
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            let prefix = doc.frontmatter.doc_id
                .map(|id| format!("{id:03}: "))
                .unwrap_or_default();
            let decision = doc.frontmatter.decision_date
                .map(|d| format!(" *accepted {d}*"))
                .unwrap_or_default();
            out.push_str(&format!("- [{prefix}{title}]({rp}){decision}\n"));
        }
    }

    // Potential Future Work (Research)
    out.push_str("\n## Potential Future Work (Research)\n\n");
    let research = tree.by_category(Category::Research);
    let mut future: Vec<&&Document> = research.iter()
        .filter(|d| d.frontmatter.may_become_design_doc == Some(true))
        .collect();
    future.sort_by_key(|d| title_or_filename(d).to_lowercase());
    if future.is_empty() {
        out.push_str("- None.\n");
    } else {
        for doc in future {
            let title = title_or_filename(doc);
            let rp = rel_path(doc, &tree.root);
            out.push_str(&format!("- [{title}]({rp}) *(may become design doc)*\n"));
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Write all
// ---------------------------------------------------------------------------

/// Generate and write INDEX.md, CHANGELOG.md, and ROADMAP.md to the output directory.
pub fn write_all(tree: &DocTree, output_dir: &Path, changelog_days: u32) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(output_dir)?;
    std::fs::write(output_dir.join("INDEX.md"), generate_index(tree))?;
    std::fs::write(output_dir.join("CHANGELOG.md"), generate_changelog(tree, changelog_days))?;
    std::fs::write(output_dir.join("ROADMAP.md"), generate_roadmap(tree))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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
    fn index_groups_active_by_subdirectory() {
        let tree = scan_fixtures();
        let idx = generate_index(&tree);
        assert!(idx.contains("## Active Documentation"));
        assert!(idx.contains("### Architecture"));
        assert!(idx.contains("### Api") || idx.contains("### api"));
        assert!(idx.contains("### Guides") || idx.contains("### guides"));
        assert!(idx.contains("Core Concepts"));
        assert!(idx.contains("Execution Engine"));
    }

    #[test]
    fn index_lists_design_by_status() {
        let tree = scan_fixtures();
        let idx = generate_index(&tree);
        assert!(idx.contains("## Design Documents"));
        assert!(idx.contains("### Proposed"));
        assert!(idx.contains("### Accepted"));
        assert!(idx.contains("Recursive Self-Optimization"));
        assert!(idx.contains("Context Fidelity"));
    }

    #[test]
    fn index_includes_research_and_archive() {
        let tree = scan_fixtures();
        let idx = generate_index(&tree);
        assert!(idx.contains("## Research"));
        assert!(idx.contains("AI Optimization Techniques Survey"));
        assert!(idx.contains("*(draft)*"));
        assert!(idx.contains("## Archive"));
        assert!(idx.contains("Original Execution Engine Design"));
    }

    #[test]
    fn changelog_shows_recently_updated() {
        let tree = scan_fixtures();
        // Use a date just after the most recent update so all fixture docs are "recent"
        let date = NaiveDate::from_ymd_opt(2026, 2, 15).unwrap();
        let cl = generate_changelog_with_date(&tree, 365, date);
        assert!(cl.contains("## Recently Updated"));
        assert!(cl.contains("Execution Engine"));
    }

    #[test]
    fn changelog_shows_no_changes_when_too_old() {
        let tree = scan_fixtures();
        // Use a date far in the future with a tiny window — nothing should match
        let date = NaiveDate::from_ymd_opt(2030, 1, 1).unwrap();
        let cl = generate_changelog_with_date(&tree, 1, date);
        assert!(cl.contains("- No changes."));
    }

    #[test]
    fn roadmap_includes_proposed() {
        let tree = scan_fixtures();
        let rm = generate_roadmap(&tree);
        assert!(rm.contains("## Under Review (Proposed)"));
        assert!(rm.contains("Recursive Self-Optimization"));
    }

    #[test]
    fn roadmap_includes_accepted() {
        let tree = scan_fixtures();
        let rm = generate_roadmap(&tree);
        assert!(rm.contains("## Ready for Implementation (Accepted)"));
        assert!(rm.contains("Context Fidelity"));
        assert!(rm.contains("accepted 2026-01-25"));
    }

    #[test]
    fn roadmap_includes_research_may_become_design() {
        let tree = scan_fixtures();
        let rm = generate_roadmap(&tree);
        assert!(rm.contains("## Potential Future Work (Research)"));
        assert!(rm.contains("AI Optimization Techniques Survey"));
        assert!(rm.contains("may become design doc"));
    }

    #[test]
    fn roadmap_excludes_research_not_becoming_design() {
        let tree = scan_fixtures();
        let rm = generate_roadmap(&tree);
        // Competitor Analysis has may_become_design_doc=false
        let future_section_start = rm.find("## Potential Future Work").unwrap();
        let future_section = &rm[future_section_start..];
        assert!(!future_section.contains("Competitor Analysis"));
    }

    #[test]
    fn write_all_creates_files() {
        let tree = scan_fixtures();
        let dir = tempfile::tempdir().unwrap();
        write_all(&tree, dir.path(), 30).unwrap();
        assert!(dir.path().join("INDEX.md").exists());
        assert!(dir.path().join("CHANGELOG.md").exists());
        assert!(dir.path().join("ROADMAP.md").exists());
    }
}
