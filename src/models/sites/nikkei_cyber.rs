use crate::models::provenance::Provenance;
use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use crate::shared::keychain;
use crate::shared::webdriver::{LaunchOptions, ManagedChromeDriver};
use chrono::{DateTime, Utc};
use request::Url;
use scraper::Selector;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thirtyfour::{Cookie as BrowserCookie, WebDriver};

const URL: &str = "https://www.nikkei.com/theme/?dw=21071204";
const LOGIN_URL: &str = "https://www.nikkei.com/login";
const PROBE_ARTICLE_URL: &str = "https://www.nikkei.com/article/DGXZQOUC0235C0S6A101C2000000/";
const KEYCHAIN_SERVICE: &str = "news-clipper-nikkei";
const COOKIE_ACCOUNT: &str = "cookie";
const LOGIN_INTERVAL_SECONDS: i64 = 20 * 60 * 60;
const LOGGED_OUT_MARKERS: &[&str] = &["有料会員に登録する", "有料会員が初回1カ月無料"];

#[derive(Debug, Serialize, Deserialize)]
struct AutoLoginState {
    last_attempt_at: i64,
    last_result: String,
}

#[derive(Debug, Clone)]
pub struct NikkeiCyber {
    site_name: String,
    url: Url,
    cookies: Option<String>,
}

impl NikkeiCyber {
    pub fn new() -> Self {
        Self {
            site_name: "日経 サイバーセキュリティー".to_string(),
            url: Url::parse(URL).unwrap(),
            cookies: None,
        }
    }

