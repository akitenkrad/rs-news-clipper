use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::{Local, NaiveDate, TimeZone};
use regex::Regex;
use request::Url;
use scraper::Selector;
use std::collections::HashSet;

const URL: &str = "https://www.anthropic.com/news";

/// Anthropic の公式ニュース (一次情報)．
///
/// RSS/Atom フィードが提供されていないため一覧ページをスクレイピングする．
/// CSS クラス名はビルドごとに変わるハッシュ付きの CSS Modules 名なので，
/// クラスではなく `a[href^="/news/"]` と `<time>` という構造だけに依存する．
#[derive(Debug, Clone)]
pub struct AnthropicNews {
    site_name: String,
    url: Url,
}

impl AnthropicNews {
    pub fn new() -> Self {
        Self {
            site_name: "Anthropic News".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }
}

impl Default for AnthropicNews {
    fn default() -> Self {
        Self::new()
    }
}

/// `Jul 24, 2026` 形式の日付をパースする．
fn parse_listing_date(text: &str) -> Option<chrono::DateTime<Local>> {
    let date = NaiveDate::parse_from_str(text.trim(), "%b %e, %Y")
        .or_else(|_| NaiveDate::parse_from_str(text.trim(), "%B %e, %Y"))
        .ok()?;
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .single()
}

#[async_trait::async_trait]
impl WebSiteInterface for AnthropicNews {
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
        let document = scraper::Html::parse_document(response.text().await?.as_str());

        let link_sel = Selector::parse(r#"a[href^="/news/"]"#).unwrap();
        let time_sel = Selector::parse("time").unwrap();
        let heading_sel = Selector::parse("h1, h2, h3, h4").unwrap();
        let span_sel = Selector::parse("span").unwrap();
        let date_re = Regex::new(r"^[A-Z][a-z]{2,8}\s+\d{1,2},\s+\d{4}$").unwrap();

        let mut seen: HashSet<String> = HashSet::new();
        let mut articles = Vec::new();

        for elem in document.select(&link_sel) {
            let href = match elem.value().attr("href") {
                Some(h) => h,
                None => continue,
            };
            // `/news` そのものへのリンク (もっと見る等) は記事ではない．
            if href.trim_end_matches('/') == "/news" {
                continue;
            }
            if !seen.insert(href.to_string()) {
                continue;
            }

            let date_text = elem
                .select(&time_sel)
                .map(|t| t.text().collect::<String>())
                .find(|t| date_re.is_match(t.trim()));
            let timestamp = match date_text.as_deref().and_then(parse_listing_date) {
                Some(ts) => ts,
                // 日付が取れない項目は取り込まない．誤った日付で登録すると
                // 下流の公開日フィルタが誤動作するため，落とす方が安全．
                None => continue,
            };

            // タイトル: 見出し要素があればそれ，無ければアンカー内の
            // 最長のテキスト (日付・カテゴリ表記を除く) を採用する．
            let title = elem
                .select(&heading_sel)
                .map(|h| h.text().collect::<String>().trim().to_string())
                .find(|t| !t.is_empty())
                .or_else(|| {
                    elem.select(&span_sel)
                        .map(|s| s.text().collect::<String>().trim().to_string())
                        .filter(|t| !t.is_empty() && !date_re.is_match(t))
                        .max_by_key(|t| t.chars().count())
                })
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }

            articles.push(WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                title,
                format!("https://www.anthropic.com{}", href),
                String::new(),
                timestamp,
            ));
        }

        if articles.is_empty() {
            return Err(AppError::ScrapeError(
                "No articles found on the Anthropic news listing".into(),
            ));
        }
        Ok(articles)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let url = Url::parse(url).unwrap();
        let cookies = self.login().await?;
        let response = self.request(url.as_str(), &cookies).await?;
        let document = scraper::Html::parse_document(response.text().await?.as_str());
        let selector = Selector::parse("article").unwrap();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_listing_date() {
        let d = parse_listing_date("Jul 24, 2026").expect("should parse abbreviated month");
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-07-24");
        // 1 桁の日 (`Aug 7, 2026`) も一覧に現れる．
        let d = parse_listing_date("Aug 7, 2026").expect("should parse single-digit day");
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2026-08-07");
        let d = parse_listing_date("January 3, 2025").expect("should parse full month");
        assert_eq!(d.format("%Y-%m-%d").to_string(), "2025-01-03");
        assert!(parse_listing_date("Product").is_none());
        assert!(parse_listing_date("").is_none());
    }
}
