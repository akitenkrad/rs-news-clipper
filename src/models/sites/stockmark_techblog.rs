use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use request::Url;
use scraper::Selector;
use crate::shared::errors::{AppError, AppResult};

const URL: &str = "https://stockmark-tech.hatenablog.com/";

#[derive(Debug, Clone)]
pub struct StockmarkTechBlog {
    site_name: String,
    url: Url,
}

impl StockmarkTechBlog {
    pub fn new() -> Self {
        Self {
            site_name: "Stockmark Tech Blog".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for StockmarkTechBlog {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for StockmarkTechBlog {

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
        let doc = scraper::Html::parse_document(response.text().await?.as_str());

        // parse html
        let mut articles: Vec<WebArticle> = Vec::new();
        let post_selector = Selector::parse("#main").unwrap();
        let posts = doc.select(&post_selector);
        for post in posts {
            let desc_selector = Selector::parse("div.archive-entry-body p.entry-description").unwrap();
            let title_selector = Selector::parse("div.archive-entry-header").unwrap();
            let url_selector = Selector::parse("div.archive-entry-header h1 a").unwrap();
            let date_selector = Selector::parse("div.archive-entry-header div.archive-date").unwrap();

            let title = match post.select(&title_selector).next() {
                Some(elem) => elem.text().collect(),
                None => continue,
            };
            let url = match post.select(&url_selector).next() {
                Some(elem) => match elem.value().attr("href") {
                    Some(href) => href.to_string(),
                    None => continue,
                },
                None => continue,
            };
            let desc = match post.select(&desc_selector).next() {
                Some(elem) => elem.text().collect(),
                None => String::new(),
            };
            let date_text = match post.select(&date_selector).next() {
                Some(elem) => elem.text().collect::<Vec<_>>().join(""),
                None => continue,
            };
            let publish_date = match DateTime::parse_from_str(
                &format!("{} 00:00:00+0900", date_text),
                "%Y-%m-%d %H:%M:%S%z",
            ) {
                Ok(date) => date.into(),
                Err(_) => continue,
            };

            let article = WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                title,
                url,
                desc,
                publish_date,
            );
            articles.push(article);
        }
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = Selector::parse("#main div.entry-inner").unwrap();
        let article = match doc.select(&selector).next() {
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
