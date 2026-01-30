use crate::shared::errors::{AppError, AppResult};
use chrono::{DateTime, Local};
use derive_new::new;
use regex::Regex;
use request::{Response, Url};
use scraper::Selector;
use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::sync::OnceLock;
use strum::{Display, EnumString};

pub type Html = String;
pub type Text = String;
pub type Cookie = String;

/// 除外対象のCSSセレクタ（広告，サイドバー，ナビゲーション等）
const EXCLUDE_SELECTORS: &[&str] = &[
    // ナビゲーション・ヘッダー・フッター
    "nav",
    "header",
    "footer",
    // サイドバー・補助コンテンツ
    "aside",
    "[role='navigation']",
    "[role='complementary']",
    "[role='banner']",
    "[role='contentinfo']",
    // 広告関連
    ".ad",
    ".ads",
    ".advertisement",
    ".advert",
    "[class*='ad-']",
    "[class*='ads-']",
    "[id*='ad-']",
    "[id*='ads-']",
    // ソーシャル・シェア
    ".social",
    ".social-share",
    ".share-buttons",
    ".sharing",
    // コメント
    ".comments",
    "#comments",
    ".comment-section",
    // 関連記事・おすすめ
    ".related",
    ".related-posts",
    ".recommended",
    ".suggestions",
    // スクリプト・スタイル
    "script",
    "style",
    "noscript",
    "iframe",
    // フォーム（購読フォームなど）
    "form",
    // 非表示要素
    "[hidden]",
    "[aria-hidden='true']",
    ".hidden",
    ".visually-hidden",
];

/// HTMLから除外対象の要素を削除する
pub fn clean_html(html: &str) -> String {
    clean_html_with_selectors(html, &[])
}

/// HTMLから除外対象の要素を削除する（サイト固有のセレクタを追加可能）
pub fn clean_html_with_selectors(html: &str, additional_selectors: &[&str]) -> String {
    let doc = scraper::Html::parse_document(html);
    let mut excluded_fragments: Vec<String> = Vec::new();

    // 共通の除外セレクタを処理
    for selector_str in EXCLUDE_SELECTORS {
        if let Ok(selector) = Selector::parse(selector_str) {
            for elem in doc.select(&selector) {
                let fragment = elem.html();
                if !excluded_fragments.contains(&fragment) {
                    excluded_fragments.push(fragment);
                }
            }
        }
    }

    // サイト固有の除外セレクタを処理
    for selector_str in additional_selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            for elem in doc.select(&selector) {
                let fragment = elem.html();
                if !excluded_fragments.contains(&fragment) {
                    excluded_fragments.push(fragment);
                }
            }
        }
    }

    // 長い順にソート（ネストした要素を先に除去）
    excluded_fragments.sort_by(|a, b| b.len().cmp(&a.len()));

    // 除外対象のHTMLフラグメントを削除
    let mut cleaned = html.to_string();
    for fragment in &excluded_fragments {
        cleaned = cleaned.replace(fragment, "");
    }

    // 連続する空白行を整理
    let re = Regex::new(r"\n\s*\n\s*\n").unwrap();
    re.replace_all(&cleaned, "\n\n").to_string()
}

/// 本文らしさを判定するためのスコアリング用セレクタ
const CONTENT_SELECTORS: &[&str] = &[
    "article",
    "main",
    "[role='main']",
    ".article",
    ".content",
    ".post",
    ".entry",
    ".post-content",
    ".article-content",
    ".entry-content",
    "#content",
    "#main",
    "#article",
];

/// 本文として不適切な要素のセレクタ
const NON_CONTENT_SELECTORS: &[&str] = &[
    "nav", "header", "footer", "aside",
    ".sidebar", ".menu", ".navigation",
    ".comment", ".comments", ".footer", ".header",
];

