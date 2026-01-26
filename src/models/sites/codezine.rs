use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use chrono::{DateTime, Local};
use request::Url;
use scraper::Selector;
use crate::shared::{
    errors::{AppError, AppResult},
};

const URL: &str = "https://codezine.jp/news";

#[derive(Debug, Clone)]
pub struct CodeZine {
    site_name: String,
    url: Url,
}

impl CodeZine {
    pub fn new() -> Self {
        let url = Url::parse(URL).unwrap();
        CodeZine {
            site_name: "CodeZine".to_string(),
            url,
        }
    }
}

impl Default for CodeZine {
    fn default() -> Self {
        CodeZine::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for CodeZine {

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

        // parse html
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let sel = Selector::parse("ul.c-articleindex_list").unwrap();
        let mut articles = Vec::new();
        for ul in doc.select(&sel) {
            let sel = Selector::parse("li.c-articleindex_listitem").unwrap();
            for item in ul.select(&sel) {
                // title, url
                let title_sel = Selector::parse("p.c-articleindex_item_heading a").unwrap();
                let title = item.select(&title_sel).next().unwrap();
                let tilte_text = title.text().collect::<Vec<_>>().join("");
                let url = title.value().attr("href").unwrap().to_string();

                // date
                let date_sel = Selector::parse("p.c-featureindex_item_date").unwrap();
                let date_text = match item.select(&date_sel).next() {
                    Some(x) => x.text().collect::<Vec<_>>().join(""),
                    None => String::default(),
                };
                let date_text = date_text + " 00:00:00+09:00";
                let date = match DateTime::parse_from_str(&date_text, "%Y/%m/%d %H:%M:%S%z") {
                    Ok(x) => x.with_timezone(&Local),
                    Err(_) => DateTime::<Local>::default(),
                };

                articles.push(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    tilte_text,
                    "https://codezine.jp".to_string() + &url,
                    "".to_string(),
                    date.into(),
                ));
            }
        }
        Ok(articles)
    }
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let sel = Selector::parse("main article div.detailBlock").unwrap();
        match doc.select(&sel).next() {
            Some(elem) => {
                let raw_html = elem.html().to_string();
                let html = self.clean_content(&raw_html);
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            None => {
                Err(AppError::ScrapeError("Failed to parse article text".into()))
            }
        }
    }
}
