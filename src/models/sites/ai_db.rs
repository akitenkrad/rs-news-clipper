use crate::models::web_article::{Cookie, Html, Text, WebArticle, WebSiteInterface};
use crate::shared::errors::{AppError, AppResult};
use crate::shared::keychain;
use crate::shared::webdriver::{LaunchOptions, ManagedChromeDriver};
use chrono::{DateTime, Utc};
use feed_parser::parsers;
use request::Url;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thirtyfour::{By, Cookie as BrowserCookie, WebDriver, prelude::ElementQueryable};

const URL: &str = "https://ai-data-base.com/feed";
const LOGIN_URL: &str = "https://ai-data-base.com/membership-login";
const KEYCHAIN_SERVICE: &str = "news-clipper-aidb";
const COOKIE_ACCOUNT: &str = "cookie";
const USER_ACCOUNT: &str = "user";
const PASSWORD_ACCOUNT: &str = "password";
const LOGGED_OUT_MARKER: &str = "プレミアム会員限定コンテンツです";
const REFRESH_BEFORE_SECONDS: i64 = 48 * 60 * 60;
const LOGIN_INTERVAL_SECONDS: i64 = 20 * 60 * 60;

#[derive(Debug, Serialize, Deserialize)]
struct AutoLoginState {
    last_attempt_at: i64,
    last_result: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArticleDesign {
    Legacy,
    Timeline,
}

#[derive(Debug)]
struct FetchedPage {
    final_url: String,
    html: String,
}

#[derive(Debug)]
struct FetchedArticle {
    page_html: String,
    article_html: String,
}

#[derive(Debug, Clone)]
pub struct AIDB {
    site_name: String,
    site_url: Url,
    cookies: Option<String>,
}

impl AIDB {
    pub fn new() -> Self {
        Self {
            site_name: "AI DB".to_string(),
            site_url: Url::parse(URL).unwrap(),
            cookies: None,
        }
    }

    fn extract_article_content(&self, page: &str) -> Option<String> {
        let page = restore_lazy_images(page);
        let document = scraper::Html::parse_document(&page);
        let reader_selector = scraper::Selector::parse("#xui-read-html").ok()?;
        if let Some(element) = document.select(&reader_selector).next() {
            return non_empty_article_html(self.clean_content(&element.inner_html()));
        }

        let legacy_selector = scraper::Selector::parse(".post_content").ok()?;
        let element = document.select(&legacy_selector).next()?;
        non_empty_article_html(self.clean_content(&element.inner_html()))
    }

