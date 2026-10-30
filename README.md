# news-clipper

A Rust library that collects articles from technology blogs and news sites.

## Features

- Collects articles from more than 50 sources
- Parses RSS feeds and web pages
- Analyzes article properties such as AI and security relevance

## Usage

```rust
use news_clipper::models::get_all_sites;

#[tokio::main]
async fn main() {
    let sites = get_all_sites().await.unwrap();
    
    for mut site in sites {
        let articles = site.get_articles().await.unwrap();
        for article in articles {
            println!("{}: {}", article.site.name, article.title);
        }
    }
}
```

## AIDB premium articles

AIDB premium article access is configured through macOS Keychain. Store the
browser-exported Cookie header and the account credentials in three separate
items. Each command prompts for the value so it does not appear in the shell
command line:

```shell
security add-generic-password -U -s news-clipper-aidb -a cookie -w
security add-generic-password -U -s news-clipper-aidb -a user -w
security add-generic-password -U -s news-clipper-aidb -a password -w
```

The initial `cookie` value should be the complete Cookie request-header value
exported from a logged-in browser. When it has less than 48 hours remaining,
news-clipper uses Chrome and the dedicated profile at
`~/Library/Application Support/news_clipper/chrome-profile-aidb` to refresh it.
Automatic login is limited to one attempt every 20 hours to avoid account
lockout.

Article source URLs remain in the `/archives/<id>` form provided by the RSS
feed. When reading an article, news-clipper supports both the timeline reader
and the legacy page. It follows redirects, extracts either `#xui-read-html` or
`.post_content`, and tries the other design once when the first page has no
extractable article body. The same two-design flow is repeated after an
automatic login refresh.

Run one login attempt manually with:

```shell
cargo run --example aidb_login --
```

Use `--force` to bypass the 20-hour limit. Set `AIDB_LOGIN_HEADFUL=1` to show
the Chrome window while diagnosing reCAPTCHA or login behavior. The
`AIDB_FORCE_LOGIN=1` environment variable also bypasses the attempt limit.
