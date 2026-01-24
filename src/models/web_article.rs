use crate::shared::errors::{AppError, AppResult};
use chrono::{DateTime, Local};
use derive_new::new;
use regex::Regex;
use request::{Response, Url};
use serde::{Deserialize, Serialize};
use std::boxed::Box;
use std::sync::OnceLock;
use strum::{Display, EnumString};

pub type Html = String;
pub type Text = String;
pub type Cookie = String;

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
