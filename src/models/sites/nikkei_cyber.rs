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
use thirtyfour::{By, Cookie as BrowserCookie, WebDriver, WebElement};

const URL: &str = "https://www.nikkei.com/theme/?dw=21071204";
const LOGIN_URL: &str = "https://www.nikkei.com/login";
const PROBE_ARTICLE_URL: &str = "https://www.nikkei.com/article/DGXZQOUC0235C0S6A101C2000000/";
const KEYCHAIN_SERVICE: &str = "news-clipper-nikkei";
const COOKIE_ACCOUNT: &str = "cookie";
const USER_ACCOUNT: &str = "user";
const PASSWORD_ACCOUNT: &str = "password";
const LOGIN_INTERVAL_SECONDS: i64 = 20 * 60 * 60;
const LOGGED_OUT_MARKERS: &[&str] = &["有料会員に登録する", "有料会員が初回1カ月無料"];
const EMAIL_SELECTOR: &str = r#"input[data-testid="email"]"#;
const PASSWORD_SELECTOR: &str = r#"input[type="password"]"#;
const PRIMARY_SUBMIT_SELECTOR: &str = r#"button[data-testid="submit"]"#;
const FALLBACK_SUBMIT_SELECTOR: &str = r#"button[type="submit"]"#;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryStep {
    RateLimited,
    RefreshProfile,
    CredentialLogin,
    Complete,
    NotifyFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NotificationKind {
    LoginFailed,
    AdditionalVerification,
}

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

fn cookie_applies_to_host(cookie: &BrowserCookie, host: &str) -> bool {
    cookie.domain.as_deref().is_some_and(|domain| {
        let domain = domain.trim_start_matches('.');
        host == domain || host.strip_suffix(domain).is_some_and(|prefix| prefix.ends_with('.'))
    })
}

fn has_nikkei_auth_cookie(cookies: &[BrowserCookie]) -> bool {
    cookies
        .iter()
        .any(|cookie| cookie.name == "RNikkeiAuth" && cookie_applies_to_host(cookie, "www.nikkei.com"))
}

fn is_existing_nikkei_session(current_url: &Url, cookies: &[BrowserCookie]) -> bool {
    current_url.host_str().is_some_and(|host| host != "id.nikkei.com") && has_nikkei_auth_cookie(cookies)
}

fn safe_url_location(url: &Url) -> String {
    format!("{}{}", url.host_str().unwrap_or("<unknown-host>"), url.path())
}

