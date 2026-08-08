use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://deepmind.google/blog/rss.xml";

/// Google DeepMind の公式ブログ (一次情報)．
#[derive(Debug, Clone)]
pub struct DeepMindBlog {
    site_name: String,
    url: Url,
}

impl DeepMindBlog {
    pub fn new() -> Self {
        Self {
            site_name: "Google DeepMind Blog".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for DeepMindBlog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for DeepMindBlog {
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
            Ok(feeds) => feeds,
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
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        // 本文は `main#page-content` 配下の複数の `div.rich-text` に分割されているため
        // すべて結合する．先頭 1 個だけではリード文しか取れない．
        let selector = scraper::Selector::parse("main#page-content div.rich-text").unwrap();
        let raw_html = document
            .select(&selector)
            .map(|elem| elem.html())
            .collect::<Vec<_>>()
            .join("\n");
        if raw_html.is_empty() {
            return Err(AppError::ScrapeError("Failed to parse article text".into()));
        }
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
