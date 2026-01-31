use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::{DateTime, Local};
use request::Url;
use scraper::Selector;

const URL: &str = "https://ai-scholar.tech/";

#[derive(Debug, Clone)]
pub struct AIScholar {
    site_name: String,
    url: Url,
}

impl AIScholar {
    pub fn new() -> Self {
        Self {
            site_name: "AI Scholar".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for AIScholar {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for AIScholar {
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
        // No login required
        Ok(String::new())
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let mut cookies = self.login().await?;
        cookies.push_str("display_language=ja;");
        let response = self.request(self.url.as_str(), &cookies).await?;

        // parse html
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let sel =
            Selector::parse("body div.content main.main section.indexlists article.list-item")
                .unwrap();
        let articles = doc
            .select(&sel)
            .filter_map(|article| {
                let a_sel = Selector::parse("a").unwrap();
                let a_elem = article.select(&a_sel).next()?;
                let title_text = a_elem.text().collect::<Vec<_>>().join("");
                let url = a_elem.value().attr("href")?;
                let date_sel = Selector::parse("a div.list-item__description time").unwrap();
                let mut date_text = match article.select(&date_sel).next() {
                    Some(x) => x.value().attr("datetime").unwrap_or_default().to_string(),
                    None => String::default(),
                };
                date_text.push_str("+09:00");
                let desc_sel = Selector::parse("a div.list-item__description span").unwrap();
                let desc_text = match article.select(&desc_sel).next() {
                    Some(x) => x.text().collect::<Vec<_>>().join(""),
                    None => String::default(),
                };
                let date = match DateTime::parse_from_str(&date_text, "%Y-%m-%d %H:%M:%S%z") {
                    Ok(x) => x.with_timezone(&Local),
                    Err(_) => DateTime::<Local>::default(),
                };
                Some(WebArticle::new(
                    self.site_name(),
                    self.site_url().to_string(),
                    title_text,
                    url.to_string(),
                    desc_text,
                    date,
                ))
            })
            .collect::<Vec<WebArticle>>();
        Ok(articles)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let cookies = self.login().await?;
        let response = self.request(url, &cookies).await?;
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let sel = Selector::parse("article").unwrap();
        match doc.select(&sel).next() {
            Some(elem) => {
                let raw_html = elem.html().to_string();
                let html = self.clean_content(&raw_html);
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            None => Err(AppError::ScrapeError("Failed to parse article text".into())),
        }
    }
}
