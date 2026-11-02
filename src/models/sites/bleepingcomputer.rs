use crate::models::provenance::Provenance;
use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use scraper::Selector;

const URL: &str = "https://www.bleepingcomputer.com/feed/";

#[derive(Debug, Clone)]
pub struct BleepingComputer {
    site_name: String,
    url: Url,
}

impl BleepingComputer {
    pub fn new() -> Self {
        Self {
            site_name: "BleepingComputer".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn parse_feed(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss2::parse(raw).map_err(|error| AppError::ScrapeError(format!("Failed to parse BleepingComputer RSS: {error}")))?;
        feeds
            .into_iter()
            .map(|feed| {
                let published_at = feed
                    .publish_date
                    .ok_or_else(|| AppError::ScrapeError("Missing BleepingComputer pubDate".into()))?;
                Ok(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title,
                    feed.link,
                    feed.description.unwrap_or_default(),
                    DateTime::parse_from_rfc2822(&published_at)?.into(),
                ))
            })
            .collect()
    }

    fn extract_article(&self, raw: &str) -> AppResult<(Html, Text)> {
        let document = scraper::Html::parse_document(raw);
        let selector = Selector::parse("div.articleBody").unwrap();
        let body = document
            .select(&selector)
            .next()
            .ok_or_else(|| AppError::ScrapeError("Failed to find BleepingComputer article body".into()))?;
        let html = self.clean_content(&body.inner_html());
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

impl Default for BleepingComputer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for BleepingComputer {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }

    fn site_url(&self) -> Url {
        self.url.clone()
    }

    fn domain(&self) -> String {
        "www.bleepingcomputer.com".to_string()
    }

    fn provenance(&self) -> Provenance {
        Provenance::Reporting
    }

    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![".cz-related-article-wrapp", ".article-callout", ".cz-newz-comment-wrapp"]
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        Ok(Cookie::default())
    }

    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let response = self.request(self.url.as_str(), "").await?;
        self.parse_feed(&response.text().await?)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let response = self.request(url, "").await?;
        self.extract_article(&response.text().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bleepingcomputer_feed_fixture() {
        let site = BleepingComputer::default();
        let fixture = include_str!("../../../tests/fixtures/bleepingcomputer.xml");
        let articles = site.parse_feed(fixture).unwrap();

        assert_eq!(articles.len(), 15);
        assert!(articles.iter().all(|article| {
            article.article_url.starts_with("https://www.bleepingcomputer.com/") && !article.title.is_empty() && article.timestamp.timestamp() > 0
        }));
    }

    #[test]
    fn extracts_bleepingcomputer_body_without_related_articles_or_comments() {
        let site = BleepingComputer::default();
        let fixture = include_str!("../../../tests/fixtures/bleepingcomputer_article.html");
        let (_, text) = site.extract_article(fixture).unwrap();

        assert!(text.contains("North Korean state-sponsored hackers"));
        assert!(!text.contains("Related Articles:"));
        assert!(!text.contains("Start your free email security scan"));
        assert!(!text.contains("Comments"));
    }
}