/// 要素のテキスト密度を計算（テキスト長 / HTML長）
fn calculate_text_density(html: &str, text: &str) -> f64 {
    if html.is_empty() {
        return 0.0;
    }
    text.len() as f64 / html.len() as f64
}

/// 要素のリンク密度を計算（リンクテキスト長 / 全テキスト長）
fn calculate_link_density(elem: &scraper::ElementRef) -> f64 {
    let total_text: String = elem.text().collect();
    if total_text.is_empty() {
        return 0.0;
    }

    let link_selector = Selector::parse("a").unwrap();
    let link_text_len: usize = elem
        .select(&link_selector)
        .map(|a| a.text().collect::<String>().len())
        .sum();

    link_text_len as f64 / total_text.len() as f64
}

/// 要素の本文スコアを計算
fn calculate_content_score(elem: &scraper::ElementRef) -> f64 {
    let html = elem.html();
    let text: String = elem.text().collect();

    // 基本スコア
    let mut score = 0.0;

    // テキスト密度（高いほど良い）
    let text_density = calculate_text_density(&html, &text);
    score += text_density * 100.0;

    // リンク密度（低いほど良い）
    let link_density = calculate_link_density(elem);
    score -= link_density * 50.0;

    // 段落タグの数（多いほど良い）
    let p_selector = Selector::parse("p").unwrap();
    let p_count = elem.select(&p_selector).count();
    score += (p_count as f64).min(10.0) * 5.0;

    // テキスト長ボーナス（一定以上のテキストがある場合）
    if text.len() > 500 {
        score += 20.0;
    }
    if text.len() > 1000 {
        score += 10.0;
    }

    // クラス名/ID による調整
    if let Some(class) = elem.value().attr("class") {
        let class_lower = class.to_lowercase();
        if class_lower.contains("article") || class_lower.contains("content") || class_lower.contains("post") {
            score += 25.0;
        }
        if class_lower.contains("sidebar") || class_lower.contains("comment") || class_lower.contains("nav") {
            score -= 25.0;
        }
    }
    if let Some(id) = elem.value().attr("id") {
        let id_lower = id.to_lowercase();
        if id_lower.contains("article") || id_lower.contains("content") || id_lower.contains("main") {
            score += 25.0;
        }
    }

    score
}

/// Readability風のヒューリスティックで本文を抽出する
pub fn extract_main_content(html: &str) -> Option<String> {
    let doc = scraper::Html::parse_document(html);

    // まず、本文らしいセレクタで要素を探す
    for selector_str in CONTENT_SELECTORS {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(elem) = doc.select(&selector).next() {
                let text: String = elem.text().collect();
                // 十分なテキスト量がある場合は採用
                if text.len() > 200 {
                    return Some(elem.html());
                }
            }
        }
    }

    // セレクタで見つからない場合、スコアリングで最適な要素を探す
    let candidates_selector = Selector::parse("div, section, article, main").unwrap();
    let mut best_score = 0.0;
    let mut best_html: Option<String> = None;

    for elem in doc.select(&candidates_selector) {
        let text: String = elem.text().collect();

        // 最低限のテキスト量がない要素はスキップ
        if text.len() < 100 {
            continue;
        }

        // 非コンテンツ要素はスキップ
        let elem_html = elem.html();
        let is_non_content = NON_CONTENT_SELECTORS.iter().any(|sel| {
            if let Ok(s) = Selector::parse(sel) {
                doc.select(&s).any(|e| e.html() == elem_html)
            } else {
                false
            }
        });
        if is_non_content {
            continue;
        }

        let score = calculate_content_score(&elem);
        if score > best_score {
            best_score = score;
            best_html = Some(elem_html);
        }
    }

    best_html
}

