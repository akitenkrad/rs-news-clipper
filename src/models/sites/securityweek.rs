use crate::models::provenance::Provenance;
use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use scraper::Selector;

const URL: &str = "https://www.securityweek.com/feed/";

#[derive(Debug, Clone)]
pub struct SecurityWeek {
    site_name: String,
    url: Url,
}

impl SecurityWeek {
    pub fn new() -> Self {
        Self {
            site_name: "SecurityWeek".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn parse_feed(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss2::parse(raw).map_err(|error| AppError::ScrapeError(format!("Failed to parse SecurityWeek RSS: {error}")))?;
        feeds
            .into_iter()
            .map(|feed| {
                let published_at = feed
                    .publish_date
                    .ok_or_else(|| AppError::ScrapeError("Missing SecurityWeek pubDate".into()))?;
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
        let body_selector = Selector::parse("div.zox-post-body").unwrap();
        let paragraph_selector = Selector::parse("p").unwrap();
        let body = document
            .select(&body_selector)
            .next()
            .ok_or_else(|| AppError::ScrapeError("Failed to find SecurityWeek article body".into()))?;
        let mut raw_html = body.inner_html();
        for paragraph in body.select(&paragraph_selector) {
            let text = paragraph.text().collect::<String>();
            if text.trim_start().starts_with("Related:") {
                raw_html = raw_html.replace(&paragraph.html(), "");
            }
        }
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

impl Default for SecurityWeek {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for SecurityWeek {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }

    fn site_url(&self) -> Url {
        self.url.clone()
    }

    fn domain(&self) -> String {
        "www.securityweek.com".to_string()
    }

    fn provenance(&self) -> Provenance {
        Provenance::Reporting
    }

    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![".zox-post-ad-wrap", ".sw-newsletter", "[class*='popup']"]
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
    fn parses_securityweek_feed_fixture() {
        let site = SecurityWeek::default();
        let fixture = include_str!("../../../tests/fixtures/securityweek.xml");
        let articles = site.parse_feed(fixture).unwrap();

        assert_eq!(articles.len(), 10);
        assert!(articles.iter().all(|article| {
            article.article_url.starts_with("https://www.securityweek.com/") && !article.title.is_empty() && article.timestamp.timestamp() > 0
        }));
    }

    #[test]
    fn extracts_securityweek_body_without_related_or_newsletter_copy() {
        let site = SecurityWeek::default();
        let fixture = include_str!("../../../tests/fixtures/securityweek_article.html");
        let (_, text) = site.extract_article(fixture).unwrap();

        assert!(text.contains("Syrian Electronic Army"));
        assert!(!text.contains("Related:"));
        assert!(!text.contains("Daily Briefing Newsletter"));
        assert!(!text.contains("Advertisement. Scroll to continue reading"));
    }
}
