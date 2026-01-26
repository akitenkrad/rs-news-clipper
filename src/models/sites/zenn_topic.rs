use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use crate::shared::{
    errors::{AppError, AppResult},
};

const URL: &str = "https://zenn.dev/topics/{}/feed";

#[derive(Debug, Clone)]
pub struct ZennTopic {
    site_name: String,
    url: Url,
    pub topic: String,
}

impl ZennTopic {
    pub fn new(topic: &str) -> Self {
        Self {
            site_name: format!("Zenn Topic - {}", topic).to_string(),
            topic: topic.to_string(),
            url: Url::parse(URL.replace("{}", topic).as_str()).unwrap(),
        }
    }
    pub fn get_url(&self) -> String {
        URL.replace("{}", &self.topic)
    }
}

impl Default for ZennTopic {
    fn default() -> Self {
        Self::new("自然言語処理")
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for ZennTopic {

    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.url.clone()
    }
    fn domain(&self) -> String {
        self.url.domain().unwrap().to_string()
    }

    /// Zenn固有の除外セレクタ
    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![
            ".LikeButton", ".BookmarkButton",
            ".AuthorProfile", ".SupportButton",
        ]
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        Ok(Cookie::default())
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let url = Url::parse(self.get_url().as_str()).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let feeds = match parsers::rss2::parse(response.text().await?.as_str()) {
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
        let selector = scraper::Selector::parse("article section").unwrap();
        let article = match document.select(&selector).next() {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(format!(
                    "Failed to parse article: {:?}",
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
