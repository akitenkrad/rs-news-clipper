use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://therecord.media/feed";
const ARTICLE_SELECTOR: &str = "div.article-body, div.article__content, article";

#[derive(Debug, Clone)]
pub struct TheRecord {
    site_name: String,
    url: Url,
}

impl TheRecord {
    pub fn new() -> Self {
        Self {
            site_name: "The Record".to_string(),
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

impl Default for TheRecord {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for TheRecord {
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
<rss version="2.0"><channel><title>The Record</title>
<item><title>Valid article</title><link>https://therecord.media/example</link><description>Summary</description><pubDate>Thu, 29 Oct 2026 16:00:00 +0000</pubDate></item>
<item><title>Undated</title><link>https://therecord.media/undated</link><description>Skip</description></item>
<item><title>Invalid date</title><link>https://therecord.media/invalid-date</link><description>Skip</description><pubDate>invalid</pubDate></item>
</channel></rss>"#;

    #[test]
    fn parses_rss2_and_skips_items_without_valid_dates() {
        let site = TheRecord::default();
        let articles = site.parse_feed(RSS).unwrap();

        assert_eq!(articles.len(), 1);
        assert_eq!(articles[0].title, "Valid article");
        assert_eq!(articles[0].description, "Summary");
    }

    #[test]
    fn extracts_article_body() {
        let site = TheRecord::default();
        let page = r#"<html><body><nav>Navigation</nav><div class="article-body"><p>The Record article body.</p></div></body></html>"#;
        let (html, text) = site.extract_article(page).unwrap();

        assert!(html.contains("The Record article body"));
        assert!(text.contains("The Record article body"));
        assert!(!html.contains("Navigation"));
    }
}