    fn parse_listing(&self, raw: &str) -> AppResult<Vec<WebArticle>> {
        let document = scraper::Html::parse_document(raw);
        let card_selector = Selector::parse("article").unwrap();
        let link_selector = Selector::parse(r#"h2 a[href^="/article/DGX"]"#).unwrap();
        let time_selector = Selector::parse("time[datetime]").unwrap();
        let excerpt_selector = Selector::parse(r#"[class*="excerpt"]"#).unwrap();
        let base = Url::parse("https://www.nikkei.com/").unwrap();
        let mut seen = HashSet::new();
        let mut articles = Vec::new();

        for card in document.select(&card_selector) {
            let Some(link) = card.select(&link_selector).next() else {
                continue;
            };
            let Some(href) = link.value().attr("href") else {
                continue;
            };
            let article_url = base.join(href)?.to_string();
            if !seen.insert(article_url.clone()) {
                continue;
            }
            let title = link.text().collect::<String>().trim().to_string();
            let published_at = card
                .select(&time_selector)
                .find_map(|time| time.value().attr("datetime"))
                .ok_or_else(|| AppError::ScrapeError(format!("Missing dateTime for {article_url}")))?;
            let timestamp = DateTime::parse_from_rfc3339(published_at)?.into();
            let description = card
                .select(&excerpt_selector)
                .next()
                .map(|excerpt| excerpt.text().collect::<String>().trim().to_string())
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
            return Err(AppError::ScrapeError("No articles found on the Nikkei cyber theme listing".into()));
        }
        Ok(articles)
    }

    fn extract_article(&self, raw: &str) -> AppResult<(Html, Text)> {
        let document = scraper::Html::parse_document(raw);
        let selector = Selector::parse(r#"p[class*="paragraph"]"#).unwrap();
        let raw_html = document
            .select(&selector)
            .map(|paragraph| paragraph.html())
            .collect::<Vec<_>>()
            .join("\n");
        if raw_html.is_empty() {
            return Err(AppError::ScrapeError("Failed to find Nikkei article paragraphs".into()));
        }
        let html = self.clean_content(&raw_html);
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

impl Default for NikkeiCyber {
    fn default() -> Self {
        Self::new()
    }
}

fn is_logged_out(html: &str) -> bool {
    LOGGED_OUT_MARKERS.iter().any(|marker| html.contains(marker))
}

fn is_nikkei_cookie(cookie: &BrowserCookie) -> bool {
    cookie.domain.as_deref().is_some_and(|domain| {
        let domain = domain.trim_start_matches('.');
        domain == "nikkei.com" || domain.ends_with(".nikkei.com")
    })
}

fn join_nikkei_cookies(cookies: &[BrowserCookie]) -> String {
    cookies
        .iter()
        .filter(|cookie| is_nikkei_cookie(cookie))
        .map(|cookie| format!("{}={}", cookie.name, cookie.value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn profile_dir() -> AppResult<PathBuf> {
    dirs::data_dir()
        .map(|path| path.join("news_clipper").join("chrome-profile-nikkei"))
        .ok_or_else(|| AppError::InternalError("data directory is unavailable".into()))
}

fn auto_login_state_path() -> AppResult<PathBuf> {
    dirs::cache_dir()
        .map(|path| path.join("news_clipper").join("nikkei_auto_login.json"))
        .ok_or_else(|| AppError::InternalError("cache directory is unavailable".into()))
}

fn read_auto_login_state(path: &Path) -> AppResult<Option<AutoLoginState>> {
    match std::fs::read(path) {
        Ok(contents) => Ok(Some(serde_json::from_slice(&contents)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(AppError::InternalError("failed to read Nikkei auto-login state".into())),
    }
}

fn write_auto_login_state(path: &Path, state: &AutoLoginState) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::InternalError("invalid Nikkei auto-login state path".into()))?;
    std::fs::create_dir_all(parent).map_err(|_| AppError::InternalError("failed to create Nikkei auto-login state directory".into()))?;
    let temporary_path = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&temporary_path, serde_json::to_vec(state)?)
        .map_err(|_| AppError::InternalError("failed to write temporary Nikkei auto-login state".into()))?;
    std::fs::rename(&temporary_path, path).map_err(|_| AppError::InternalError("failed to replace Nikkei auto-login state".into()))?;
    Ok(())
}

fn login_attempt_allowed(now: i64, last_attempt_at: i64) -> bool {
    now.saturating_sub(last_attempt_at) >= LOGIN_INTERVAL_SECONDS
}

fn browser_error(message: &str) -> AppError {
    AppError::InternalError(message.to_string())
}

async fn save_browser_cookies(driver: &WebDriver) -> AppResult<Cookie> {
    let cookies = driver
        .get_all_cookies()
        .await
        .map_err(|_| browser_error("failed to read Nikkei browser cookies"))?;
    let cookie_header = join_nikkei_cookies(&cookies);
    if cookie_header.is_empty() {
        return Err(browser_error("Nikkei returned no usable cookies"));
    }
    keychain::write(KEYCHAIN_SERVICE, COOKIE_ACCOUNT, &cookie_header)?;
    Ok(cookie_header)
}

async fn refresh_cookie_from_profile() -> AppResult<Cookie> {
    let profile_dir = profile_dir()?;
    std::fs::create_dir_all(&profile_dir).map_err(|_| browser_error("failed to create Nikkei Chrome profile directory"))?;
    let managed = ManagedChromeDriver::launch_with(LaunchOptions {
        headless: true,
        user_data_dir: Some(profile_dir),
    })
    .await
    .map_err(|_| browser_error("failed to launch Chrome for Nikkei cookie refresh"))?;
    let driver = managed.driver();
    let result = async {
        driver
            .goto(PROBE_ARTICLE_URL)
            .await
            .map_err(|_| browser_error("failed to open the Nikkei probe article"))?;
        let html = driver
            .source()
            .await
            .map_err(|_| browser_error("failed to inspect the Nikkei probe article"))?;
        if is_logged_out(&html) {
            return Err(browser_error("Nikkei profile is logged out"));
        }
        save_browser_cookies(driver).await
    }
    .await;
    if managed.close().await.is_err() {
        tracing::warn!("Nikkei: failed to close Chrome after cookie refresh");
    }
    result
}

async fn auto_refresh_cookie(force: bool) -> AppResult<Cookie> {
    let state_path = auto_login_state_path()?;
    let now = Utc::now().timestamp();
    if !force
        && let Some(state) = read_auto_login_state(&state_path)?
        && !login_attempt_allowed(now, state.last_attempt_at)
    {
        return Err(browser_error("Nikkei cookie refresh is limited to one attempt per 20 hours"));
    }
    write_auto_login_state(
        &state_path,
        &AutoLoginState {
            last_attempt_at: now,
            last_result: "in_progress".into(),
        },
    )?;
    let result = refresh_cookie_from_profile().await;
    let last_result = if result.is_ok() { "success" } else { "failed" };
    if write_auto_login_state(
        &state_path,
        &AutoLoginState {
            last_attempt_at: now,
            last_result: last_result.into(),
        },
    )
    .is_err()
    {
        tracing::warn!("Nikkei: failed to update auto-login state");
    }
    result
}

pub async fn manual_login() -> AppResult<Cookie> {
    let profile_dir = profile_dir()?;
    std::fs::create_dir_all(&profile_dir).map_err(|_| browser_error("failed to create Nikkei Chrome profile directory"))?;
    let managed = ManagedChromeDriver::launch_with(LaunchOptions {
        headless: false,
        user_data_dir: Some(profile_dir),
    })
    .await
    .map_err(|_| browser_error("failed to launch Chrome for Nikkei login"))?;
    let driver = managed.driver();
    let result = async {
        driver
            .goto(LOGIN_URL)
            .await
            .map_err(|_| browser_error("failed to open the Nikkei login page"))?;
        let initial_names = driver
            .get_all_cookies()
            .await
            .map_err(|_| browser_error("failed to inspect pre-login Nikkei cookies"))?
            .into_iter()
            .filter(is_nikkei_cookie)
            .map(|cookie| cookie.name)
            .collect::<HashSet<_>>();
        let deadline = Instant::now() + Duration::from_secs(5 * 60);

        loop {
            let cookies = driver
                .get_all_cookies()
                .await
                .map_err(|_| browser_error("failed to inspect Nikkei login cookies"))?;
            let has_new_cookie = cookies
                .iter()
                .filter(|cookie| is_nikkei_cookie(cookie))
                .any(|cookie| !initial_names.contains(&cookie.name));
            let left_login_page = driver.current_url().await.is_ok_and(|url| url.path().trim_end_matches('/') != "/login");

            if has_new_cookie && left_login_page {
                driver
                    .goto(PROBE_ARTICLE_URL)
                    .await
                    .map_err(|_| browser_error("failed to open the Nikkei probe article"))?;
                let html = driver
                    .source()
                    .await
                    .map_err(|_| browser_error("failed to inspect the Nikkei probe article"))?;
                if !is_logged_out(&html) {
                    return save_browser_cookies(driver).await;
                }
                driver
                    .goto(LOGIN_URL)
                    .await
                    .map_err(|_| browser_error("failed to return to the Nikkei login page"))?;
            }

            if Instant::now() >= deadline {
                return Err(browser_error("Nikkei manual login timed out after 5 minutes"));
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
    .await;
    if managed.close().await.is_err() {
        tracing::warn!("Nikkei: failed to close Chrome after manual login");
    }
    result
}

#[cfg(target_os = "macos")]
fn notify_cookie_expired() {
    static NOTIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if NOTIFIED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "display notification \"日経のログインが切れています．`cargo run --example nikkei_login` を実行してください\" with title \"news-clipper\"",
        ])
        .status();
}

#[cfg(not(target_os = "macos"))]
fn notify_cookie_expired() {}

#[async_trait::async_trait]
impl WebSiteInterface for NikkeiCyber {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }

    fn site_url(&self) -> Url {
        self.url.clone()
    }

    fn domain(&self) -> String {
        "www.nikkei.com".to_string()
    }

    fn provenance(&self) -> Provenance {
        Provenance::Reporting
    }

    async fn login(&mut self) -> AppResult<Cookie> {
        let cookies = keychain::read(KEYCHAIN_SERVICE, COOKIE_ACCOUNT)?
            .or_else(|| self.cookies.clone())
            .unwrap_or_default();
        self.cookies = Some(cookies.clone());
        Ok(cookies)
    }

    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.url.as_str(), &cookies).await?;
        self.parse_listing(&response.text().await?)
    }

    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)> {
        let mut cookies = self.login().await?;
        let mut raw = self.request(url, &cookies).await?.text().await?;
        if is_logged_out(&raw)
            && let Ok(refreshed) = auto_refresh_cookie(false).await
        {
            cookies = refreshed;
            self.cookies = Some(cookies.clone());
            raw = self.request(url, &cookies).await?.text().await?;
        }
        if is_logged_out(&raw) {
            tracing::warn!("Nikkei: cookie expired, profile refresh failed");
            notify_cookie_expired();
        }
        self.extract_article(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thirtyfour::Cookie as BrowserCookie;

    #[test]
    fn parses_nikkei_theme_fixture_without_duplicate_article_ids() {
        let site = NikkeiCyber::default();
        let fixture = include_str!("../../../tests/fixtures/nikkei_theme_cyber.html");
        let articles = site.parse_listing(fixture).unwrap();

        assert!(articles.len() >= 10, "only {} articles found", articles.len());
        assert!(articles.iter().all(|article| {
            article.article_url.starts_with("https://www.nikkei.com/article/DGX") && !article.title.is_empty() && article.timestamp.timestamp() > 0
        }));
        let unique_urls = articles
            .iter()
            .map(|article| &article.article_url)
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique_urls.len(), articles.len());
        assert!(articles[0].description.contains("不正アクセス問題"));
    }

    #[test]
    fn extracts_nikkei_article_teaser_without_paid_membership_copy() {
        let site = NikkeiCyber::default();
        let fixture = include_str!("../../../tests/fixtures/nikkei_article_DGXZQOUC0235C0S6A101C2000000.html");
        let (html, text) = site.extract_article(fixture).unwrap();

        assert!(html.contains("paragraph_"));
        assert!(text.contains("GMOインターネットグループ"));
        assert!(!text.contains("有料会員に登録する"));
        assert!(!text.contains("有料会員が初回1カ月無料"));
    }

    #[test]
    fn detects_nikkei_logged_out_markers() {
        assert!(is_logged_out("<p>有料会員に登録する</p>"));
        assert!(is_logged_out("<p>有料会員が初回1カ月無料</p>"));
        assert!(!is_logged_out("<p>記事本文だけです</p>"));
    }

    #[test]
    fn joins_only_nikkei_browser_cookies() {
        let cookie = |name: &str, value: &str, domain: &str| {
            let mut cookie = BrowserCookie::new(name, value);
            cookie.set_domain(domain);
            cookie
        };
        let cookies = vec![
            cookie("session", "member", ".nikkei.com"),
            cookie("preferences", "compact", "www.nikkei.com"),
            cookie("foreign", "secret", ".example.com"),
        ];

        assert_eq!(join_nikkei_cookies(&cookies), "session=member; preferences=compact");
    }
}
