use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://rss.arxiv.org/rss/{}";

/// arXiv の新着アナウンス (一次情報)．
///
/// カテゴリ (`cs.CR`, `cs.CL`, `cs.MA` 等) ごとにインスタンスを作る．
/// フィードは平日のみ更新され，週末・祝日は項目が空になる (エラーではない)．
/// `cs.CL` のような大カテゴリは 1 日あたりの件数が多いため，
/// 追加する際は下流の推薦件数への影響を確認すること．
#[derive(Debug, Clone)]
pub struct Arxiv {
    site_name: String,
    url: Url,
    pub category: String,
}

impl Arxiv {
    pub fn new(category: &str) -> Self {
        Self {
            site_name: format!("arXiv {}", category),
            category: category.to_string(),
            url: Url::parse(URL.replace("{}", category).as_str()).unwrap(),
        }
    }
}

impl Default for Arxiv {
    fn default() -> Self {
        Self::new("cs.CR")
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Arxiv {
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
        // abs ページの本文は要旨のみ．全文 PDF は取得対象にしない．
        let selector = scraper::Selector::parse("blockquote.abstract").unwrap();
        match document.select(&selector).next() {
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
