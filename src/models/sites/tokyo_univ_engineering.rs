use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use crate::shared::{
    errors::{AppError, AppResult},
};

const URL: &str = "https://www.t.u-tokyo.ac.jp/press/rss.xml";

#[derive(Debug, Clone)]
pub struct TokyoUniversityEngineering {
    site_name: String,
    url: Url,
}

impl TokyoUniversityEngineering {
    pub fn new() -> Self {
        let url = Url::parse(URL).unwrap();
        Self {
            site_name: "Tokyo University Engineering".to_string(),
            url,
        }
    }
}

impl Default for TokyoUniversityEngineering {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for TokyoUniversityEngineering {

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

        let selectors = [
            "div.blog-body-1__content",
            "main div.ly_cont div.blog_title",
            "div.bl_wysiwyg",
        ];

        let mut article_element = None;
        for sel_str in &selectors {
            let selector = scraper::Selector::parse(sel_str).unwrap();
            if let Some(element) = document.select(&selector).next() {
                article_element = Some(element);
                break;
            }
        }

        let article = match article_element {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(
                    "Failed to find article content: no matching selector found for Tokyo University Engineering".into(),
                ));
            }
        };
        let raw_html = article.html().to_string();
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
