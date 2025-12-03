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

            let article = WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                post.select(&title_selector).next().unwrap().text().collect(),
                post.select(&url_selector)
                    .next()
                    .unwrap()
                    .value()
                    .attr("href")
                    .unwrap()
                    .to_string(),
                post.select(&desc_selector).next().unwrap().text().collect(),
                DateTime::parse_from_str(
                    &format!(
                        "{} 00:00:00+0900",
                        post.select(&date_selector)
                            .next()
                            .unwrap()
                            .text()
                            .collect::<Vec<_>>()
                            .join("")
                    ),
                    "%Y-%m-%d %H:%M:%S%z",
                )
                .unwrap()
                .into(),
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
        let html = article.html().to_string();
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
