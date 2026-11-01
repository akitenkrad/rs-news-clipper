use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://rss.itmedia.co.jp/rss/2.0/itmedia_all.xml";
const ARTICLE_SELECTORS: &[&str] = &["p.p-syntax-sentence", "#cmsBody > div.inner > p"];

#[derive(Debug, Clone)]
pub struct ITMediaGeneral {
    site_name: String,
    url: Url,
}

impl ITMediaGeneral {
    pub fn new() -> Self {
        let url = Url::parse(URL).unwrap();
        Self {
            site_name: "ITMedia General".to_string(),
            url,
        }
    }

    fn extract_article(&self, raw: &str) -> AppResult<(Html, Text)> {
        if crate::models::web_article::detect_login_required(raw) {
            return Err(AppError::LoginRequired);
        }

        let document = scraper::Html::parse_document(raw);
        for selector_str in ARTICLE_SELECTORS {
            let selector = scraper::Selector::parse(selector_str).map_err(|e| {
                AppError::ScrapeError(format!(
                    "Failed to parse article selector ({selector_str}): {e}"
                ))
            })?;
            let raw_html = document
                .select(&selector)
                .map(|element| element.html())
                .collect::<Vec<_>>()
                .join("\n");
            if !raw_html.is_empty() {
                let html = self.clean_content(&raw_html);
                let text = html2md::rewrite_html(&html, false);
                return Ok((self.trim_text(&html), self.trim_text(&text)));
            }
        }

        Err(AppError::ScrapeError(format!(
            "Failed to find article element with selectors: {ARTICLE_SELECTORS:?}"
        )))
    }
}

impl Default for ITMediaGeneral {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for ITMediaGeneral {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.url.clone()
    }
    fn domain(&self) -> String {
        "www.itmedia.co.jp".to_string() // This is the correct domain for ITMedia General
    }

    /// ITmedia固有の除外セレクタ
    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![
            // プレミアム記事案内
            ".premium-info",
            ".premium-banner",
            // 記事評価・フィードバック
            ".article-rating",
            ".feedback",
            // メルマガ・会員登録
            ".newsletter",
            ".member-banner",
            // 続きを読むボタン
            ".read-more",
            ".colBoxPremium",
        ]
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
                return Err(AppError::ScrapeError(format!(
                    "Failed to parse RSS feed: {}",
                    e
                )));
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
        let raw = response.text().await?;
        self.extract_article(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_article(result: AppResult<(Html, Text)>, opening: &str, excluded: &str) {
        let (html, text) = result.unwrap();

        assert!(
            html.len() > 500,
            "article HTML was too short: {}",
            html.len()
        );
        assert!(
            text.len() > 500,
            "article text was too short: {}",
            text.len()
        );
        assert!(
            text.contains(opening),
            "missing opening sentence: {opening}"
        );
        assert!(
            !text.contains(excluded),
            "unexpected non-article text: {excluded}"
        );
    }

    #[test]
    fn extracts_itmedia_news_2000002722_new_cms_article() {
        let site = ITMediaGeneral::default();
        let fixture = include_str!("../../../tests/fixtures/itmedia_news_2000002722.html");

        assert_article(
            site.extract_article(fixture),
            "全日本空輸（ANA）は10月29日、ANA便の予約情報に不正アクセスがあったと発表した。",
            "関連リンク",
        );
    }

    #[test]
    fn extracts_itmedia_news_2000002775_new_cms_article() {
        let site = ITMediaGeneral::default();
        let fixture = include_str!("../../../tests/fixtures/itmedia_news_2000002775.html");

        assert_article(
            site.extract_article(fixture),
            "暗号資産取引所で世界最大手のBinanceから約63億ドル（約1兆円）相当の",
            "関連リンク",
        );
    }

    #[test]
    fn extracts_itmedia_business_news038_legacy_article() {
        let site = ITMediaGeneral::default();
        let fixture = String::from_utf8_lossy(include_bytes!(
            "../../../tests/fixtures/itmedia_business_news038.html"
        ));

        assert_article(site.extract_article(&fixture), "��3000�L�����ꂽ", "TBS");
    }
}
