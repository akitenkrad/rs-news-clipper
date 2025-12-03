use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use crate::shared::errors::{AppError, AppResult};

const URL: &str = "https://tech.gunosy.io/feed";

#[derive(Debug, Clone)]
pub struct GunosyTechBlog {
    site_name: String,
    url: Url,
}

impl GunosyTechBlog {
    pub fn new() -> Self {
        let url = Url::parse(URL).unwrap();
        Self {
            site_name: "Gunosy Tech Blog".to_string(),
            url,
        }
    }
}

impl Default for GunosyTechBlog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for GunosyTechBlog {

    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.url.clone()
    }
    fn domain(&self) -> String {
        "tech.gunosy.io".to_string()
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        Ok(Cookie::new())
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.url.as_str(), &cookies).await?;
        let feeds = match parsers::atom::parse(response.text().await?.as_str()) {
            Ok(feeds) => feeds,
            Err(e) => {
                return Err(AppError::ScrapeError(format!("Failed to parse RSS: {}", e)));
            }
        };
        let articles = feeds
            .iter()
            .map(|feed| {
                WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title.clone(),
                    feed.link.clone(),
                    feed.description.clone().unwrap_or("".to_string()),
                    DateTime::parse_from_rfc3339(&feed.updated.clone().unwrap())
                        .unwrap()
                        .into(),
                )
            })
            .collect::<Vec<WebArticle>>();
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = scraper::Selector::parse("#main article div.entry-content").unwrap();
        let article = match document.select(&selector).next() {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(
                    "Failed to find article content: #main article div.entry-content".to_string(),
                ));
            }
        };
        let html = article.html().to_string();
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
