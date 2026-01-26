use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::{
    errors::{AppError, AppResult},
};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://engineering.dena.com/index.xml";

#[derive(Debug, Clone)]
pub struct DeNAEngineeringBlog {
    site_name: String,
    url: Url,
}

impl DeNAEngineeringBlog {
    pub fn new() -> Self {
        Self {
            site_name: "DeNA Engineering Blog".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for DeNAEngineeringBlog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for DeNAEngineeringBlog {
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
        let cookie = self.login().await?;
        let response = self.request(self.url.as_str(), &cookie).await?;
        let feeds = match parsers::rss2::parse(response.text().await?.as_str()) {
            Ok(feeds) => feeds,
            Err(e) => {
                return Err(AppError::ScrapeError(format!("Failed to parse RSS: {}", e)));
            }
        };
        let articles = feeds
            .iter()
            .filter_map(|feed| {
                if !feed.link.starts_with("https://engineering.dena.com/blog") {
                    return None;
                }
                Some(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title.clone(),
                    feed.link.clone(),
                    feed.description.clone().unwrap_or("".to_string()),
                    DateTime::parse_from_rfc2822(&feed.publish_date.clone().unwrap())
                        .unwrap()
                        .into(),
                ))
            })
            .collect::<Vec<WebArticle>>();
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookie = self.login().await?;
        let response = self.request(url.as_str(), &cookie).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = scraper::Selector::parse("main article section.content-box").unwrap();
        match document.select(&selector).next() {
            Some(elem) => {
                let raw_html = elem.html().to_string();
                let html = self.clean_content(&raw_html);
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            None => Err(AppError::ScrapeError("Failed to parse article text".into())),
        }
    }
}
