# docman

A CLI tool for organizing, validating, and maintaining technical documentation with YAML frontmatter.

## Install from Source

Requires Rust 1.70+ ([install Rust](https://rustup.rs/)).

```bash
git clone https://github.com/strongdm/docman.git
cd docman
cargo install --path crates/dm-cli
```

This installs the `docman` binary to `~/.cargo/bin/`.

For development:

```bash
cargo build
# binary at ./target/debug/docman
```

## Install from GitHub

```bash
cargo install --git https://github.com/hanw/docman.git --bin docman
```

## Install from crates.io

```bash
cargo install docman
```

## Usage

```bash
# Search documents by keyword
docman search <query>

# Filter documents by tag
docman tag <tag>

# Show document metadata and counts
docman status [path]

# Run health checks (staleness, orphans, broken links)
docman check

# Generate INDEX.md, CHANGELOG.md, ROADMAP.md
docman index

# Create a new document from template
docman new

# Archive a document
docman archive
```

## Project Structure

```
crates/
├── dm-cli     # CLI entry point (binary)
├── dm-scan    # Filesystem scanner — builds a DocTree from markdown files
├── dm-meta    # YAML frontmatter parser, category inference, validation
├── dm-index   # Generates INDEX.md, CHANGELOG.md, ROADMAP.md
└── dm-checks  # Health checks: staleness, orphans, broken links, frontmatter
```

## Running Tests

```bash
cargo test
```