/// セレクタ抽出に失敗した場合のフォールバックとしてReadability風抽出を使用
pub fn extract_content_with_fallback(html: &str, primary_selector: &str) -> Option<String> {
    let doc = scraper::Html::parse_document(html);

    // まずプライマリセレクタを試す
    if let Ok(selector) = Selector::parse(primary_selector) {
        if let Some(elem) = doc.select(&selector).next() {
            return Some(elem.html());
        }
    }

    // フォールバック: Readability風抽出
    extract_main_content(html)
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, Display, EnumString)]
pub enum Status {
    #[default]
    #[serde(rename = "new")]
    #[strum(serialize = "new")]
    New,
    #[serde(rename = "archived")]
    #[strum(serialize = "archived")]
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebArticleProperty {
    pub summary: Option<String>,
    pub is_new_technology_related: Option<bool>,
    pub is_new_product_related: Option<bool>,
    pub is_new_academic_paper_related: Option<bool>,
    pub is_ai_related: Option<bool>,
    pub is_security_related: Option<bool>,
    pub is_it_related: Option<bool>,
}

impl Default for WebArticleProperty {
    fn default() -> Self {
        Self {
            summary: Some("".to_string()),
            is_new_technology_related: Some(false),
            is_new_product_related: Some(false),
            is_new_academic_paper_related: Some(false),
            is_ai_related: Some(false),
            is_security_related: Some(false),
            is_it_related: Some(false),
        }
    }
}

#[derive(Debug, Clone, new, Default, Serialize, Deserialize)]
pub struct WebSite {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebArticle {
    pub site: WebSite,
    pub title: String,
    pub article_url: String,
    pub description: String,
    pub properties: WebArticleProperty,
    pub timestamp: DateTime<Local>,
    pub text: String,
    pub html: String,
}

impl WebArticle {
    pub fn new(
        site_name: String,
        site_url: String,
        title: String,
        article_url: String,
        description: String,
        timestamp: DateTime<Local>,
    ) -> Self {
        let reg_cdata = Regex::new(r"<!\[CDATA\[(?<text>.+?)\]\]>").unwrap();
        let title = reg_cdata
            .captures(&title)
            .and_then(|cap| cap.name("text").map(|m| m.as_str().to_string()))
            .unwrap_or(title);
        let description = reg_cdata
            .captures(&description)
            .and_then(|cap| cap.name("text").map(|m| m.as_str().to_string()))
            .unwrap_or(description);
        let description = html2md::rewrite_html(&description, false);
        Self {
            site: WebSite {
                name: site_name.clone(),
                url: site_url.clone(),
            },
            title,
            article_url,
            description,
            properties: WebArticleProperty::default(),
            timestamp,
            text: "".to_string(),
            html: "".to_string(),
        }
    }
}

static HTTP_CLIENT: OnceLock<request::Client> = OnceLock::new();

fn shared_client() -> &'static request::Client {
    HTTP_CLIENT.get_or_init(|| {
        let mut headers = request::header::HeaderMap::new();
        headers.insert(
            request::header::USER_AGENT,
            format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
                .parse()
                .unwrap(),
        );

        request::ClientBuilder::new()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(60))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .gzip(true)
            .brotli(true)
            .deflate(true)
            .build()
            .expect("Failed to build HTTP client")
    })
}

