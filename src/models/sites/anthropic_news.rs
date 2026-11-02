use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::{Local, NaiveDate, TimeZone};
use request::Url;
use scraper::Selector;
use std::collections::HashSet;

const URL: &str = "https://www.anthropic.com/news";

/// Anthropic の公式ニュース (一次情報)．
///
/// RSS/Atom フィードが提供されていないため一覧ページをスクレイピングする．
/// CSS クラス名はビルドごとに変わるハッシュ付きの CSS Modules 名なので，
/// クラスではなく article 内の `header time` と `a > h3` という構造だけに依存する．
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

    fn parse_listing(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let document = scraper::Html::parse_document(raw);
        let article_selector = Selector::parse("article").unwrap();
        let time_selector = Selector::parse("header time").unwrap();
        let link_selector = Selector::parse("a[href]").unwrap();
        let heading_selector = Selector::parse("h3").unwrap();
        let summary_selector = Selector::parse("p").unwrap();
        let base = Url::parse("https://www.anthropic.com/").unwrap();
        let mut seen = HashSet::new();
        let mut articles = Vec::new();

        for tile in document.select(&article_selector) {
            let Some(timestamp) = tile
                .select(&time_selector)
                .next()
                .and_then(|time| parse_listing_date(&time.text().collect::<String>()))
            else {
                continue;
            };
            let Some(link) = tile.select(&link_selector).find(|link| link.select(&heading_selector).next().is_some()) else {
                continue;
            };
            let Some(href) = link.value().attr("href") else {
                continue;
            };
            let article_url = base.join(href)?.to_string();
            if !seen.insert(article_url.clone()) {
                continue;
            }
            let title = link
                .select(&heading_selector)
                .next()
                .map(|heading| heading.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            if title.is_empty() {
                continue;
            }
            let description = link
                .select(&summary_selector)
                .next()
                .map(|summary| summary.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            articles.push(WebArticle::new(
                self.site_name(),
                self.site_url().to_string(),
                title,
                article_url,
                description,
                timestamp,
            ));
        }

        if articles.is_empty() {
            return Err(AppError::ScrapeError("No articles found on the Anthropic news listing".into()));
        }
        Ok(articles)
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
    Local.from_local_datetime(&date.and_hms_opt(0, 0, 0)?).single()
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
        self.parse_listing(&response.text().await?)
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

    #[test]
    fn parses_anthropic_article_tiles_with_external_header_time() {
        let site = AnthropicNews::default();
        let fixture = include_str!("../../../tests/fixtures/anthropic_news.html");
        let articles = site.parse_listing(fixture).unwrap();

        assert_eq!(articles.len(), 5);
        assert!(articles.iter().any(|article| {
            article.article_url == "https://www.anthropic.com/research/early-progress-drug-development"
                && article.title == "Early progress on accelerating drug development with Claude"
                && article.description.contains("six organizations")
                && article.timestamp.format("%Y-%m-%d").to_string() == "2026-10-27"
        }));
        assert!(
            articles
                .iter()
                .all(|article| article.article_url != "https://www.anthropic.com/news/announcements")
        );
    }
}