    async fn fetch_article_with_cookie(&self, initial_url: &str, cookies: &str) -> AppResult<FetchedArticle> {
        fetch_article_with_fallback(self, initial_url, |target_url| {
            let cookies = cookies.to_string();
            async move {
                let response = self.request(&target_url, &cookies).await?;
                let final_url = response.url().to_string();
                let html = response.text().await?;
                Ok(FetchedPage { final_url, html })
            }
        })
        .await
    }
}

/// 旧デザインの画像は遅延読み込みで，`src` が base64 の仮画像，実体が `data-src` にある．
/// `data-src` があり `src` が `data:` の `<img>` だけ，`src` を `data-src` の値に差し替える．
fn restore_lazy_images(html: &str) -> String {
    let img = regex::Regex::new(r"<img\b[^>]*>").unwrap();
    let data_src = regex::Regex::new(r#"\sdata-src="([^"]+)""#).unwrap();
    let placeholder_src = regex::Regex::new(r#"\ssrc="data:[^"]*""#).unwrap();
    img.replace_all(html, |caps: &regex::Captures| {
        let tag = &caps[0];
        match data_src.captures(tag) {
            Some(real) if placeholder_src.is_match(tag) => {
                let replacement = format!(" src=\"{}\"", &real[1]);
                placeholder_src.replace(tag, regex::NoExpand(&replacement)).into_owned()
            }
            _ => tag.to_string(),
        }
    })
    .into_owned()
}

fn non_empty_article_html(html: String) -> Option<String> {
    let document = scraper::Html::parse_fragment(&html);
    document.root_element().text().any(|text| !text.trim().is_empty()).then_some(html)
}

fn article_id_from_url(url: &str) -> Option<String> {
    let url = Url::parse(url).ok()?;
    if url.domain()? != "ai-data-base.com" {
        return None;
    }
    let segments = url.path().trim_matches('/').split('/').collect::<Vec<_>>();
    let id = match segments.as_slice() {
        ["archives", id] | ["timeline", "read", id] => *id,
        _ => return None,
    };
    (!id.is_empty() && id.chars().all(|character| character.is_ascii_digit())).then(|| id.to_string())
}

fn article_design_from_url(url: &str) -> Option<ArticleDesign> {
    let url = Url::parse(url).ok()?;
    match url.path().trim_matches('/').split('/').collect::<Vec<_>>().as_slice() {
        ["archives", _] => Some(ArticleDesign::Legacy),
        ["timeline", "read", _] => Some(ArticleDesign::Timeline),
        _ => None,
    }
}

fn other_design_url(article_id: &str, current_design: ArticleDesign) -> String {
    match current_design {
        ArticleDesign::Legacy => format!("https://ai-data-base.com/timeline/read/{article_id}"),
        ArticleDesign::Timeline => format!("https://ai-data-base.com/archives/{article_id}?ui=once"),
    }
}

async fn fetch_article_with_fallback<F, Fut>(site: &AIDB, initial_url: &str, mut fetch: F) -> AppResult<FetchedArticle>
where
    F: FnMut(String) -> Fut + Send,
    Fut: Future<Output = AppResult<FetchedPage>> + Send,
{
    let first_page = fetch(initial_url.to_string()).await?;
    if let Some(article_html) = site.extract_article_content(&first_page.html) {
        return Ok(FetchedArticle {
            page_html: first_page.html,
            article_html,
        });
    }

    let article_id = article_id_from_url(&first_page.final_url)
        .or_else(|| article_id_from_url(initial_url))
        .ok_or_else(|| AppError::ScrapeError("Failed to parse article text".into()))?;
    let current_design = article_design_from_url(&first_page.final_url)
        .or_else(|| article_design_from_url(initial_url))
        .unwrap_or(ArticleDesign::Legacy);
    let fallback_page = fetch(other_design_url(&article_id, current_design)).await?;
    let article_html = site
        .extract_article_content(&fallback_page.html)
        .ok_or_else(|| AppError::ScrapeError("Failed to parse article text".into()))?;
    Ok(FetchedArticle {
        page_html: fallback_page.html,
        article_html,
    })
}

pub fn membership_cookie_expiry(cookies: &str) -> Option<i64> {
    let encoded_value = cookies.split(';').find_map(|part| {
        let (name, value) = part.trim().split_once('=')?;
        name.starts_with("simple_wp_membership_sec_").then_some(value)
    })?;
    let encoded_pair = format!("value={encoded_value}");
    let decoded_value =
        url::form_urlencoded::parse(encoded_pair.as_bytes()).find_map(|(name, value)| (name == "value").then(|| value.into_owned()))?;
    decoded_value.split('|').nth(1)?.parse().ok()
}

fn filter_and_join_cookies(cookies: &[BrowserCookie]) -> String {
    cookies
        .iter()
        .filter(|cookie| {
            cookie
                .domain
                .as_deref()
                .is_some_and(|domain| domain.trim_start_matches('.') == "ai-data-base.com")
        })
        .filter(|cookie| {
            !["_ga", "__stripe", "comment_author_"]
                .iter()
                .any(|prefix| cookie.name.starts_with(prefix))
        })
        .map(|cookie| format!("{}={}", cookie.name, cookie.value))
        .collect::<Vec<_>>()
        .join("; ")
}

fn has_required_auth_cookies(cookies: &[BrowserCookie]) -> bool {
    let has_membership = cookies.iter().any(|cookie| cookie.name.starts_with("simple_wp_membership_sec_"));
    let has_wordpress = cookies.iter().any(|cookie| cookie.name.starts_with("wordpress_logged_in_"));
    has_membership && has_wordpress
}

fn is_logged_out(html: &str) -> bool {
    html.contains(LOGGED_OUT_MARKER)
}

fn login_attempt_allowed(now: i64, last_attempt_at: i64) -> bool {
    now.saturating_sub(last_attempt_at) >= LOGIN_INTERVAL_SECONDS
}

fn auto_login_state_path() -> AppResult<PathBuf> {
    dirs::cache_dir()
        .map(|path| path.join("news_clipper").join("aidb_auto_login.json"))
        .ok_or_else(|| AppError::InternalError("cache directory is unavailable".into()))
}

fn read_auto_login_state(path: &Path) -> AppResult<Option<AutoLoginState>> {
    match std::fs::read(path) {
        Ok(contents) => Ok(Some(serde_json::from_slice(&contents)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(AppError::InternalError("failed to read AIDB auto-login state".into())),
    }
}

fn write_auto_login_state(path: &Path, state: &AutoLoginState) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::InternalError("invalid AIDB auto-login state path".into()))?;
    std::fs::create_dir_all(parent).map_err(|_| AppError::InternalError("failed to create AIDB auto-login state directory".into()))?;
    let temporary_path = path.with_extension(format!("tmp-{}", std::process::id()));
    let contents = serde_json::to_vec(state)?;
    std::fs::write(&temporary_path, contents).map_err(|_| AppError::InternalError("failed to write temporary AIDB auto-login state".into()))?;
    std::fs::rename(&temporary_path, path).map_err(|_| AppError::InternalError("failed to replace AIDB auto-login state".into()))?;
    Ok(())
}

fn browser_error(message: &str) -> AppError {
    AppError::InternalError(message.to_string())
}

async fn wait_for_recaptcha(driver: &WebDriver) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(element) = driver.find(By::Css(".g-recaptcha-response")).await
            && let Ok(Some(value)) = element.attr("value").await
            && !value.is_empty()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(browser_error("AIDB reCAPTCHA token timed out"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_auth_cookies(driver: &WebDriver) -> AppResult<Vec<BrowserCookie>> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(cookies) = driver.get_all_cookies().await
            && has_required_auth_cookies(&cookies)
        {
            return Ok(cookies);
        }
        if Instant::now() >= deadline {
            return Err(browser_error("AIDB authentication cookies timed out"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn perform_browser_login(user: &str, password: &str) -> AppResult<Cookie> {
    let profile_dir = dirs::data_dir()
        .ok_or_else(|| AppError::InternalError("data directory is unavailable".into()))?
        .join("news_clipper")
        .join("chrome-profile-aidb");
    std::fs::create_dir_all(&profile_dir).map_err(|_| browser_error("failed to create AIDB Chrome profile directory"))?;
    let headless = std::env::var("AIDB_LOGIN_HEADFUL").as_deref() != Ok("1");
    let managed = ManagedChromeDriver::launch_with(LaunchOptions {
        headless,
        user_data_dir: Some(profile_dir),
    })
    .await
    .map_err(|_| browser_error("failed to launch Chrome for AIDB auto-login"))?;

    let driver = managed.driver();
    let result = async {
        driver
            .goto(LOGIN_URL)
            .await
            .map_err(|_| browser_error("failed to open the AIDB login page"))?;

        let form_present = driver
            .query(By::Id("swpm_user_name"))
            .nowait()
            .exists()
            .await
            .map_err(|_| browser_error("failed to inspect the AIDB login page"))?;
        if form_present {
            wait_for_recaptcha(driver).await?;
            let user_field = driver
                .find(By::Id("swpm_user_name"))
                .await
                .map_err(|_| browser_error("AIDB user field is unavailable"))?;
            user_field
                .clear()
                .await
                .map_err(|_| browser_error("failed to clear the AIDB user field"))?;
            user_field
                .send_keys(user)
                .await
                .map_err(|_| browser_error("failed to fill the AIDB user field"))?;

            let password_field = driver
                .find(By::Id("swpm_password"))
                .await
                .map_err(|_| browser_error("AIDB password field is unavailable"))?;
            password_field
                .clear()
                .await
                .map_err(|_| browser_error("failed to clear the AIDB password field"))?;
            password_field
                .send_keys(password)
                .await
                .map_err(|_| browser_error("failed to fill the AIDB password field"))?;

            let remember = driver
                .find(By::Id("swpm-rememberme"))
                .await
                .map_err(|_| browser_error("AIDB remember-me field is unavailable"))?;
            if !remember
                .is_selected()
                .await
                .map_err(|_| browser_error("failed to inspect AIDB remember-me field"))?
            {
                remember
                    .click()
                    .await
                    .map_err(|_| browser_error("failed to select AIDB remember-me field"))?;
            }

            let submit = driver
                .find(By::Css("input[name='swpm-login']"))
                .await
                .map_err(|_| browser_error("AIDB login button is unavailable"))?;
            // headless の小さな画面ではボタンが画面外や他要素の下にあり，通常のクリックが弾かれる．
            // そのときは JS でクリックする（`form.submit()` だと `swpm-login` が送られない）．
            if let Err(error) = submit.click().await {
                tracing::debug!("AIDB: native click on the login button failed ({error}); retrying via JS");
                let element = submit.to_json().map_err(|_| browser_error("failed to submit the AIDB login form"))?;
                driver
                    .execute("arguments[0].click();", vec![element])
                    .await
                    .map_err(|_| browser_error("failed to submit the AIDB login form"))?;
            }
        }

        let cookies = wait_for_auth_cookies(driver).await?;
        let cookie_header = filter_and_join_cookies(&cookies);
        if cookie_header.is_empty() {
            return Err(browser_error("AIDB returned no usable cookies"));
        }
        keychain::write(KEYCHAIN_SERVICE, COOKIE_ACCOUNT, &cookie_header)?;
        Ok(cookie_header)
    }
    .await;

    if managed.close().await.is_err() {
        tracing::warn!("AIDB: failed to close Chrome after auto-login");
    }
    result
}

pub async fn auto_login(force: bool) -> AppResult<Cookie> {
    let user = keychain::read(KEYCHAIN_SERVICE, USER_ACCOUNT)?
        .ok_or_else(|| AppError::InternalError("AIDB user is not configured in macOS Keychain".into()))?;
    let password = keychain::read(KEYCHAIN_SERVICE, PASSWORD_ACCOUNT)?
        .ok_or_else(|| AppError::InternalError("AIDB password is not configured in macOS Keychain".into()))?;
    let state_path = auto_login_state_path()?;
    let now = Utc::now().timestamp();
    let force = force || std::env::var("AIDB_FORCE_LOGIN").as_deref() == Ok("1");
    if !force
        && let Some(state) = read_auto_login_state(&state_path)?
        && !login_attempt_allowed(now, state.last_attempt_at)
    {
        return Err(AppError::InternalError("AIDB auto-login is limited to one attempt per 20 hours".into()));
    }

    write_auto_login_state(
        &state_path,
        &AutoLoginState {
            last_attempt_at: now,
            last_result: "in_progress".into(),
        },
    )?;
    let result = perform_browser_login(&user, &password).await;
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
        tracing::warn!("AIDB: failed to update auto-login state");
    }
    result
}

/// 1 回の実行で通知は 1 度だけ出す（記事ごとに出すと日次の実行で何通も届く）．
#[cfg(target_os = "macos")]
fn notify_cookie_expired() {
    static NOTIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if NOTIFIED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let _ = std::process::Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "display notification \"AIDB: cookie expired, auto-login failed\" with title \"news-clipper\"",
        ])
        .status();
}

#[cfg(not(target_os = "macos"))]
fn notify_cookie_expired() {}

impl Default for AIDB {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebSiteInterface for AIDB {
    fn site_name(&self) -> String {
        self.site_name.clone()
    }
    fn site_url(&self) -> Url {
        self.site_url.clone()
    }
    fn domain(&self) -> String {
        self.site_url.domain().unwrap().to_string()
    }
    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![
            ".aidb-tts-player",
            ".aidb-tips-support",
            ".ajm-article-jobs",
            ".aidb-upsell-content",
            ".premium-section-label",
        ]
    }
    async fn login(&mut self) -> AppResult<Cookie> {
        let existing = keychain::read(KEYCHAIN_SERVICE, COOKIE_ACCOUNT)?.or_else(|| self.cookies.clone());
        let needs_refresh = existing
            .as_deref()
            .and_then(membership_cookie_expiry)
            .is_none_or(|expiry| expiry - Utc::now().timestamp() < REFRESH_BEFORE_SECONDS);

        if needs_refresh {
            match auto_login(false).await {
                Ok(cookies) => {
                    self.cookies = Some(cookies.clone());
                    return Ok(cookies);
                }
                Err(error) if existing.is_none() => return Err(error),
                Err(_) => {}
            }
        }

        let cookies = existing.ok_or_else(|| AppError::InternalError("AIDB cookie is not configured in macOS Keychain".into()))?;
        self.cookies = Some(cookies.clone());
        Ok(cookies)
    }
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>> {
        let cookies = self.login().await?;
        let response = self.request(self.site_url.as_str(), &cookies).await?;
        let feeds = match parsers::rss2::parse(response.text().await?.as_str()) {
            Ok(feeds) => feeds,
            Err(e) => return Err(AppError::RssParseError(e)),
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
        let mut cookies = self.login().await?;
        let mut article = self.fetch_article_with_cookie(url, &cookies).await?;

        if is_logged_out(&article.page_html)
            && let Ok(refreshed) = auto_login(false).await
        {
            cookies = refreshed;
            self.cookies = Some(cookies.clone());
            article = self.fetch_article_with_cookie(url, &cookies).await?;
        }

        if is_logged_out(&article.page_html) {
            tracing::warn!("AIDB: cookie expired, auto-login failed");
            notify_cookie_expired();
        }

        let html = article.article_html;
        let text = html2md::rewrite_html(&html, false);
        Ok((self.trim_text(&html), self.trim_text(&text)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use thirtyfour::Cookie as BrowserCookie;

    #[test]
    fn parses_url_encoded_membership_cookie_expiry() {
        let cookies = concat!(
            "other=value; ",
            "simple_wp_membership_sec_hash=member%7C1794553581%7Csignature%7C1; ",
            "wordpress_logged_in_hash=value"
        );

        assert_eq!(membership_cookie_expiry(cookies), Some(1_794_553_581));
    }

    #[test]
    fn expiry_is_none_without_membership_cookie() {
        assert_eq!(membership_cookie_expiry("wordpress_logged_in_hash=value"), None);
    }

    #[test]
    fn filters_and_joins_browser_cookies() {
        let cookie = |name: &str, value: &str, domain: &str| {
            let mut cookie = BrowserCookie::new(name, value);
            cookie.set_domain(domain);
            cookie
        };
        let cookies = vec![
            cookie("simple_wp_membership_sec_hash", "member", ".ai-data-base.com"),
            cookie("wordpress_logged_in_hash", "wordpress", "ai-data-base.com"),
            cookie("_ga_123", "analytics", ".ai-data-base.com"),
            cookie("__stripe_mid", "stripe", ".ai-data-base.com"),
            cookie("comment_author_name", "author", ".ai-data-base.com"),
            cookie("foreign", "secret", ".example.com"),
        ];

        assert_eq!(
            filter_and_join_cookies(&cookies),
            "simple_wp_membership_sec_hash=member; wordpress_logged_in_hash=wordpress"
        );
    }

    #[test]
    fn detects_logged_out_article() {
        assert!(is_logged_out("<p>この続きはプレミアム会員限定コンテンツです</p>"));
        assert!(!is_logged_out("<p>合成した公開本文</p>"));
    }

    #[test]
    fn extracts_reader_content_and_removes_aidb_components() {
        let page = r#"
            <div class="post_content"><p>短いプレビュー</p></div>
            <div id="xui-read-html" class="xui-article" hidden>
                <p>合成した会員本文</p>
                <div class="aidb-tts-player">音声</div>
                <div class="aidb-tips-support">投げ銭</div>
                <div class="ajm-article-jobs">求人</div>
                <div class="aidb-upsell-content">案内</div>
                <div class="premium-section-label">限定ラベル</div>
                <div class="aidb-sl-embed-card">Related Agent Skill</div>
                <div class="aidb-rec-article-embed-card">関連記事</div>
            </div>
        "#;

        let html = AIDB::new().extract_article_content(page).unwrap();

        assert!(html.contains("合成した会員本文"));
        assert!(html.contains("Related Agent Skill"));
        assert!(html.contains("関連記事"));
        assert!(!html.contains("短いプレビュー"));
        assert!(!html.contains("音声"));
        assert!(!html.contains("投げ銭"));
        assert!(!html.contains("求人"));
        assert!(!html.contains("案内"));
        assert!(!html.contains("限定ラベル"));
    }

    #[test]
    fn restores_lazy_loaded_image_sources() {
        let html = r#"<p><img src="data:image/png;base64,AAAA" alt="a" data-src="https://example.com/a.png"></p><img src="https://example.com/b.png" data-src="https://example.com/other.png"><img src="data:image/gif;base64,BBBB">"#;
        let restored = restore_lazy_images(html);
        assert!(restored.contains(r#"src="https://example.com/a.png""#));
        assert!(!restored.contains("base64,AAAA"));
        // 実体の src を持つ画像と，data-src の無い仮画像は触らない
        assert!(restored.contains(r#"src="https://example.com/b.png""#));
        assert!(restored.contains("base64,BBBB"));
    }

    #[test]
    fn falls_back_to_post_content() {
        let page = r#"
            <main>
                <section class="post_content">
                    <p>合成した旧デザインの全文</p>
                    <div class="aidb-tts-player">音声</div>
                    <div class="aidb-tips-support">投げ銭</div>
                    <div class="ajm-article-jobs">求人</div>
                    <div class="aidb-upsell-content">案内</div>
                    <div class="premium-section-label">限定ラベル</div>
                </section>
            </main>
        "#;

        let html = AIDB::new().extract_article_content(page).unwrap();

        assert!(html.contains("合成した旧デザインの全文"));
        assert!(!html.contains("音声"));
        assert!(!html.contains("投げ銭"));
        assert!(!html.contains("求人"));
        assert!(!html.contains("案内"));
        assert!(!html.contains("限定ラベル"));
    }

    #[test]
    fn extracts_article_id_from_both_design_urls() {
        for url in [
            "https://ai-data-base.com/archives/139789",
            "https://ai-data-base.com/archives/139789?ui=once",
            "https://ai-data-base.com/timeline/read/139789",
            "https://ai-data-base.com/timeline/read/139789/",
        ] {
            assert_eq!(article_id_from_url(url).as_deref(), Some("139789"));
        }

        assert_eq!(article_id_from_url("https://ai-data-base.com/archives/"), None);
        assert_eq!(article_id_from_url("https://ai-data-base.com/timeline/read/"), None);
        assert_eq!(article_id_from_url("https://ai-data-base.com/about"), None);
    }

    #[test]
    fn builds_the_other_design_url_in_both_directions() {
        assert_eq!(
            other_design_url("139789", ArticleDesign::Timeline),
            "https://ai-data-base.com/archives/139789?ui=once"
        );
        assert_eq!(
            other_design_url("139789", ArticleDesign::Legacy),
            "https://ai-data-base.com/timeline/read/139789"
        );
    }

    #[tokio::test]
    async fn falls_back_from_timeline_to_legacy_when_extraction_fails() {
        let site = AIDB::new();
        let mut requested_urls = Vec::new();
        let result = fetch_article_with_fallback(&site, "https://ai-data-base.com/archives/139789", |url| {
            requested_urls.push(url.clone());
            std::future::ready(Ok(if url.contains("ui=once") {
                FetchedPage {
                    final_url: url,
                    html: "<section class='post_content'><p>旧デザインの合成本文</p></section>".into(),
                }
            } else {
                FetchedPage {
                    final_url: "https://ai-data-base.com/timeline/read/139789".into(),
                    html: "<main>本文要素なし</main>".into(),
                }
            }))
        })
        .await
        .unwrap();

        assert!(result.article_html.contains("旧デザインの合成本文"));
        assert_eq!(
            requested_urls,
            vec![
                "https://ai-data-base.com/archives/139789",
                "https://ai-data-base.com/archives/139789?ui=once",
            ]
        );
    }

    #[tokio::test]
    async fn succeeds_without_fallback_when_first_design_has_content() {
        let site = AIDB::new();
        let mut request_count = 0;
        let result = fetch_article_with_fallback(&site, "https://ai-data-base.com/archives/139789?ui=once", |url| {
            request_count += 1;
            std::future::ready(Ok(FetchedPage {
                final_url: url,
                html: "<section class='post_content'><p>最初の合成本文</p></section>".into(),
            }))
        })
        .await
        .unwrap();

        assert!(result.article_html.contains("最初の合成本文"));
        assert_eq!(request_count, 1);
    }

    #[tokio::test]
    async fn returns_error_after_both_designs_fail_extraction() {
        let site = AIDB::new();
        let mut request_count = 0;
        let result = fetch_article_with_fallback(&site, "https://ai-data-base.com/timeline/read/139789", |url| {
            request_count += 1;
            std::future::ready(Ok(FetchedPage {
                final_url: url,
                html: "<section class='post_content'>   </section>".into(),
            }))
        })
        .await;

        assert!(matches!(result, Err(AppError::ScrapeError(_))));
        assert_eq!(request_count, 2);
    }

    #[test]
    fn enforces_twenty_hour_attempt_interval() {
        const TWENTY_HOURS: i64 = 20 * 60 * 60;
        let last_attempt_at = 1_000_000;

        assert!(!login_attempt_allowed(last_attempt_at + TWENTY_HOURS - 1, last_attempt_at));
        assert!(login_attempt_allowed(last_attempt_at + TWENTY_HOURS, last_attempt_at));
    }
}
