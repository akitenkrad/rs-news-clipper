use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use chrono::DateTime;
use feed_parser::parsers;
use request::Url;

const URL: &str = "https://www.gizmodo.jp/index.xml";
const ARTICLE_SELECTOR: &str = "div.article_pArticle_Body";

#[derive(Debug, Clone)]
pub struct Gizmodo {
    site_name: String,
    url: Url,
}

impl Gizmodo {
    pub fn new() -> Self {
        Self {
            site_name: "Gizmodo".to_string(),
            url: Url::parse(URL).unwrap(),
        }
    }

    fn extract_article(&self, raw: &str) -> AppResult<(Html, Text)> {
        let document = scraper::Html::parse_document(raw);
        let selector = scraper::Selector::parse(ARTICLE_SELECTOR).map_err(|e| {
            AppError::ScrapeError(format!(
                "Failed to parse article selector ({ARTICLE_SELECTOR}): {e}"
            ))
        })?;
        let article = document
            .select(&selector)
            .next()
            .ok_or_else(|| AppError::ScrapeError("Failed to parse article text".into()))?;
        let html = self.clean_content(&article.html());
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

impl Default for Gizmodo {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for Gizmodo {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.url.clone()
    }
    fn domain(&self) -> String {
        self.url.domain().unwrap().to_string()
    }

    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![
            "[class*='adDisplayhandler_']",
            "[class*='bodyAd_']",
            "[class*='loglyLiftAd_']",
            ".embedblock",
        ]
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
        assert!(!text.contains("Advertisement"));
    }

    #[test]
    fn extracts_gizmodo_zelda_article() {
        let site = Gizmodo::default();
        let fixture = include_str!("../../../tests/fixtures/gizmodo_zelda.html");

        assert_article(site.extract_article(fixture), "これはずるい。");
    }

    #[test]
    fn extracts_gizmodo_blackfriday_article() {
        let site = Gizmodo::default();
        let fixture = include_str!("../../../tests/fixtures/gizmodo_blackfriday.html");

        assert_article(
            site.extract_article(fixture),
            "メガネやサングラスを少しの間だけ外したいとき、",
        );
    }
}
