# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development Commands

```bash
cargo build                    # Debug build
cargo build --release          # Optimized build
cargo test                     # Run all tests
cargo test test_all_sites      # Run the main integration test
cargo test -- --nocapture      # Show stdout during tests
cargo fmt                      # Format code
cargo clippy                   # Lint
cargo doc --open               # Generate and view docs
```

## Architecture

**news-clipper** is a Rust library (edition 2024) that aggregates articles from 50+ tech blogs and news sites via RSS/Atom feed parsing and HTML scraping.

### Core Trait Pattern

All site implementations use the `WebSiteInterface` async trait (`src/models/web_article.rs`):

```rust
#[async_trait]
pub trait WebSiteInterface: Send + Sync {
    fn site_name(&self) -> String;
    fn site_url(&self) -> Url;
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>>;
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)>;
    async fn login(&mut self) -> AppResult<Cookie>;
    fn domain(&self) -> String;
}
```

### Main API

`src/models/mod.rs` exports `get_all_sites()` which returns `Vec<Box<dyn WebSiteInterface>>` containing all site implementations.

### Two Site Implementation Patterns

1. **Feed-based** (RSS/Atom): Use `feed_parser` crate to parse feeds, extract articles from entries. See `src/models/sites/ai_news.rs` for Atom, `src/models/sites/tech_crunch.rs` for RSS.

2. **HTML scraping**: Fetch page HTML, parse with `scraper` crate using CSS selectors. See `src/models/sites/medium.rs`.

### Module Layout

- `src/models/sites/` — Individual site implementations (58 modules)
- `src/models/web_article.rs` — `WebArticle`, `WebArticleProperty` structs, `WebSiteInterface` trait
- `src/models/web_site.rs` — `WebSite` struct
- `src/shared/errors.rs` — `AppError` enum, `AppResult<T>` type alias
- `src/shared/logger.rs` — Tracing-based logging setup
- `src/shared/id.rs` — UUID-based ID types via macro (`WebSiteId`, `WebArticleId`)
- `src/shared/utils.rs` — Progress bars, Levenshtein distance

### External Git Dependencies

- `keyword-tools` — github.com/akitenkrad/rs-keyword-tools
- `openai-tools` — github.com/akitenkrad/rs-openai-tools

## Writing Style

- Use full-width punctuation in Japanese text: ピリオド`．` (not `。`), カンマ`，` (not `、`)

## Adding a New Site

1. Create a new module in `src/models/sites/`
2. Implement `WebSiteInterface` for your struct
3. Add the module declaration in `src/models/sites/mod.rs`
4. Add an instance to the vector in `get_all_sites()` in `src/models/mod.rs`
