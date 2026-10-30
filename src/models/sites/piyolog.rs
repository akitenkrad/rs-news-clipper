use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://piyolog.hatenadiary.jp/rss";
const ARTICLE_SELECTOR: &str = "div.entry-content";

#[derive(Debug, Clone)]
pub struct Piyolog {
    site_name: String,
    url: Url,
}

impl Piyolog {
    pub fn new() -> Self {
        Self {
            site_name: "piyolog".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn parse_feed(&self, body: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss2::parse(body).map_err(|e| AppError::ScrapeError(format!("Failed to parse RSS: {e}")))?;

        Ok(feeds
            .into_iter()
            .filter_map(|feed| {
                let publish_date = feed.publish_date?;
                let timestamp = DateTime::parse_from_rfc2822(&publish_date).ok()?;
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

impl Default for Piyolog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Piyolog {
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

    const RSS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0"><channel><title>piyolog</title>
<item><title>有効な記事</title><link>https://piyolog.hatenadiary.jp/entry/2026/10/30/120000</link><description>説明</description><pubDate>Fri, 30 Oct 2026 12:00:00 +0900</pubDate></item>
<item><title>日付なし</title><link>https://piyolog.hatenadiary.jp/entry/undated</link><description>飛ばす</description></item>
<item><title>不正な日付</title><link>https://piyolog.hatenadiary.jp/entry/invalid-date</link><description>飛ばす</description><pubDate>invalid</pubDate></item>
</channel></rss>"#;

    #[test]
    fn parses_rss2_and_skips_items_without_valid_dates() {
        let site = Piyolog::default();
        let articles = site.parse_feed(RSS).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "有効な記事");
        assert_eq!(articles[0].description, "説明");
    }

    #[test]
    fn extracts_entry_content() {
        let site = Piyolog::default();
        let page = r#"<html><body><nav>ナビ</nav><div class="entry-content"><h2>見出し</h2><p>piyolog の記事本文です．十分な内容を返します．</p></div></body></html>"#;
        let (html, text) = site.extract_article(page).unwrap();

        assert!(html.contains("piyolog の記事本文"));
        assert!(text.contains("piyolog の記事本文"));
        assert!(!html.contains("ナビ"));
    }
}
