use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use request::Url;
use scraper::Selector;
use crate::shared::errors::{AppError, AppResult};

const URL: &str = "https://medium.com/tag/{}/archive";

#[derive(Debug, Clone)]
pub struct Medium {
    site_name: String,
    url: Url,
    tag: String,
}

impl Medium {
    pub fn new(target: &str, tag: &str) -> Self {
        return Medium {
            site_name: format!("Medium {}", target).to_string(),
            tag: tag.to_string(),
            url: Url::parse(URL.replace("{}", tag).as_str()).unwrap(),
        };
    }
    pub fn get_url(&self) -> String {
        URL.replace("{}", &self.tag)
    }
}

impl Default for Medium {
    fn default() -> Self {
        Medium::new("AI", "ai")
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Medium {

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
        let mut articles: Vec<WebArticle> = Vec::new();
        // parse html
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let sel = Selector::parse("article").unwrap();
        for article in doc.select(&sel) {
            let title_sel = Selector::parse("a h2").unwrap();
            let title_text = match article.select(&title_sel).next() {
                Some(elem) => elem.text().collect::<Vec<_>>().join(""),
                None => continue,
            };
            let mut url = Url::parse("https://medium.com").unwrap();
            let a_sel = Selector::parse("div a").unwrap();
            let href = match article.select(&a_sel).next() {
                Some(elem) => match elem.value().attr("href") {
                    Some(h) => h,
                    None => continue,
                },
                None => continue,
            };
            if href.contains("https://") {
                url = Url::parse(href).unwrap();
            } else {
                url.set_path(href);
            }
            let date_sel = Selector::parse("span").unwrap();

            match article.select(&date_sel).next() {
                Some(x) => {
                    let _text = x.text().collect::<Vec<_>>().join("").trim().to_string().to_lowercase();
                    if !(_text.contains("just now") || _text.contains("h ago") || _text.contains("m ago")) {
                        println!("{} is not recent", _text);
                        continue;
                    }
                }
                None => {
                    println!("No date found");
                    continue;
                }
            };
            let date = chrono::Local::now();
            let desc_sel = Selector::parse("a h3").unwrap();
            let desc_text = match article.select(&desc_sel).next() {
                Some(x) => x.text().collect::<Vec<_>>().join(""),
                None => "".to_string(),
            };
            let article = WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                title_text,
                url.to_string(),
                desc_text,
                date.into(),
            );
            articles.push(article);
        }
        Ok(articles)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url)?;
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let doc = scraper::Html::parse_document(response.text().await?.as_str());
        let sel = match Selector::parse("article") {
            Ok(s) => s,
            Err(e) => {
                return Err(AppError::ScrapeError(format!("Failed to parse selector: {}", e)));
            }
        };
        let (html, text) = match doc.select(&sel).next() {
            Some(elem) => {
                let raw_html = elem.html().to_string();
                let html = self.clean_content(&raw_html);
                (html.clone(), html2md::rewrite_html(&html, false))
            }
            None => ("NO HTML".into(), "NO TEXT".into()),
        };
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}
