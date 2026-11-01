use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://ascii.jp/rss.xml";
const ARTICLE_SELECTORS: &[&str] = &[
    "#mainC > p:not(.twitBtn):not(.returnCat), #mainC > h2, #mainC > div.photo",
    "#contents_detail > p, #contents_detail > h2, #contents_detail > div.photo",
    "#detail_contents",
];

#[derive(Debug, Clone)]
pub struct Ascii {
    site_name: String,
    url: Url,
}

impl Ascii {
    pub fn new() -> Self {
        Self {
            site_name: "ASCII.jp".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn extract_article(&self, raw: &str) -> AppResult<(Html, Text)> {
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

        Err(AppError::ScrapeError("Failed to parse article text".into()))
    }
}

impl Default for Ascii {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Ascii {
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
            Ok(feed) => feed,
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
        self.extract_article(&response.text().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_article(result: AppResult<(Html, Text)>, opening: &str) {
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
        assert!(!text.contains("この記事の編集者は以下の記事をオススメしています"));
    }

    #[test]
    fn extracts_ascii_4443888_article() {
        let site = Ascii::default();
        let fixture = include_str!("../../../tests/fixtures/ascii_4443888.html");

        assert_article(
            site.extract_article(fixture),
            "ARグラスは各社から新製品が登場し、選択肢が増えている。",
        );
    }

    #[test]
    fn extracts_ascii_4441800_article() {
        let site = Ascii::default();
        let fixture = include_str!("../../../tests/fixtures/ascii_4441800.html");

        assert_article(
            site.extract_article(fixture),
            "「ROG Xbox Ally X20」は、ROGブランド設立20周年記念モデル",
        );
    }

    #[test]
    fn extracts_ascii_elem_4447476_legacy_article() {
        let site = Ascii::default();
        let fixture = include_str!("../../../tests/fixtures/ascii_elem_4447476.html");

        assert_article(
            site.extract_article(fixture),
            "自動車好き女子の矢田部明子です！今回は、中古車なら200万円台で狙える",
        );
    }

    #[test]
    fn extracts_ascii_elem_4446501_contents_detail_article() {
        let site = Ascii::default();
        let fixture =
            include_str!("../../../tests/fixtures/ascii_elem_4446501_contents_detail.html");

        let result = site.extract_article(fixture);
        let (_, text) = result.as_ref().unwrap();
        assert!(
            !text.contains("関連サイト"),
            "関連サイトの欄が本文に混ざっている"
        );
        assert_article(result, "持ち帰り弁当の「ほっともっと」は11月1日から");
    }
}
