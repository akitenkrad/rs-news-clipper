use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;
use crate::shared::{
    errors::{AppError, AppResult},
};

const URL: &str = "https://engineering.mercari.com/blog/feed.xml";

#[derive(Debug, Clone)]
pub struct MercariEngineeringBlog {
    site_name: String,
    url: Url,
}

impl MercariEngineeringBlog {
    pub fn new() -> Self {
        Self {
            site_name: "Mercari Engineering Blog".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for MercariEngineeringBlog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for MercariEngineeringBlog {

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
                tracing::error!("Error parsing RSS feed: {}", e);
                return Err(AppError::ScrapeError("Failed to parse RSS".into()));
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

        // Try multiple selectors for robustness (Astro migration changed the page structure)
        let selectors = [
            "div.page-content",
            "main div.page-content",
            "main section div._body_5d9ad_19",
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
                return Err(AppError::ScrapeError(format!(
                    "Failed to parse article: no matching selector found for Mercari Engineering Blog"
                )));
            }
        };
        let html = article.html().to_string();
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
