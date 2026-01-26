use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use crate::shared::errors::{AppError, AppResult};

const URL: &str = "https://rss.itmedia.co.jp/rss/2.0/ait.xml";

#[derive(Debug, Clone)]
pub struct ITMediaAtIt {
    site_name: String,
    url: Url,
}

impl ITMediaAtIt {
    pub fn new() -> Self {
        Self {
            site_name: "ITMedia @IT".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for ITMediaAtIt {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for ITMediaAtIt {

    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.url.clone()
    }
    fn domain(&self) -> String {
        "atmarkit.itmedia.co.jp".to_string() // This is the correct domain for @IT
    }

    /// ITmedia固有の除外セレクタ
    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![
            ".premium-info", ".premium-banner",
            ".article-rating", ".feedback",
            ".newsletter", ".member-banner",
            ".read-more", ".colBoxPremium",
        ]
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        Ok(Cookie::default())
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.url.as_str(), &cookies).await?;
        let feeds = match parsers::rss2::parse(response.text().await?.as_str()) {
            Ok(feeds) => feeds,
            Err(e) => return Err(AppError::ScrapeError(format!("Failed to parse RSS feed: {}", e))),
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
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = match scraper::Selector::parse("#cmsBody div.inner p") {
            Ok(selector) => selector,
            Err(e) => {
                return Err(AppError::ScrapeError(format!(
                    "Failed to parse selector (#cmsBody div.inner p): {}",
                    e
                )));
            }
        };
        let article = match document.select(&selector).next() {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(
                    "Failed to parse article: #cmsBody div.inner p".to_string(),
                ));
            }
        };
        let raw_html = article.html().to_string();
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
