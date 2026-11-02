use crate::models::provenance::Provenance;
use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use scraper::Selector;
use std::collections::HashMap;

const URL: &str = "https://www.nhk.or.jp/rss/news/cat0.xml";

#[derive(Debug, Clone)]
pub struct NhkNews {
    site_name: String,
    url: Url,
    descriptions: HashMap<String, String>,
}

impl NhkNews {
    pub fn new() -> Self {
        Self {
            site_name: "NHK ニュース".to_string(),
            url: Url::parse(URL).unwrap(),
            descriptions: HashMap::new(),
        }
    }

    fn parse_feed(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss2::parse(raw).map_err(|error| AppError::ScrapeError(format!("Failed to parse NHK RSS: {error}")))?;

        feeds
            .into_iter()
            .map(|feed| {
                let published_at = feed.publish_date.ok_or_else(|| AppError::ScrapeError("Missing NHK pubDate".into()))?;
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

    fn extract_article(&self, raw: &str, rss_description: Option<&str>) -> AppResult<(Html, Text)> {
        let document = scraper::Html::parse_document(raw);
        let selector = Selector::parse("main p").unwrap();
        if let Some(paragraph) = document.select(&selector).next() {
            let html = self.clean_content(&paragraph.html());
            let text = html2md::rewrite_html(&html, false);
            return Ok((self.trim_text(&html), self.trim_text(&text)));
        }

        let description = rss_description
            .filter(|description| !description.trim().is_empty())
            .ok_or_else(|| AppError::ScrapeError("NHK article had no main paragraph or RSS description".into()))?;
        let text = html2md::rewrite_html(description, false);
        Ok((description.to_string(), self.trim_text(&text)))
    }
}

impl Default for NhkNews {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for NhkNews {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }

    fn site_url(&self) -> Url {
        self.url.clone()
    }

    fn domain(&self) -> String {
        "news.web.nhk".to_string()
    }

    fn provenance(&self) -> Provenance {
        Provenance::Reporting
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        Ok(Cookie::default())
    }

    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let response = self.request(self.url.as_str(), "").await?;
        let articles = self.parse_feed(&response.text().await?)?;
        self.descriptions = articles
            .iter()
            .map(|article| (article.article_url.clone(), article.description.clone()))
            .collect();
        Ok(articles)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let response = self.request(url, "").await?;
        let raw = response.text().await?;
        self.extract_article(&raw, self.descriptions.get(url).map(String::as_str))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nhk_cat0_fixture() {
        let site = NhkNews::default();
        let fixture = include_str!("../../../tests/fixtures/nhk_cat0.xml");
        let articles = site.parse_feed(fixture).unwrap();

        assert_eq!(articles.len(), 7);
        assert!(articles.iter().all(|article| {
            article.article_url.starts_with("https://news.web.nhk/") && !article.title.is_empty() && article.timestamp.timestamp() > 0
        }));
        assert!(articles[1].description.contains("713万通"));
    }

    #[test]
    fn extracts_nhk_main_paragraph() {
        let site = NhkNews::default();
        let fixture = include_str!("../../../tests/fixtures/nhk_article.html");
        let (_, text) = site.extract_article(fixture, None).unwrap();

        assert!(text.contains("企業向けにメール配信サービス"));
        assert!(!text.contains("注目ワード"));
    }

    #[test]
    fn falls_back_to_rss_description_when_page_has_no_paragraph() {
        let site = NhkNews::default();
        let (_, text) = site.extract_article("<main><div>動画だけ</div></main>", Some("RSSの要約本文")).unwrap();

        assert_eq!(text, "RSSの要約本文");
    }
}
