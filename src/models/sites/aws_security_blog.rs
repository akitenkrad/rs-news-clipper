use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://aws.amazon.com/jp/blogs/security/feed/";

#[derive(Debug, Clone)]
pub struct AWSSecurityBlog {
    site_name: String,
    url: Url,
}

impl AWSSecurityBlog {
    pub fn new() -> Self {
        Self {
            site_name: "AWS Security Blog".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for AWSSecurityBlog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for AWSSecurityBlog {
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
        let feeds = match parsers::rss2::parse(response.text().await.unwrap().as_str()) {
            Ok(r) => r,
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
                    DateTime::parse_from_rfc2822(&feed.publish_date.clone().unwrap())
                        .unwrap()
                        .into(),
                )
            })
            .collect::<Vec<WebArticle>>();
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let cookies = self.login().await?;
        let response = self.request(url, &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await.unwrap().as_str());
        let selector = scraper::Selector::parse("article section.blog-post-content").unwrap();
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