#[async_trait::async_trait]
pub trait WebSiteInterface: Send + Sync {
    fn site_name(&self) -> String;
    fn site_url(&self) -> Url;
    async fn get_articles(&mut self) -> AppResult<Vec<WebArticle>>;
    async fn parse_article(&mut self, url: &str) -> AppResult<(Html, Text)>;
    async fn login(&mut self) -> AppResult<Cookie>;
    fn domain(&self) -> String;
    fn trim_text(&self, text: &str) -> String {
        let re = Regex::new(r"\s\s+").unwrap();
        re.replace_all(text, "\n").to_string()
    }
    fn get_domain(&self, url: &str) -> AppResult<String> {
        Ok(Url::parse(url)?.domain().unwrap_or_default().to_string())
    }
    /// サイト固有の除外セレクタを返す（デフォルトは空）
    /// 各サイト実装でオーバーライドしてサイト特有の不要要素を指定できる
    fn site_specific_exclude_selectors(&self) -> Vec<&'static str> {
        vec![]
    }
    /// HTMLから広告・サイドバー等の不要要素を除去してクリーンなコンテンツを返す
    fn clean_content(&self, html: &str) -> String {
        let additional = self.site_specific_exclude_selectors();
        clean_html_with_selectors(html, &additional)
    }
    /// セレクタで抽出を試み，失敗した場合はReadability風ヒューリスティックで抽出
    fn extract_with_fallback(&self, html: &str, selector: &str) -> Option<String> {
        let result = extract_content_with_fallback(html, selector);
        result.map(|content| self.clean_content(&content))
    }
    /// Readability風ヒューリスティックで本文を抽出（セレクタなし）
    fn extract_main_content_heuristic(&self, html: &str) -> Option<String> {
        extract_main_content(html).map(|content| self.clean_content(&content))
    }
    async fn request(&self, url: &str, cookie_str: &str) -> AppResult<Response> {
        let url = request::Url::parse(url).unwrap();

        let mut request_builder = shared_client().get(url);

        if !cookie_str.is_empty() {
            request_builder = request_builder.header(request::header::COOKIE, cookie_str);
        }

        let response = match request_builder.send().await {
            Ok(response) => response,
            Err(e) => return Err(AppError::RequestError(e)),
        };
        Ok(response)
    }
}

