use std::path::PathBuf;
use std::process;

use chrono::Local;
use clap::{Parser, Subcommand};

/// docman — document management CLI
#[derive(Parser)]
#[command(name = "docman", version, about = "Document management CLI tool")]
struct Cli {
    /// Root directory for documentation
    #[arg(long, default_value = "docs", global = true)]
    docs_root: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Search documents by keyword
    Search {
        /// Search query
        query: String,
    },
    /// Filter documents by tag
    Tag {
        /// Tag to filter by
        tag: String,
    },
    /// Show document metadata (provide a path relative to docs root)
    Status {
        /// Relative path to the document (e.g. active/architecture/EXECUTION_ENGINE.md)
        path: Option<String>,
    },
    /// Run health checks (staleness, orphans, broken links)
    Check,
    /// Generate INDEX.md, CHANGELOG.md, ROADMAP.md
    Index {
        /// Output directory for generated files
        #[arg(short, long, default_value = ".")]
        output: PathBuf,
        /// Number of days for changelog
        #[arg(long, default_value_t = 30)]
        days: u32,
    },
    /// Create a new document from template
    New {
        /// Document category: active, design, or research
        category: String,
        /// Document title
        #[arg(long)]
        title: String,
        /// Author name
        #[arg(long)]
        author: String,
    },
    /// Archive a document (move to archive directory)
    Archive {
        /// Relative path to the document to archive
        path: String,
        /// Reason for archiving
        #[arg(long)]
        reason: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { category, title, author } => {
            cmd_new(&cli.docs_root, &category, &title, &author);
        }
        Commands::Archive { path, reason } => {
            cmd_archive(&cli.docs_root, &path, reason.as_deref());
        }
        _ => {
            let tree = dm_scan::DocTree::scan(&cli.docs_root);
            match cli.command {
                Commands::Search { query } => cmd_search(&tree, &query),
                Commands::Tag { tag } => cmd_tag(&tree, &tag),
                Commands::Status { path } => cmd_status(&tree, path.as_deref()),
                Commands::Check => cmd_check(&tree),
                Commands::Index { output, days } => cmd_index(&tree, &output, days),
                Commands::New { .. } | Commands::Archive { .. } => unreachable!(),
            }
        }
    }
}

fn cmd_search(tree: &dm_scan::DocTree, query: &str) {
    let results = tree.search(query);
    if results.is_empty() {
        println!("No documents found matching '{query}'.");
    } else {
        println!("Found {} document(s):", results.len());
        for doc in results {
            let title = doc.frontmatter.title.as_deref().unwrap_or("(untitled)");
            println!("  [{}] {} — {}", doc.category, title, doc.path.display());
        }
    }
}

fn cmd_tag(tree: &dm_scan::DocTree, tag: &str) {
    let results = tree.by_tag(tag);
    if results.is_empty() {
        println!("No documents found with tag '{tag}'.");
    } else {
        println!("Found {} document(s) with tag '{tag}':", results.len());
        for doc in results {
            let title = doc.frontmatter.title.as_deref().unwrap_or("(untitled)");
            println!("  [{}] {} — {}", doc.category, title, doc.path.display());
        }
    }
}

fn cmd_status(tree: &dm_scan::DocTree, path: Option<&str>) {
    match path {
        Some(rel_path) => {
            match tree.get(rel_path) {
                Some(doc) => {
                    let fm = &doc.frontmatter;
                    println!("title: {}", fm.title.as_deref().unwrap_or("(untitled)"));
                    println!("category: {}", doc.category);
                    println!("status: {}", dm_meta::resolve_status(fm, doc.category));
                    if let Some(v) = fm.version {
                        println!("version: {v}");
                    }
                    if let Some(ref a) = fm.author {
                        println!("author: {a}");
                    }
                    if let Some(ref o) = fm.owner {
                        println!("owner: {o}");
                    }
                    if let Some(d) = fm.created {
                        println!("created: {d}");
                    }
                    if let Some(d) = fm.last_updated {
                        println!("last_updated: {d}");
                    }
                    if let Some(d) = fm.next_review {
                        println!("next_review: {d}");
                    }
                    if let Some(ref tags) = fm.tags {
                        println!("tags: {}", tags.join(", "));
                    }
                    if let Some(ref reviewers) = fm.reviewers {
                        println!("reviewers: {}", reviewers.join(", "));
                    }
                    if let Some(ref related) = fm.related_docs {
                        if !related.is_empty() {
                            println!("related_docs: {}", related.join(", "));
                        }
                    }
                    if let Some(id) = fm.doc_id {
                        println!("doc_id: {id}");
                    }
                    if let Some(d) = fm.decision_date {
                        println!("decision_date: {d}");
                    }
                    if let Some(pr) = fm.implementation_pr {
                        println!("implementation_pr: {pr}");
                    }
                }
                None => {
                    eprintln!("Document not found: {rel_path}");
                    process::exit(1);
                }
            }
        }
        None => {
            let counts = tree.counts();
            println!("Document Status");
            println!("===============");
            println!("Total: {} documents ({} errors during scan)", tree.docs.len(), tree.errors.len());
            for (cat, count) in &counts {
                println!("  {cat}: {count}");
            }
        }
    }
}

fn cmd_check(tree: &dm_scan::DocTree) {
    let report = dm_checks::run_all_checks(tree);
    print!("{}", dm_checks::format_report(&report));
    if report.has_errors() {
        process::exit(1);
    }
}

