use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://www.sbbit.jp/rss/HotTopics.rss";

#[derive(Debug, Clone)]
pub struct Sbbit {
    site_name: String,
    url: Url,
}

impl Sbbit {
    pub fn new() -> Self {
        Self {
            site_name: "ビジネス＋IT".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for Sbbit {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Sbbit {
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
        let feeds = match parsers::rss2::parse(response.text().await?.as_str()) {
            Ok(feed) => feed,
            Err(e) => {
                return Err(AppError::ScrapeError(format!("Failed to parse RSS: {}", e)));
            }
        };
        let articles = feeds
            .iter()
            .map(|feed| -> AppResult<WebArticle> {
                let publish_date = feed
                    .publish_date
                    .clone()
                    .ok_or_else(|| AppError::ScrapeError("Missing publish_date".into()))?;
                Ok(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    feed.title.clone(),
                    feed.link.clone(),
                    feed.description.clone().unwrap_or("".to_string()),
                    DateTime::parse_from_rfc2822(&publish_date)?.into(),
                ))
            })
            .collect::<AppResult<Vec<WebArticle>>>()?;
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let raw = response.text().await?;
        // ビジネス＋IT is membership-gated; the full text sits behind a 会員登録 /
        // 会員限定 wall. Detect it before extraction and bail out rather than
        // returning a truncated teaser.
        if crate::models::web_article::detect_login_required(&raw) {
            return Err(AppError::LoginRequired);
        }
        let document = scraper::Html::parse_document(raw.as_str());
        let selector = scraper::Selector::parse("div.article_note").unwrap();
        match document.select(&selector).next() {
            Some(elem) => {
                let raw_html = elem.html().to_string();
                let html = self.clean_content(&raw_html);
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            // When the body container is absent we assume the content is gated.
            None => Err(AppError::LoginRequired),
        }
    }
}
