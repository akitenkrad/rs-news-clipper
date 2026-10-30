use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://scan.netsecurity.ne.jp/rss/index.rdf";
const ARTICLE_SELECTOR: &str = "div.article-body, article";

#[derive(Debug, Clone)]
pub struct ScanNetSecurity {
    site_name: String,
    url: Url,
}

impl ScanNetSecurity {
    pub fn new() -> Self {
        Self {
            site_name: "ScanNetSecurity".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn parse_feed(&self, body: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss1::parse(body).map_err(|e| AppError::ScrapeError(format!("Failed to parse RSS feed: {e}")))?;

        Ok(feeds
            .into_iter()
            .filter_map(|feed| {
                let date = feed.date?;
                let timestamp = DateTime::parse_from_rfc3339(&date).ok()?;
                Some(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title,
                    feed.link,
                    feed.description.unwrap_or_default(),
                    timestamp.into(),
                ))
            })
            .collect())
    }

    fn extract_article(&self, body: &str) -> AppResult<(Html, Text)> {
        let html = self
            .extract_with_fallback(body, ARTICLE_SELECTOR)
            .ok_or_else(|| AppError::ScrapeError(format!("Failed to parse article with selector: {ARTICLE_SELECTOR}")))?;
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

impl Default for ScanNetSecurity {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for ScanNetSecurity {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }

    fn site_url(&self) -> Url {
        self.url.clone()
    }

    fn domain(&self) -> String {
        self.url.domain().unwrap().to_string()
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        Ok(Cookie::default())
    }

    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.url.as_str(), &cookies).await?;
        self.parse_feed(&response.text().await?)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let cookies = self.login().await?;
        let response = self.request(url, &cookies).await?;
        self.extract_article(&response.text().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RDF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:dc="http://purl.org/dc/elements/1.1/">
<item rdf:about="https://scan.netsecurity.ne.jp/article/2026/10/30/1.html"><title>有効な記事</title><link>https://scan.netsecurity.ne.jp/article/2026/10/30/1.html</link><description>説明</description><dc:date>2026-10-30T08:05:00+09:00</dc:date></item>
<item rdf:about="https://scan.netsecurity.ne.jp/article/undated.html"><title>日付なし</title><link>https://scan.netsecurity.ne.jp/article/undated.html</link><description>飛ばす</description></item>
<item rdf:about="https://scan.netsecurity.ne.jp/article/invalid-date.html"><title>不正な日付</title><link>https://scan.netsecurity.ne.jp/article/invalid-date.html</link><description>飛ばす</description><dc:date>invalid</dc:date></item>
</rdf:RDF>"#;

    #[test]
    fn parses_rss1_dc_date_and_skips_items_without_valid_dates() {
        let site = ScanNetSecurity::default();
        let articles = site.parse_feed(RDF).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "有効な記事");
        assert_eq!(articles[0].description, "説明");
    }

    #[test]
    fn extracts_article_body() {
        let site = ScanNetSecurity::default();
        let page = r#"<html><body><nav>ナビ</nav><div class="article-body"><p>ScanNetSecurity の記事本文です．</p></div></body></html>"#;
        let (html, text) = site.extract_article(page).unwrap();

        assert!(html.contains("ScanNetSecurity の記事本文"));
        assert!(text.contains("ScanNetSecurity の記事本文"));
        assert!(!html.contains("ナビ"));
    }
}
