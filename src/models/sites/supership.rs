use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::DateTime;
use request::Url;
use scraper::Selector;
use crate::shared::errors::{AppError, AppResult};

const URL: &str = "https://supership.jp/news/";

#[derive(Debug, Clone)]
pub struct Supership {
    site_name: String,
    url: Url,
}

impl Supership {
    pub fn new() -> Self {
        Self {
            site_name: "Supership".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for Supership {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Supership {

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
        let sel = Selector::parse("main article ul.p-magazine__archive li.p-magazine__card").unwrap();
        for li in doc.select(&sel) {
            let title_sel = Selector::parse("p.p-magazine__card_title").unwrap();
            let title_text = li
                .select(&title_sel)
                .next()
                .unwrap()
                .text()
                .collect::<Vec<_>>()
                .join("");
            let url_sel = Selector::parse("a").unwrap();
            let url = li
                .select(&url_sel)
                .next()
                .unwrap()
                .value()
                .attr("href")
                .unwrap()
                .to_string();
            let pubdate_sel = Selector::parse("time.p-magazine__card_time").unwrap();
            let publish_date_text = li
                .select(&pubdate_sel)
                .next()
                .unwrap()
                .text()
                .collect::<Vec<_>>()
                .join("")
                + " 00:00:00+09:00";
            let publish_date = match DateTime::parse_from_str(&publish_date_text, "%Y.%m.%d %H:%M:%S%z") {
                Ok(x) => x,
                Err(e) => {
                    println!("Got ERROR {}: {}", e, publish_date_text);
                    continue;
                }
            };
            let article = WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                title_text,
                url,
                "".to_string(),
                publish_date.into(),
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
        let sel = Selector::parse("main article div.c-grid__block--content").unwrap();
        let article = match doc.select(&sel).next() {
            Some(article) => article,
            None => {
                return Err(AppError::ScrapeError(format!("Failed to parse article: {:?}", sel)));
            }
        };
        let raw_html = article.html().to_string();
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
