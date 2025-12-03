use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://ainow.ai/feed/";

#[derive(Debug, Clone)]
pub struct AIItNow {
    site_name: String,
    url: Url,
}

impl AIItNow {
    pub fn new() -> Self {
        Self {
            site_name: "AI IT Now".into(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for AIItNow {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for AIItNow {
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
        Ok(String::default())
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.url.as_str(), &cookies).await?;
        let feeds = if let Ok(r) = parsers::rss2::parse(response.text().await?.as_str()) {
            r
        } else {
            return Err(AppError::ScrapeError("Failed to parse RSS".into()));
        };
        let mut articles = Vec::new();
        for feed in feeds {
            articles.push(WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                feed.title,
                feed.link,
                feed.description.unwrap_or("".to_string()),
                DateTime::parse_from_rfc2822(&feed.publish_date.unwrap())
                    .unwrap()
                    .into(),
            ));
        }
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let cookies = self.login().await?;
        let response = self.request(url, &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector =
            scraper::Selector::parse("body div.contents div.article_area div.entry-content")
                .unwrap();
        match document.select(&selector).next() {
            Some(elem) => {
                let html = elem.html().to_string();
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            None => Err(AppError::ScrapeError("Failed to parse article text".into())),
        }
    }
}