fn cmd_index(tree: &dm_scan::DocTree, output: &std::path::Path, days: u32) {
    if let Err(e) = dm_index::write_all(tree, output, days) {
        eprintln!("Error writing index files: {e}");
        process::exit(1);
    }
    println!("Generated INDEX.md, CHANGELOG.md, ROADMAP.md in {}", output.display());
}

fn cmd_new(docs_root: &std::path::Path, category: &str, title: &str, author: &str) {
    let cat = category.to_lowercase();
    let today = Local::now().date_naive();
    let year = today.format("%Y").to_string();

    // Build slug from title
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<&str>>()
        .join("-");

    let (dir, filename, frontmatter) = match cat.as_str() {
        "design" => {
            // Find next doc_id by scanning existing design docs
            let next_id = find_next_design_id(docs_root);
            let dir = docs_root.join(format!("design/{year}/proposed"));
            let filename = format!("{next_id:03}-{slug}.md");
            let fm = format!(
                "---\ndoc_id: {next_id}\ntitle: \"{title}\"\nstatus: proposed\ncreated: {today}\nauthor: {author}\ntags: []\n---\n\n# {title}\n\nTODO: Write content here.\n"
            );
            (dir, filename, fm)
        }
        "research" => {
            let dir = docs_root.join(format!("research/{year}"));
            let filename = format!("{slug}.md");
            let fm = format!(
                "---\ntitle: \"{title}\"\nstatus: draft\ncreated: {today}\nauthor: {author}\ntype: research\nmay_become_design_doc: false\ntags: []\n---\n\n# {title}\n\nTODO: Write content here.\n"
            );
            (dir, filename, fm)
        }
        _ => {
            let dir = docs_root.join("active");
            let filename = format!("{}.md", slug.to_uppercase());
            let fm = format!(
                "---\ntitle: \"{title}\"\nversion: 1.0\nstatus: active\ncreated: {today}\nlast_updated: {today}\nauthor: {author}\ntags: []\n---\n\n# {title}\n\nTODO: Write content here.\n"
            );
            (dir, filename, fm)
        }
    };

    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("Error creating directory: {e}");
        process::exit(1);
    }

    let file_path = dir.join(&filename);
    if let Err(e) = std::fs::write(&file_path, frontmatter) {
        eprintln!("Error writing file: {e}");
        process::exit(1);
    }

    // Print the relative path from docs_root
    let rel = file_path.strip_prefix(docs_root).unwrap_or(&file_path);
    println!("Created: {}", rel.display());
}

fn find_next_design_id(docs_root: &std::path::Path) -> u32 {
    let pattern = format!("{}/**/design/**/*.md", docs_root.display());
    let mut max_id: u32 = 0;
    if let Ok(paths) = glob::glob(&pattern) {
        for entry in paths.flatten() {
            if let Ok(doc) = dm_meta::parse_document(&entry) {
                if let Some(id) = doc.frontmatter.doc_id {
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }
    }
    // Also scan design/ directly under docs_root
    let pattern2 = format!("{}/design/**/*.md", docs_root.display());
    if let Ok(paths) = glob::glob(&pattern2) {
        for entry in paths.flatten() {
            if let Ok(doc) = dm_meta::parse_document(&entry) {
                if let Some(id) = doc.frontmatter.doc_id {
                    if id > max_id {
                        max_id = id;
                    }
                }
            }
        }
    }
    max_id + 1
}

fn cmd_archive(docs_root: &std::path::Path, rel_path: &str, reason: Option<&str>) {
    let source = docs_root.join(rel_path);
    if !source.exists() {
        eprintln!("File not found: {}", source.display());
        process::exit(1);
    }

    let today = Local::now().date_naive();
    let year = today.format("%Y").to_string();

    // Read and parse the file
    let content = match std::fs::read_to_string(&source) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error reading file: {e}");
            process::exit(1);
        }
    };

    // Extract filename
    let filename = source.file_name().unwrap().to_string_lossy().to_string();

    // Build new content with updated frontmatter
    let new_content = match dm_meta::extract_frontmatter(&content) {
        Some((yaml, body)) => {
            let mut fm: dm_meta::RawFrontmatter = match serde_yaml::from_str(yaml) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Error parsing frontmatter: {e}");
                    process::exit(1);
                }
            };
            fm.archived_date = Some(today);
            if let Some(r) = reason {
                fm.archived_reason = Some(r.to_string());
            }
            fm.status = Some("archived".to_string());
            let new_yaml = serde_yaml::to_string(&fm).unwrap();
            format!("---\n{new_yaml}---\n{body}")
        }
        None => {
            // No frontmatter — add minimal
            let reason_line = reason.map(|r| format!("archived_reason: \"{r}\"\n")).unwrap_or_default();
            format!("---\nstatus: archived\narchived_date: {today}\n{reason_line}---\n{content}")
        }
    };

    // Create destination directory
    let dest_dir = docs_root.join(format!("archive/{year}"));
    if let Err(e) = std::fs::create_dir_all(&dest_dir) {
        eprintln!("Error creating archive directory: {e}");
        process::exit(1);
    }

    let dest = dest_dir.join(&filename);

    // Write new content
    if let Err(e) = std::fs::write(&dest, new_content) {
        eprintln!("Error writing archive file: {e}");
        process::exit(1);
    }

    // Remove original
    if let Err(e) = std::fs::remove_file(&source) {
        eprintln!("Error removing original file: {e}");
        process::exit(1);
    }

    let dest_rel = dest.strip_prefix(docs_root).unwrap_or(&dest);
    println!("Archived: {} -> {}", rel_path, dest_rel.display());
}
