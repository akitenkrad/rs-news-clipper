use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://openai.com/news/rss.xml";

/// OpenAI の公式ニュース (一次情報)．
///
/// フィードは全アーカイブ (1000 件超) を返すが，下流のコレクタが
/// `timestamp` でカットオフしてから本文を取得するため問題にはならない．
#[derive(Debug, Clone)]
pub struct OpenAINews {
    site_name: String,
    url: Url,
}

impl OpenAINews {
    pub fn new() -> Self {
        Self {
            site_name: "OpenAI News".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for OpenAINews {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for OpenAINews {
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

        // 記事ページは bot 対策で 403 を返し，JavaScript 必須のシェルしか得られない．
        // フィードには title と description が揃っているため，本文なしの
        // メタデータのみで取り込めるよう LoginRequired を返す
        // (コレクタはこのエラーだけ「本文空で記事は残す」扱いにする)．
        if response.status().is_client_error() {
            return Err(AppError::LoginRequired);
        }

        let raw = response.text().await?;
        if crate::models::web_article::detect_login_required(&raw) {
            return Err(AppError::LoginRequired);
        }
        let document = scraper::Html::parse_document(raw.as_str());
        let selector = scraper::Selector::parse("main").unwrap();
        match document.select(&selector).next() {
            Some(elem) => {
                let raw_html = elem.html().to_string();
                let html = self.clean_content(&raw_html);
                let text = html2md::rewrite_html(&html, false);
                Ok((self.trim_text(&html), self.trim_text(&text)))
            }
            // セレクタで取れない場合も本文なしで記事だけは残す．
            None => Err(AppError::LoginRequired),
        }
    }
}
