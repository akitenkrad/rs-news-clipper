use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://stockmark.co.jp/feed/?post_type=news";

#[derive(Debug, Clone)]
pub struct StockmarkNews {
    site_name: String,
    url: Url,
}

impl StockmarkNews {
    pub fn new() -> Self {
        Self {
            site_name: "Stockmark News".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn parse_feed(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let feeds = parsers::rss2::parse(raw).map_err(|error| AppError::ScrapeError(format!("Failed to parse RSS: {error}")))?;
        feeds
            .into_iter()
            .map(|feed| {
                let publish_date = feed.publish_date.ok_or_else(|| AppError::ScrapeError("Missing publish_date".into()))?;
                Ok(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title,
                    feed.link,
                    feed.description.unwrap_or_default(),
                    DateTime::parse_from_rfc2822(&publish_date)?.into(),
                ))
            })
            .collect()
    }
}

impl Default for StockmarkNews {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for StockmarkNews {
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
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = scraper::Selector::parse("main div.l-body").unwrap();
        let article = match document.select(&selector).next() {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(format!("Failed to find article: {:?}", selector)));
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
    fn parses_stockmark_news_post_type_feed_fixture() {
        let site = StockmarkNews::default();
        let fixture = include_str!("../../../tests/fixtures/stockmark_post_type_news.xml");
        let articles = site.parse_feed(fixture).unwrap();

        assert_eq!(articles.len(), 10);
        assert!(articles.iter().all(|article| {
            article.article_url.starts_with("https://stockmark.co.jp/news/") && !article.title.is_empty() && article.timestamp.timestamp() > 0
        }));
        assert_eq!(site.site_url().as_str(), "https://stockmark.co.jp/feed/?post_type=news");
    }
}