fn login_success_timeout_message(url: &Url) -> String {
    format!("Nikkei auto-login timed out at {}", safe_url_location(url))
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

struct TemporaryProfileDir {
    path: PathBuf,
}

impl TemporaryProfileDir {
    fn create() -> AppResult<Self> {
        let path = std::env::temp_dir().join(format!("news-clipper-nikkei-profile-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).map_err(|_| browser_error("failed to create temporary Nikkei Chrome profile directory"))?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TemporaryProfileDir {
    fn drop(&mut self) {
        if std::fs::remove_dir_all(&self.path).is_err() {
            tracing::warn!("Nikkei: failed to remove temporary Chrome profile directory");
        }
    }
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

fn select_recovery_step(attempt_allowed: bool, profile_succeeded: Option<bool>, has_credentials: bool) -> RecoveryStep {
    if !attempt_allowed {
        return RecoveryStep::RateLimited;
    }
    match profile_succeeded {
        None => RecoveryStep::RefreshProfile,
        Some(true) => RecoveryStep::Complete,
        Some(false) if has_credentials => RecoveryStep::CredentialLogin,
        Some(false) => RecoveryStep::NotifyFailure,
    }
}

fn has_additional_verification(html: &str) -> bool {
    let document = scraper::Html::parse_document(html);
    let challenge_selector = Selector::parse(r#"iframe[src*="recaptcha"][title*="challenge"]"#).unwrap();
    let one_time_code_selector = Selector::parse(r#"input[autocomplete="one-time-code"]"#).unwrap();
    document.select(&challenge_selector).next().is_some()
        || document.select(&one_time_code_selector).next().is_some()
        || rendered_text_requests_verification_code(&document)
}

fn rendered_text_requests_verification_code(document: &scraper::Html) -> bool {
    const VERIFICATION_CODE_TERMS: [&str; 4] = ["ワンタイムパスワード", "ワンタイムコード", "認証コード", "確認コード"];

    document.tree.nodes().any(|node| {
        let Some(text) = node.value().as_text() else {
            return false;
        };
        let belongs_to_non_rendered_element = node.ancestors().any(|ancestor| {
            ancestor
                .value()
                .as_element()
                .is_some_and(|element| matches!(element.name(), "script" | "style"))
        });
        !belongs_to_non_rendered_element && text.contains("入力") && VERIFICATION_CODE_TERMS.iter().any(|term| text.contains(term))
    })
}

fn notification_message(kind: NotificationKind) -> &'static str {
    match kind {
        NotificationKind::LoginFailed => "日経のログインが切れています．`cargo run --example nikkei_login` を実行してください",
        NotificationKind::AdditionalVerification => {
            "日経の自動ログインで追加の確認を求められました．`cargo run --example nikkei_login` で手動ログインしてください"
        }
    }
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

async fn first_interactable(driver: &WebDriver, selector: &'static str) -> AppResult<Option<WebElement>> {
    let elements = driver
        .find_all(By::Css(selector))
        .await
        .map_err(|_| browser_error("failed to inspect the Nikkei login form"))?;
    for element in elements {
        if element.is_displayed().await.unwrap_or(false) && element.is_enabled().await.unwrap_or(false) {
            return Ok(Some(element));
        }
    }
    Ok(None)
}

async fn fill_login_field(element: &WebElement, value: &str, field_name: &str) -> AppResult<()> {
    element
        .clear()
        .await
        .map_err(|_| browser_error(&format!("failed to clear the Nikkei {field_name} field")))?;
    element
        .send_keys(value)
        .await
        .map_err(|_| browser_error(&format!("failed to fill the Nikkei {field_name} field")))
}

async fn click_login_submit(driver: &WebDriver, allow_fallback: bool) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(button) = first_interactable(driver, PRIMARY_SUBMIT_SELECTOR).await? {
            return button.click().await.map_err(|_| browser_error("failed to submit the Nikkei login form"));
        }
        if allow_fallback && let Some(button) = first_interactable(driver, FALLBACK_SUBMIT_SELECTOR).await? {
            return button.click().await.map_err(|_| browser_error("failed to submit the Nikkei login form"));
        }
        if Instant::now() >= deadline {
            return Err(browser_error("Nikkei login button is unavailable"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

#[derive(Debug)]
enum BrowserLoginResult {
    Success(Cookie),
    AdditionalVerification,
}

enum LoginEntryResult {
    EmailField(WebElement),
    ExistingSession(Cookie),
    AdditionalVerification,
}

#[derive(Debug)]
struct AutoRecoveryFailure {
    error: AppError,
    notification_kind: NotificationKind,
}

async fn inspect_additional_verification(driver: &WebDriver) -> AppResult<bool> {
    let html = driver
        .source()
        .await
        .map_err(|_| browser_error("failed to inspect the Nikkei login page"))?;
    Ok(has_additional_verification(&html))
}

async fn verify_authenticated_session(driver: &WebDriver) -> AppResult<Option<Cookie>> {
    let cookies = driver
        .get_all_cookies()
        .await
        .map_err(|_| browser_error("failed to inspect Nikkei authentication cookies"))?;
    if !has_nikkei_auth_cookie(&cookies) {
        return Ok(None);
    }

    driver
        .goto(PROBE_ARTICLE_URL)
        .await
        .map_err(|_| browser_error("failed to open the Nikkei probe article after login"))?;
    let html = driver
        .source()
        .await
        .map_err(|_| browser_error("failed to inspect the Nikkei probe article after login"))?;
    let cookies = driver
        .get_all_cookies()
        .await
        .map_err(|_| browser_error("failed to inspect Nikkei authentication cookies"))?;
    if has_nikkei_auth_cookie(&cookies) && !is_logged_out(&html) {
        return save_browser_cookies(driver).await.map(Some);
    }
    Ok(None)
}

async fn wait_for_login_entry(driver: &WebDriver) -> AppResult<LoginEntryResult> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let current_url = driver
            .current_url()
            .await
            .map_err(|_| browser_error("failed to inspect the current Nikkei login URL"))?;
        let current_location = safe_url_location(&current_url);
        let cookies = driver
            .get_all_cookies()
            .await
            .map_err(|_| browser_error("failed to inspect Nikkei login cookies"))?;

        if is_existing_nikkei_session(&current_url, &cookies) {
            return match verify_authenticated_session(driver).await? {
                Some(cookie) => Ok(LoginEntryResult::ExistingSession(cookie)),
                None => Err(browser_error(&format!("Nikkei email field is unavailable at {current_location}"))),
            };
        }
        if inspect_additional_verification(driver).await? {
            return Ok(LoginEntryResult::AdditionalVerification);
        }
        if let Some(email) = first_interactable(driver, EMAIL_SELECTOR).await? {
            return Ok(LoginEntryResult::EmailField(email));
        }
        if Instant::now() >= deadline {
            return Err(browser_error(&format!("Nikkei email field is unavailable at {current_location}")));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_password_or_verification(driver: &WebDriver) -> AppResult<Option<WebElement>> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if inspect_additional_verification(driver).await? {
            return Ok(None);
        }
        if let Some(password) = first_interactable(driver, PASSWORD_SELECTOR).await? {
            return Ok(Some(password));
        }
        if Instant::now() >= deadline {
            return Err(browser_error("Nikkei password field is unavailable"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn wait_for_login_success(driver: &WebDriver) -> AppResult<BrowserLoginResult> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(cookies) = driver.get_all_cookies().await
            && has_nikkei_auth_cookie(&cookies)
            && let Some(cookie) = verify_authenticated_session(driver).await?
        {
            return Ok(BrowserLoginResult::Success(cookie));
        }

        if inspect_additional_verification(driver).await? {
            return Ok(BrowserLoginResult::AdditionalVerification);
        }
        if Instant::now() >= deadline {
            let current_url = driver
                .current_url()
                .await
                .map_err(|_| browser_error("failed to inspect the current Nikkei login URL after auto-login timed out"))?;
            return Err(browser_error(&login_success_timeout_message(&current_url)));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn perform_browser_login(user: &str, password: &str, headless: bool, profile_dir: &Path) -> AppResult<BrowserLoginResult> {
    std::fs::create_dir_all(profile_dir).map_err(|_| browser_error("failed to create Nikkei Chrome profile directory"))?;
    let managed = ManagedChromeDriver::launch_without_automation_evasion(LaunchOptions {
        headless,
        user_data_dir: Some(profile_dir.to_path_buf()),
    })
    .await
    .map_err(|_| browser_error("failed to launch Chrome for Nikkei auto-login"))?;
    let driver = managed.driver();
    let result = async {
        driver
            .goto(LOGIN_URL)
            .await
            .map_err(|_| browser_error("failed to open the Nikkei login page"))?;

        let email = match wait_for_login_entry(driver).await? {
            LoginEntryResult::EmailField(email) => email,
            LoginEntryResult::ExistingSession(cookie) => return Ok(BrowserLoginResult::Success(cookie)),
            LoginEntryResult::AdditionalVerification => return Ok(BrowserLoginResult::AdditionalVerification),
        };
        fill_login_field(&email, user, "email").await?;

        if inspect_additional_verification(driver).await? {
            return Ok(BrowserLoginResult::AdditionalVerification);
        }
        let visible_password = first_interactable(driver, PASSWORD_SELECTOR).await?;
        if let Some(password_field) = visible_password.as_ref() {
            fill_login_field(password_field, password, "password").await?;
            if inspect_additional_verification(driver).await? {
                return Ok(BrowserLoginResult::AdditionalVerification);
            }
        }
        click_login_submit(driver, false).await?;

        if visible_password.is_none() {
            let Some(password_field) = wait_for_password_or_verification(driver).await? else {
                return Ok(BrowserLoginResult::AdditionalVerification);
            };
            fill_login_field(&password_field, password, "password").await?;
            if inspect_additional_verification(driver).await? {
                return Ok(BrowserLoginResult::AdditionalVerification);
            }
            click_login_submit(driver, true).await?;
        }

        wait_for_login_success(driver).await
    }
    .await;
    if managed.close().await.is_err() {
        tracing::warn!("Nikkei: failed to close Chrome after auto-login");
    }
    result
}

async fn refresh_cookie_from_profile() -> AppResult<Cookie> {
    let profile_dir = profile_dir()?;
    std::fs::create_dir_all(&profile_dir).map_err(|_| browser_error("failed to create Nikkei Chrome profile directory"))?;
    let headless = std::env::var("NIKKEI_LOGIN_HEADFUL").as_deref() != Ok("1");
    let managed = ManagedChromeDriver::launch_without_automation_evasion(LaunchOptions {
        headless,
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
        let cookies = driver
            .get_all_cookies()
            .await
            .map_err(|_| browser_error("failed to inspect Nikkei authentication cookies"))?;
        if is_logged_out(&html) || !has_nikkei_auth_cookie(&cookies) {
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

fn read_login_credentials() -> AppResult<Option<(String, String)>> {
    let Some(user) = keychain::read(KEYCHAIN_SERVICE, USER_ACCOUNT)?.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(password) = keychain::read(KEYCHAIN_SERVICE, PASSWORD_ACCOUNT)?.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    Ok(Some((user, password)))
}

fn update_auto_login_state(path: &Path, last_attempt_at: i64, last_result: &str) {
    if write_auto_login_state(
        path,
        &AutoLoginState {
            last_attempt_at,
            last_result: last_result.into(),
        },
    )
    .is_err()
    {
        tracing::warn!("Nikkei: failed to update auto-login state");
    }
}

async fn auto_refresh_cookie(force: bool) -> Result<Cookie, AutoRecoveryFailure> {
    let state_path = auto_login_state_path().map_err(|error| AutoRecoveryFailure {
        error,
        notification_kind: NotificationKind::LoginFailed,
    })?;
    let now = Utc::now().timestamp();
    let force = force || std::env::var("NIKKEI_FORCE_LOGIN").as_deref() == Ok("1");
    let attempt_allowed = force
        || read_auto_login_state(&state_path)
            .map_err(|error| AutoRecoveryFailure {
                error,
                notification_kind: NotificationKind::LoginFailed,
            })?
            .is_none_or(|state| login_attempt_allowed(now, state.last_attempt_at));
    if select_recovery_step(attempt_allowed, None, false) == RecoveryStep::RateLimited {
        return Err(AutoRecoveryFailure {
            error: browser_error("Nikkei auto-login is limited to one attempt per 20 hours"),
            notification_kind: NotificationKind::LoginFailed,
        });
    }
    write_auto_login_state(
        &state_path,
        &AutoLoginState {
            last_attempt_at: now,
            last_result: "in_progress".into(),
        },
    )
    .map_err(|error| AutoRecoveryFailure {
        error,
        notification_kind: NotificationKind::LoginFailed,
    })?;

    let profile_error = match refresh_cookie_from_profile().await {
        Ok(cookie) => {
            update_auto_login_state(&state_path, now, "profile_success");
            return Ok(cookie);
        }
        Err(error) => error,
    };

    let credentials = read_login_credentials().map_err(|error| AutoRecoveryFailure {
        error,
        notification_kind: NotificationKind::LoginFailed,
    })?;
    let has_credentials = credentials.is_some();
    if select_recovery_step(true, Some(false), has_credentials) == RecoveryStep::NotifyFailure {
        update_auto_login_state(&state_path, now, "profile_failed_no_credentials");
        return Err(AutoRecoveryFailure {
            error: profile_error,
            notification_kind: NotificationKind::LoginFailed,
        });
    }

    let Some((user, password)) = credentials else {
        update_auto_login_state(&state_path, now, "profile_failed_no_credentials");
        return Err(AutoRecoveryFailure {
            error: profile_error,
            notification_kind: NotificationKind::LoginFailed,
        });
    };
    update_auto_login_state(&state_path, now, "profile_failed_trying_credentials");
    let headless = std::env::var("NIKKEI_LOGIN_HEADFUL").as_deref() != Ok("1");
    let credential_result = match profile_dir() {
        Ok(login_profile_dir) => perform_browser_login(&user, &password, headless, &login_profile_dir).await,
        Err(error) => Err(error),
    };
    match credential_result {
        Ok(BrowserLoginResult::Success(cookie)) => {
            update_auto_login_state(&state_path, now, "credentials_success");
            Ok(cookie)
        }
        Ok(BrowserLoginResult::AdditionalVerification) => {
            update_auto_login_state(&state_path, now, "additional_verification");
            Err(AutoRecoveryFailure {
                error: browser_error("Nikkei auto-login requires additional verification"),
                notification_kind: NotificationKind::AdditionalVerification,
            })
        }
        Err(error) => {
            update_auto_login_state(&state_path, now, "credentials_failed");
            Err(AutoRecoveryFailure {
                error,
                notification_kind: NotificationKind::LoginFailed,
            })
        }
    }
}

async fn auto_login_with_profile(profile_dir: &Path) -> AppResult<Cookie> {
    let (user, password) = read_login_credentials()?.ok_or_else(|| browser_error("Nikkei login credentials are not configured in macOS Keychain"))?;
    match perform_browser_login(&user, &password, false, profile_dir).await? {
        BrowserLoginResult::Success(cookie) => Ok(cookie),
        BrowserLoginResult::AdditionalVerification => Err(browser_error(
            "Nikkei auto-login requires additional verification; use the manual login example",
        )),
    }
}

pub async fn auto_login() -> AppResult<Cookie> {
    auto_login_with_profile(&profile_dir()?).await
}

pub async fn auto_login_with_fresh_profile() -> AppResult<Cookie> {
    let profile = TemporaryProfileDir::create()?;
    auto_login_with_profile(profile.path()).await
}

pub async fn manual_login() -> AppResult<Cookie> {
    let profile_dir = profile_dir()?;
    std::fs::create_dir_all(&profile_dir).map_err(|_| browser_error("failed to create Nikkei Chrome profile directory"))?;
    let managed = ManagedChromeDriver::launch_without_automation_evasion(LaunchOptions {
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
fn notify_cookie_expired(kind: NotificationKind) {
    static NOTIFIED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if NOTIFIED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let script = format!("display notification {:?} with title {:?}", notification_message(kind), "news-clipper");
    let _ = std::process::Command::new("/usr/bin/osascript").args(["-e", &script]).status();
}

#[cfg(not(target_os = "macos"))]
fn notify_cookie_expired(_kind: NotificationKind) {}

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
        let mut notification_kind = NotificationKind::LoginFailed;
        if is_logged_out(&raw) {
            match auto_refresh_cookie(false).await {
                Ok(refreshed) => {
                    cookies = refreshed;
                    self.cookies = Some(cookies.clone());
                    raw = self.request(url, &cookies).await?.text().await?;
                }
                Err(failure) => {
                    notification_kind = failure.notification_kind;
                    tracing::debug!(error = %failure.error, "Nikkei auto-login did not recover the session");
                }
            }
        }
        if is_logged_out(&raw) {
            tracing::warn!("Nikkei: cookie expired, automatic recovery failed");
            notify_cookie_expired(notification_kind);
        }
        self.extract_article(&raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scraper::Html as ScraperHtml;
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

    #[test]
    fn requires_nikkei_auth_cookie_on_nikkei_domain() {
        let cookie = |name: &str, domain: &str| {
            let mut cookie = BrowserCookie::new(name, "opaque-value");
            cookie.set_domain(domain);
            cookie
        };

        assert!(has_nikkei_auth_cookie(&[cookie("RNikkeiAuth", ".nikkei.com")]));
        assert!(has_nikkei_auth_cookie(&[cookie("RNikkeiAuth", "www.nikkei.com")]));
        assert!(!has_nikkei_auth_cookie(&[cookie("RNikkeiAuth", ".example.com")]));
        assert!(!has_nikkei_auth_cookie(&[cookie("session", ".nikkei.com")]));
    }

    #[test]
    fn recognizes_existing_session_after_leaving_nikkei_id() {
        let mut auth_cookie = BrowserCookie::new("RNikkeiAuth", "opaque-value");
        auth_cookie.set_domain(".nikkei.com");

        assert!(is_existing_nikkei_session(
            &Url::parse("https://www.nikkei.com/?n_cid=login").unwrap(),
            &[auth_cookie]
        ));
    }

    #[test]
    fn does_not_recognize_existing_session_while_still_on_nikkei_id() {
        let mut auth_cookie = BrowserCookie::new("RNikkeiAuth", "opaque-value");
        auth_cookie.set_domain(".nikkei.com");

        assert!(!is_existing_nikkei_session(
            &Url::parse("https://id.nikkei.com/login/?auth=secret-jwt").unwrap(),
            &[auth_cookie]
        ));
    }

    #[test]
    fn requires_auth_cookie_that_applies_to_www_nikkei() {
        let mut id_only_cookie = BrowserCookie::new("RNikkeiAuth", "opaque-value");
        id_only_cookie.set_domain("id.nikkei.com");

        assert!(!is_existing_nikkei_session(
            &Url::parse("https://www.nikkei.com/").unwrap(),
            &[id_only_cookie.clone()]
        ));
        assert!(!is_existing_nikkei_session(&Url::parse("https://www.nikkei.com/").unwrap(), &[]));
        assert!(!is_existing_nikkei_session(
            &Url::parse("file:///tmp/redirect.html").unwrap(),
            &[id_only_cookie]
        ));
    }

    #[test]
    fn formats_login_location_without_query_or_fragment() {
        let url = Url::parse("https://id.nikkei.com/login/next?auth=secret-jwt#step").unwrap();

        assert_eq!(safe_url_location(&url), "id.nikkei.com/login/next");
        assert!(!safe_url_location(&url).contains("secret-jwt"));
    }

    #[test]
    fn temporary_profile_directory_is_removed_on_drop() {
        let path;
        {
            let profile = TemporaryProfileDir::create().unwrap();
            path = profile.path().to_path_buf();
            assert!(path.is_dir());
        }
        assert!(!path.exists());
    }

    #[test]
    fn finds_step_one_login_controls_in_fixture() {
        let fixture = include_str!("../../../tests/fixtures/nikkei_login_step1.html");
        let document = ScraperHtml::parse_document(fixture);

        for selector in [EMAIL_SELECTOR, PASSWORD_SELECTOR, PRIMARY_SUBMIT_SELECTOR] {
            let selector = Selector::parse(selector).unwrap();
            assert!(document.select(&selector).next().is_some(), "missing selector: {selector:?}");
        }
    }

    #[test]
    fn detects_additional_verification_challenge_iframe() {
        let html = r#"<iframe src="https://www.google.com/recaptcha/api2/bframe" title="recaptcha challenge"></iframe>"#;
        assert!(has_additional_verification(html));
    }

    #[test]
    fn detects_additional_verification_one_time_code_field() {
        assert!(has_additional_verification(r#"<input autocomplete="one-time-code">"#));
    }

    #[test]
    fn detects_additional_verification_japanese_copy() {
        assert!(has_additional_verification("<p>ワンタイムパスワードを入力してください</p>"));
        assert!(has_additional_verification("<p>認証コードを入力してください</p>"));
        assert!(has_additional_verification("<p>確認コードを入力してください</p>"));
    }

    #[test]
    fn ignores_ordinary_password_login_page_as_additional_verification() {
        assert!(!has_additional_verification(
            r#"<form><input type="password"><button type="submit">ログイン</button></form>"#
        ));
        assert!(!has_additional_verification(
            r#"<script>const labels = ["ワンタイム", "認証コード"];</script>"#
        ));
        assert!(!has_additional_verification(
            "<p>メールで届くワンタイムパスワードを使ってログインできます</p>"
        ));
        assert!(!has_additional_verification("<p>認証コードについてのヘルプ</p>"));
    }

    #[test]
    fn ignores_step_one_fixture_as_additional_verification() {
        let fixture = include_str!("../../../tests/fixtures/nikkei_login_step1.html");

        assert!(!has_additional_verification(fixture));
    }

    #[test]
    fn formats_login_success_timeout_without_query() {
        let url = Url::parse("https://id.nikkei.com/login/next?auth=secret-jwt#step").unwrap();

        let message = login_success_timeout_message(&url);
        assert_eq!(message, "Nikkei auto-login timed out at id.nikkei.com/login/next");
        assert!(!message.contains("secret-jwt"));
    }

    #[test]
    fn selects_profile_refresh_as_first_recovery_step() {
        assert_eq!(select_recovery_step(true, None, true), RecoveryStep::RefreshProfile);
    }

    #[test]
    fn skips_credential_login_when_credentials_are_missing() {
        assert_eq!(select_recovery_step(true, Some(false), false), RecoveryStep::NotifyFailure);
    }

    #[test]
    fn skips_credential_login_when_profile_refresh_succeeds() {
        assert_eq!(select_recovery_step(true, Some(true), true), RecoveryStep::Complete);
    }

    #[test]
    fn selects_credential_login_after_profile_refresh_failure() {
        assert_eq!(select_recovery_step(true, Some(false), true), RecoveryStep::CredentialLogin);
    }

    #[test]
    fn stops_recovery_when_attempt_is_rate_limited() {
        assert_eq!(select_recovery_step(false, None, true), RecoveryStep::RateLimited);
    }

    #[test]
    fn enforces_twenty_hour_attempt_interval_boundary() {
        let last_attempt_at = 1_000_000;
        assert!(!login_attempt_allowed(last_attempt_at + LOGIN_INTERVAL_SECONDS - 1, last_attempt_at));
        assert!(login_attempt_allowed(last_attempt_at + LOGIN_INTERVAL_SECONDS, last_attempt_at));
    }

    #[test]
    fn selects_additional_verification_notification_copy() {
        assert_eq!(
            notification_message(NotificationKind::AdditionalVerification),
            "日経の自動ログインで追加の確認を求められました．`cargo run --example nikkei_login` で手動ログインしてください"
        );
        assert_eq!(
            notification_message(NotificationKind::LoginFailed),
            "日経のログインが切れています．`cargo run --example nikkei_login` を実行してください"
        );
    }
}
