use crate::models::provenance::Provenance;
use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://www.jpcert.or.jp/rss/jpcert.rdf";

#[derive(Debug, Clone)]
pub struct JPCert {
    site_name: String,
    url: Url,
}

impl JPCert {
    pub fn new() -> Self {
        Self {
            site_name: "JPCERT".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn parse_feed(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss1::parse(raw)
            .map_err(|e| AppError::ScrapeError(format!("Failed to parse RSS feed: {e}")))?;

        feeds
            .into_iter()
            .map(|feed| {
                let date = feed
                    .date
                    .ok_or_else(|| AppError::ScrapeError("Missing date".into()))?;
                let timestamp = DateTime::parse_from_rfc3339(&date)
                    .or_else(|_| DateTime::parse_from_str(&date, "%Y-%m-%dT%H:%M%:z"))?;
                Ok(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title,
                    feed.link,
                    feed.description.unwrap_or_default(),
                    timestamp.into(),
                ))
            })
            .collect()
    }
}

impl Default for JPCert {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for JPCert {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.url.clone()
    }
    fn domain(&self) -> String {
        self.url.domain().unwrap().to_string()
    }

    fn provenance(&self) -> Provenance {
        Provenance::Origin
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
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = scraper::Selector::parse("article div.p-article__content").unwrap();
        let article = match document.select(&selector).next() {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(format!(
                    "Failed to find article content: {:?}",
                    selector
                )));
            }
        };
        let raw_html = article.html().to_string();
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_jpcert_rdf_fixture() {
        let site = JPCert::default();
        let fixture = include_str!("../../../tests/fixtures/jpcert.rdf");
        let articles = site.parse_feed(fixture).unwrap();

        assert!(!articles.is_empty());
        assert!(articles.iter().all(|article| !article.title.is_empty()));
        assert!(
            articles
                .iter()
                .all(|article| !article.article_url.is_empty())
        );
        assert_eq!(
            site.site_url().as_str(),
            "https://www.jpcert.or.jp/rss/jpcert.rdf"
        );
    }
}