impl From<Box<dyn WebSiteInterface>> for WebSite {
    fn from(site: Box<dyn WebSiteInterface>) -> Self {
        Self {
            name: site.site_name(),
            url: site.domain(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_html_removes_nav() {
        let html = r#"<html><body><nav>Menu</nav><article>Content</article></body></html>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("<nav>"));
        assert!(cleaned.contains("Content"));
    }

    #[test]
    fn test_clean_html_removes_script() {
        let html = r#"<html><body><script>alert('ad')</script><p>Article text</p></body></html>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("<script>"));
        assert!(cleaned.contains("Article text"));
    }

    #[test]
    fn test_clean_html_removes_aside() {
        let html = r#"<html><body><aside class="sidebar">Related</aside><main>Main content</main></body></html>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("<aside"));
        assert!(cleaned.contains("Main content"));
    }

    #[test]
    fn test_clean_html_removes_ad_classes() {
        let html = r#"<html><body><div class="advertisement">Buy now</div><p>Real content</p></body></html>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("advertisement"));
        assert!(cleaned.contains("Real content"));
    }

    #[test]
    fn test_clean_html_removes_social_share() {
        let html = r#"<html><body><div class="social-share">Share this</div><article>Article</article></body></html>"#;
        let cleaned = clean_html(html);
        assert!(!cleaned.contains("social-share"));
        assert!(cleaned.contains("Article"));
    }

    #[test]
    fn test_clean_html_preserves_main_content() {
        let html = r#"
            <html>
            <body>
                <header>Site header</header>
                <nav>Navigation</nav>
                <main>
                    <article>
                        <h1>Title</h1>
                        <p>This is the main article content.</p>
                    </article>
                </main>
                <aside>Sidebar</aside>
                <footer>Copyright</footer>
            </body>
            </html>
        "#;
        let cleaned = clean_html(html);
        // 除外要素が削除されている
        assert!(!cleaned.contains("<header>"));
        assert!(!cleaned.contains("<nav>"));
        assert!(!cleaned.contains("<aside>"));
        assert!(!cleaned.contains("<footer>"));
        // 本文が残っている
        assert!(cleaned.contains("Title"));
        assert!(cleaned.contains("main article content"));
    }

    #[test]
    fn test_clean_html_with_additional_selectors() {
        let html = r#"
            <html>
            <body>
                <div class="custom-ad">Custom advertisement</div>
                <div class="premium-banner">Premium content</div>
                <article>
                    <p>Main article content here.</p>
                </article>
                <div class="site-specific-widget">Widget</div>
            </body>
            </html>
        "#;
        // サイト固有のセレクタを追加
        let additional = vec![".custom-ad", ".premium-banner", ".site-specific-widget"];
        let cleaned = clean_html_with_selectors(html, &additional);

        // サイト固有の除外要素が削除されている
        assert!(!cleaned.contains("custom-ad"));
        assert!(!cleaned.contains("premium-banner"));
        assert!(!cleaned.contains("site-specific-widget"));
        // 本文が残っている
        assert!(cleaned.contains("Main article content here"));
    }

    #[test]
    fn test_clean_html_with_empty_additional_selectors() {
        let html = r#"<html><body><nav>Menu</nav><p>Content</p></body></html>"#;
        // 追加セレクタなしでも動作する
        let cleaned = clean_html_with_selectors(html, &[]);
        assert!(!cleaned.contains("<nav>"));
        assert!(cleaned.contains("Content"));
    }

    #[test]
    fn test_extract_main_content_with_article_tag() {
        let html = r#"
            <html>
            <body>
                <header>Site Header</header>
                <nav>Navigation Menu</nav>
                <article>
                    <h1>Article Title</h1>
                    <p>This is the first paragraph of the article content.</p>
                    <p>This is the second paragraph with more detailed information.</p>
                    <p>And here is the conclusion of this wonderful article.</p>
                </article>
                <aside>Related Articles</aside>
                <footer>Copyright 2024</footer>
            </body>
            </html>
        "#;
        let result = extract_main_content(html);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("Article Title"));
        assert!(content.contains("first paragraph"));
    }

    #[test]
    fn test_extract_main_content_with_content_class() {
        let html = r#"
            <html>
            <body>
                <div class="sidebar">Sidebar content</div>
                <div class="content">
                    <h1>Main Content Title</h1>
                    <p>This is substantial content that should be extracted.</p>
                    <p>More paragraphs here to ensure sufficient text length.</p>
                    <p>Additional content to meet the minimum threshold.</p>
                </div>
            </body>
            </html>
        "#;
        let result = extract_main_content(html);
        assert!(result.is_some());
        let content = result.unwrap();
        assert!(content.contains("Main Content Title"));
    }

    #[test]
    fn test_extract_content_with_fallback_primary_success() {
        let html = r#"
            <html>
            <body>
                <div id="custom-content">
                    <p>Content extracted by primary selector.</p>
                </div>
            </body>
            </html>
        "#;
        let result = extract_content_with_fallback(html, "#custom-content");
        assert!(result.is_some());
        assert!(result.unwrap().contains("primary selector"));
    }

    #[test]
    fn test_extract_content_with_fallback_uses_heuristic() {
        let html = r#"
            <html>
            <body>
                <article>
                    <h1>Fallback Article</h1>
                    <p>This content should be found by the heuristic fallback.</p>
                    <p>More content here to ensure it passes the threshold.</p>
                    <p>Even more substantial content for the extraction algorithm.</p>
                </article>
            </body>
            </html>
        "#;
        // 存在しないセレクタを指定
        let result = extract_content_with_fallback(html, "#nonexistent-selector");
        assert!(result.is_some());
        assert!(result.unwrap().contains("Fallback Article"));
    }

    #[test]
    fn test_calculate_text_density() {
        // HTMLタグが多いとテキスト密度は低い
        let html_heavy = "<div><span><a href='#'>Link</a></span></div>";
        let text_heavy = "Link";
        let density = calculate_text_density(html_heavy, text_heavy);
        assert!(density < 0.2);

        // テキストが多いと密度は高い
        let html_light = "<p>This is plain text content.</p>";
        let text_light = "This is plain text content.";
        let density = calculate_text_density(html_light, text_light);
        assert!(density > 0.5);
    }
}
