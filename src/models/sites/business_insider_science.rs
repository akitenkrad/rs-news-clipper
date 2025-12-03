use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::{DateTime, Local};
use request::Url;
use scraper::Selector;

const URL: &str = "https://www.businessinsider.jp/science/";

#[derive(Debug, Clone)]
pub struct BusinessInsiderScience {
    site_name: String,
    url: Url,
}

impl BusinessInsiderScience {
    pub fn new() -> Self {
        let url = Url::parse(URL).unwrap();
        BusinessInsiderScience {
            site_name: "Business Insider Science".to_string(),
            url,
        }
    }
}

impl Default for BusinessInsiderScience {
    fn default() -> Self {
        BusinessInsiderScience::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for BusinessInsiderScience {
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
        Ok(String::default())
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.url.as_str(), &cookies).await?;

        // parse html
        let doc = scraper::Html::parse_document(response.text().await.unwrap().as_str());
        let sel =
            Selector::parse("#mainContent div.p-cardList-content div.p-cardList-card").unwrap();
        let articles = doc
            .select(&sel)
            .map(|card| {
                let a_sel = Selector::parse("h1 a").unwrap();
                let title_text = card
                    .select(&a_sel)
                    .next()
                    .unwrap()
                    .text()
                    .collect::<Vec<_>>()
                    .join("");
                let url = card
                    .select(&a_sel)
                    .next()
                    .unwrap()
                    .value()
                    .attr("href")
                    .unwrap();

                let date_sel = Selector::parse("ul li.p-cardList-cardDate").unwrap();
                let mut date_text = match card.select(&date_sel).next() {
                    Some(x) => x.text().collect::<Vec<_>>().join(""),
                    None => String::default(),
                };
                let reg = regex::Regex::new(r"[\d]{1,2}h ago").unwrap();
                if reg.is_match(&date_text) {
                    date_text = Local::now().format("%b. %d, %Y 00:00:00+09:00").to_string();
                } else {
                    date_text = date_text + " 00:00:00+09:00";
                }
                let date = match DateTime::parse_from_str(&date_text, "%b. %d, %Y %H:%M:%S%z") {
                    Ok(x) => x.with_timezone(&Local),
                    Err(_) => DateTime::<Local>::default(),
                };
                WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    title_text,
                    "https://www.businessinsider.jp".to_string() + &url,
                    "".to_string(),
                    date.into(),
                )
            })
            .collect::<Vec<WebArticle>>();
        Ok(articles)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let doc = scraper::Html::parse_document(response.text().await.unwrap().as_str());
        let sel = Selector::parse("article div.p-post-content").unwrap();
        match doc.select(&sel).next() {
            Some(elem) => {
                let html = elem.html().to_string();
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            None => Err(AppError::ScrapeError("Failed to parse article text".into())),
        }
    }
}
