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
}
