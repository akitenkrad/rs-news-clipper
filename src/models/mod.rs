pub mod sites;
pub mod web_article;
pub mod web_site;
use crate::models::sites::*;
use crate::models::web_article::WebSiteInterface;
use crate::shared::errors::AppResult;

pub async fn get_all_sites() -> AppResult<Vec<Box<dyn WebSiteInterface>>> {
    let sites: Vec<Box<dyn WebSiteInterface>> = vec![
        Box::new(ai_db::AIDB::default()),
        Box::new(ai_it_now::AIItNow::default()),
        Box::new(ai_news::AINews::default()),
        Box::new(ai_scholar::AIScholar::default()),
        Box::new(aismiley::AISmiley::default()),
        Box::new(aizine::AIZine::default()),
        Box::new(aws_security_blog::AWSSecurityBlog::default()),
        Box::new(business_insider_science::BusinessInsiderScience::default()),
        Box::new(business_insider_technology::BusinessInsiderTechnology::default()),
        Box::new(canon_malware_center::CanonMalwareCenter::default()),
        Box::new(codezine::CodeZine::default()),
        Box::new(cookpad_techblog::CookpadTechBlog::default()),
        Box::new(crowdstrike_blog::CrowdStrikeBlog::default()),
        Box::new(cyberagent_techblog::CyberAgentTechBlog::default()),
        Box::new(cybozu_blog::CybozuBlog::default()),
        Box::new(dena_engineering_blog::DeNAEngineeringBlog::default()),
        Box::new(gigazine::Gigazine::default()),
        Box::new(github_developers_blog::GitHubDevelopersBlog::default()),
        Box::new(gizmodo::Gizmodo::default()),
        Box::new(google_developers_blog::GoogleDevelopersBlog::default()),
        Box::new(gree_techblog::GreeTechBlog::default()),
        Box::new(gunosy_techblog::GunosyTechBlog::default()),
        Box::new(ipa_security_center::IPASecurityCenter::default()),
        Box::new(itmedia_at_it::ITMediaAtIt::default()),
        Box::new(itmedia_enterprise::ITMediaEnterprise::default()),
        Box::new(itmedia_marketing::ITMediaMarketing::default()),
        Box::new(itmedia_general::ITMediaGeneral::default()),
        Box::new(jpcert::JPCert::default()),
        Box::new(line_techblog::LineTechBlog::default()),
        // Box::new(medium::Medium::new(
        //     "Artificial Intelligence",
        //     "artificial-intelligence",
        // )),
        // Box::new(medium::Medium::new("AI", "ai")),
        // Box::new(medium::Medium::new("Machine Learning", "machine-learning")),
        // Box::new(medium::Medium::new("ChatGPT", "chatgpt")),
        // Box::new(medium::Medium::new("Data Science", "data-science")),
        // Box::new(medium::Medium::new("OpenAI", "openai")),
        // Box::new(medium::Medium::new("LLM", "llm")),
        Box::new(mercari_engineering_blog::MercariEngineeringBlog::default()),
        Box::new(mit_ai::MITAI::default()),
        Box::new(mit_research::MITResearch::default()),
        Box::new(moneyforward_developers_blog::MoneyForwardDevelopersBlog::default()),
        Box::new(motex::MoTex::default()),
        Box::new(nikkei_xtech::NikkeiXTech::default()),
        Box::new(qiita_blog::QiitaBlog::default()),
        Box::new(retrieva_techblog::RetrievaTechBlog::default()),
        Box::new(rust_blog::RustBlog::default()),
        Box::new(sakura_internet_techblog::SakuraInternetTechBlog::default()),
        Box::new(sansan::Sansan::default()),
        Box::new(security_next::SecurityNext::default()),
        Box::new(sophos_news::SophosNews::default()),
        Box::new(stockmark_news::StockmarkNews::default()),
        Box::new(stockmark_techblog::StockmarkTechBlog::default()),
        Box::new(supership::Supership::default()),
        Box::new(tech_crunch::TechCrunch::default()),
        Box::new(tokyo_univ_engineering::TokyoUniversityEngineering::default()),
        Box::new(trend_micro_security_news::TrendMicroSecurityNews::default()),
        Box::new(trend_micro_security_advisories::TrendMicroSecurityAdvisories::default()),
        Box::new(yahoo_news_it::YahooNewsIT::default()),
        Box::new(yahoo_japan_techblog::YahooJapanTechBlog::default()),
        Box::new(zen_mu_tech::ZenmuTech::default()),
        Box::new(zenn_topic::ZennTopic::new("自然言語処理")),
        Box::new(zenn_topic::ZennTopic::new("生成ai")),
        Box::new(zenn_topic::ZennTopic::new("rust")),
        Box::new(zenn_topic::ZennTopic::new("ai")),
        Box::new(zenn_topic::ZennTopic::new("基盤")),
        Box::new(zenn_topic::ZennTopic::new("データサイエンス")),
        Box::new(zenn_topic::ZennTopic::new("AWS")),
        Box::new(zenn_trend::ZennTrend::default()),
    ];

    Ok(sites)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::logger::init_logger;
    use tracing::{Level, event};

    #[tokio::test]
    async fn test_all_sites() {
        init_logger("DEBUG").expect("Failed to initialize logger");

        let sites = get_all_sites().await.unwrap();

        sites.iter().for_each(|site| {
            event!(Level::INFO, "Site Name:{}", site.site_name());
        });

        assert!(!sites.is_empty());
        let mut articles = Vec::new();
        for site in sites {
            let mut site = site;
            let result = site.get_articles().await;
            match result {
                Ok(site_articles) => {
                    event!(
                        Level::INFO,
                        "Site Name:{} Articles Count:{}",
                        site.site_name(),
                        site_articles.len()
                    );
                    articles.extend(site_articles);
                }
                Err(e) => {
                    println!("Error fetching articles from {}: {}", site.site_name(), e);
                }
            }
        }
        assert!(!articles.is_empty());
    }

    /// parse_article の統合テスト：実際のサイトから記事を取得してパースする
    #[tokio::test]
    async fn test_parse_article_clean_content() {
        init_logger("INFO").expect("Failed to initialize logger");

        // テスト対象のサイト（clean_content を適用したもの）
        let test_sites: Vec<Box<dyn WebSiteInterface>> = vec![
            Box::new(qiita_blog::QiitaBlog::default()),
            Box::new(tech_crunch::TechCrunch::default()),
        ];

        for site in test_sites {
            let mut site = site;
            let site_name = site.site_name();
            event!(Level::INFO, "Testing parse_article for: {}", site_name);

            // 記事一覧を取得
            let articles = match site.get_articles().await {
                Ok(articles) => articles,
                Err(e) => {
                    event!(Level::WARN, "Skipping {} due to error: {}", site_name, e);
                    continue;
                }
            };

            if articles.is_empty() {
                event!(Level::WARN, "No articles found for {}", site_name);
                continue;
            }

            // 最初の記事をパース
            let article = &articles[0];
            event!(Level::INFO, "Parsing article: {}", article.title);

            let (html, text) = match site.parse_article(&article.article_url).await {
                Ok(result) => result,
                Err(e) => {
                    event!(Level::WARN, "Failed to parse article from {}: {}", site_name, e);
                    continue;
                }
            };

            // 除外されるべき要素が含まれていないことを確認
            assert!(!html.contains("<nav>"), "{}: nav should be removed", site_name);
            assert!(!html.contains("<script>"), "{}: script should be removed", site_name);
            assert!(!html.contains("<aside>"), "{}: aside should be removed", site_name);

            // コンテンツが存在することを確認
            assert!(!html.is_empty(), "{}: html should not be empty", site_name);
            assert!(!text.is_empty(), "{}: text should not be empty", site_name);

            event!(
                Level::INFO,
                "{}: html length={}, text length={}",
                site_name,
                html.len(),
                text.len()
            );
        }
    }
}
