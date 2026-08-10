//! 来歴クラス（provenance class）の分類．
//!
//! 来歴クラスは「主張の発生源からの距離」を表す**事実の分類であって，
//! 信頼度の点数ではない**．順位付けには一切使わず，用途は次の 2 つに限る．
//!
//! - `Origin` は接地度の検証対象から外す（自身が出典であり，照合する外部主張がない）
//! - 一次情報への差し替え候補を決める
//!
//! 未知のドメインは `Unknown` とし，`Reporting` と同等に扱って**減点しない**．

use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// 主張の発生源からの距離を表す分類．
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumString,
)]
pub enum Provenance {
    /// 主張の発生源．AI ラボ・研究機関・企業自身の技術ブログ・大学・政府・プレプリント．
    /// 「その組織自身が自分について書いたもの」．
    #[serde(rename = "origin")]
    #[strum(serialize = "origin")]
    Origin,
    /// 取材・報道．一次情報を取材して伝える媒体．
    #[serde(rename = "reporting")]
    #[strum(serialize = "reporting")]
    Reporting,
    /// 投稿プラットフォーム．企業技術ブログと個人記事が同居するため，
    /// 事前分布はドメインではなく著者スラッグ単位で引く（`cgm_author_slug`）．
    #[serde(rename = "cgm")]
    #[strum(serialize = "cgm")]
    Cgm,
    /// まとめ・転載．独自取材を伴わない再配信．
    #[serde(rename = "aggregator")]
    #[strum(serialize = "aggregator")]
    Aggregator,
    /// 分類表に無いドメイン．`Reporting` と同等に扱い，減点しない．
    #[default]
    #[serde(rename = "unknown")]
    #[strum(serialize = "unknown")]
    Unknown,
}

impl Provenance {
    /// 接地判定の対象になるか．`Origin` は自身が出典なので対象外．
    pub fn needs_grounding(&self) -> bool {
        !matches!(self, Provenance::Origin)
    }
}

/// 主張の発生源．その組織自身が自分について書いたもの．
const ORIGIN_DOMAINS: &[&str] = &[
    // AI ラボ・プラットフォーム
    "openai.com",
    "anthropic.com",
    "deepmind.google",
    "research.google",
    "blog.google",
    "ai.meta.com",
    "huggingface.co",
    "nvidia.co.jp",
    "nvidia.com",
    // プレプリント
    "arxiv.org",
    // 大手ベンダ
    "aws.amazon.com",
    "amazon.com",
    "github.blog",
    "microsoft.com",
    "google.com",
    "news.mit.edu",
    // 公的機関
    "www.aisi.gov.uk",
    "jdla.org",
    // セキュリティベンダ
    "crowdstrike.com",
    "checkpoint.com",
    "trendmicro.com",
    "canon-its.jp",
    "eset-info.canon-its.jp",
    "www.motex.co.jp",
    // 国内企業の技術ブログ
    "techblog.lycorp.co.jp",
    "lycorp.co.jp",
    "buildersbox.corp-sansan.com",
    "corp-sansan.com",
    "developers.cyberagent.co.jp",
    "cyberagent.co.jp",
    "blog.cybozu.io",
    "cybozu.io",
    "engineering.dena.com",
    "dena.com",
    "engineering.mercari.com",
    "mercari.com",
    "knowledge.sakura.ad.jp",
    "stockmark.co.jp",
    "stockmark-tech.hatenablog.com",
    "zenmutech.com",
    "moneyforward-dev.jp",
    "preferred.jp",
    "group.ntt",
    "security.ntt",
    "ntt.com",
    "nttdata.com",
    "tech.gunosy.io",
    "labs.gree.jp",
    "techlife.cookpad.com",
    "cookpad.com",
    "supership.jp",
    // OSS プロジェクト自身の公式ブログ
    "blog.rust-lang.org",
    "rust-lang.org",
    // セキュリティベンダ自身の発信
    "sophos.com",
];

/// 取材・報道．一次情報を取材して伝える媒体．
const REPORTING_DOMAINS: &[&str] = &[
    "gigazine.net",
    "techcrunch.com",
    "itmedia.co.jp",
    "atmarkit.itmedia.co.jp",
    "mag.executive.itmedia.co.jp",
    "techtarget.itmedia.co.jp",
    "monoist.itmedia.co.jp",
    "marketing.itmedia.co.jp",
    "nikkei.com",
    "xtech.nikkei.com",
    "security-next.com",
    "techno-edge.net",
    "japan.cnet.com",
    "cnet.com",
    "japan.zdnet.com",
    "zdnet.com",
    "gizmodo.jp",
    "forbesjapan.com",
    "businessinsider.jp",
    "impress.co.jp",
    "codezine.jp",
    "ascii.jp",
    "sbbit.jp",
    "realsound.jp",
    "scan.netsecurity.ne.jp",
    "uchubiz.com",
    "ampmedia.jp",
    "webtan.impress.co.jp",
    "technologyreview.jp",
    "courrier.jp",
    "coindeskjapan.com",
    "coindesk.com",
    "coinpost.jp",
    "sankei.com",
    "diamond.jp",
    "mynavi.jp",
    "enterprisezine.jp",
    "bcnretail.com",
    "tomshardware.com",
    "hbr.org",
    "www.j-cast.com",
    "www.phileweb.com",
    "newspicks.com",
    "blackhatnews.tokyo",
    "huffingtonpost.jp",
    // 論文・技術動向の解説メディア（独自の書き起こしを伴う）
    "ai-scholar.tech",
    "otafuku-lab.co",
];

/// 投稿プラットフォーム．企業技術ブログと個人記事が同居する．
const CGM_DOMAINS: &[&str] = &[
    "zenn.dev",
    "qiita.com",
    "note.com",
    "medium.com",
    "hatenablog.com",
    "speakerdeck.com",
    "docswell.com",
    "theletter.jp",
];

/// まとめ・転載．独自取材を伴わない再配信．
const AGGREGATOR_DOMAINS: &[&str] = &[
    "news.yahoo.co.jp",
    "yahoo.co.jp",
    "prtimes.jp",
    "aismiley.co.jp",
    "ainow.ai",
    "ledge.ai",
    "ai-data-base.com",
    "b.hatena.ne.jp",
    // AI 系ニュースのフィード集約
    "ai-news.dev",
];

/// 末尾一致で判定する規則（大学・政府ドメインの一括指定）．
/// 完全一致・親ドメイン一致のいずれにも当たらなかった場合にのみ使う．
const SUFFIX_RULES: &[(&str, Provenance)] = &[
    (".ac.jp", Provenance::Origin),
    (".go.jp", Provenance::Origin),
    (".edu", Provenance::Origin),
    (".gov", Provenance::Origin),
];

/// 親ドメインへの遡上を打ち切る末尾．ここまで削ると意味を持たなくなる．
const PUBLIC_SUFFIX_TAILS: &[&str] = &[
    "co.jp", "ne.jp", "or.jp", "ac.jp", "go.jp", "co.uk", "com.cn",
];

/// ホスト名を正規化する．スキーム・パス・ポート・末尾ドットを落として小文字化する．
///
/// `domain()` の実装がサイトによって URL 断片（`www.itmedia.co.jp/enterprise` 等）を
/// 返すことがあるため，入力は寛容に受ける．
fn normalize_host(input: &str) -> String {
    let s = input.trim().to_lowercase();
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(&s);
    let s = s.split('/').next().unwrap_or("");
    let s = s.split('?').next().unwrap_or("");
    let s = s.split('#').next().unwrap_or("");
    // ポート番号を落とす（IPv6 は対象外）
    let s = s.split(':').next().unwrap_or("");
    s.trim_end_matches('.').to_string()
}

/// 分類表から完全一致で引く．
fn exact_lookup(host: &str) -> Option<Provenance> {
    if ORIGIN_DOMAINS.contains(&host) {
        return Some(Provenance::Origin);
    }
    if REPORTING_DOMAINS.contains(&host) {
        return Some(Provenance::Reporting);
    }
    if CGM_DOMAINS.contains(&host) {
        return Some(Provenance::Cgm);
    }
    if AGGREGATOR_DOMAINS.contains(&host) {
        return Some(Provenance::Aggregator);
    }
    None
}

/// これ以上親へ遡ってはいけないホストか．
fn is_public_suffix(host: &str) -> bool {
    PUBLIC_SUFFIX_TAILS.contains(&host) || host.matches('.').count() < 1
}

/// ドメインから来歴クラスを決める．
///
/// # 判定順
/// 1. ホスト名の完全一致
/// 2. 親ドメインへ 1 ラベルずつ遡っての一致（`marketing.itmedia.co.jp` → `itmedia.co.jp`）
/// 3. 末尾規則（`.ac.jp` / `.go.jp` / `.edu` / `.gov`）
/// 4. いずれにも当たらなければ `Unknown`
///
/// 完全一致が親ドメイン一致より優先されるため，`stockmark-tech.hatenablog.com`
/// のような「CGM 上の企業技術ブログ」は `Origin` のまま保たれる．
///
/// 親ドメインへの遡上は Python 実装（`provenance.py`）には無い拡張である．
/// 表に無いサブドメインが `Unknown` に落ちるのを防ぐために足した．
pub fn provenance_of_domain(domain: &str) -> Provenance {
    let host = normalize_host(domain);
    if host.is_empty() {
        return Provenance::Unknown;
    }

    if let Some(p) = exact_lookup(&host) {
        return p;
    }

    // 親ドメインへ遡る．公開接尾辞まで削り切ったら打ち切る．
    let mut rest = host.as_str();
    while let Some((_, parent)) = rest.split_once('.') {
        if is_public_suffix(parent) {
            break;
        }
        if let Some(p) = exact_lookup(parent) {
            return p;
        }
        rest = parent;
    }

    for (suffix, class) in SUFFIX_RULES {
        if host.ends_with(suffix) {
            return *class;
        }
    }

    Provenance::Unknown
}

/// URL から来歴クラスを決める．ホスト部を取り出して `provenance_of_domain` に渡す．
pub fn provenance_of_url(url: &str) -> Provenance {
    provenance_of_domain(url)
}

/// CGM プラットフォームの著者スラッグが現れるパスセグメント位置（0 始まり）．
const CGM_AUTHOR_PATH_INDEX: &[(&str, usize)] = &[
    ("zenn.dev", 0),
    ("qiita.com", 0),
    ("note.com", 0),
    ("medium.com", 0),
];

/// CGM の URL から著者スラッグ（`<host>/<author>`）を返す．
///
/// 事前分布のキーに使う．CGM は企業技術ブログと個人記事が同居するため，
/// ドメイン単位で事前分布を引くと両者が混ざる．
pub fn cgm_author_slug(url: &str) -> Option<String> {
    let s = url.trim().to_lowercase();
    let without_scheme = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))?;
    let (host, rest) = without_scheme.split_once('/')?;
    let idx = CGM_AUTHOR_PATH_INDEX
        .iter()
        .find(|(h, _)| *h == host)
        .map(|(_, i)| *i)?;
    let segment = rest.split('/').filter(|s| !s.is_empty()).nth(idx)?;
    Some(format!("{}/{}", host, segment.trim_start_matches('@')))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        assert_eq!(provenance_of_domain("openai.com"), Provenance::Origin);
        assert_eq!(provenance_of_domain("gigazine.net"), Provenance::Reporting);
        assert_eq!(provenance_of_domain("zenn.dev"), Provenance::Cgm);
        assert_eq!(provenance_of_domain("prtimes.jp"), Provenance::Aggregator);
    }

    #[test]
    fn test_unknown_domain() {
        assert_eq!(
            provenance_of_domain("example-unheard-of.tld"),
            Provenance::Unknown
        );
        assert_eq!(provenance_of_domain(""), Provenance::Unknown);
    }

    #[test]
    fn test_suffix_rules() {
        assert_eq!(
            provenance_of_domain("www.t.u-tokyo.ac.jp"),
            Provenance::Origin
        );
        assert_eq!(provenance_of_domain("www.ipa.go.jp"), Provenance::Origin);
        assert_eq!(provenance_of_domain("cs.stanford.edu"), Provenance::Origin);
        assert_eq!(provenance_of_domain("nist.gov"), Provenance::Origin);
    }

    #[test]
    fn test_exact_match_wins_over_parent() {
        // CGM 上の企業技術ブログ．親 (hatenablog.com) は Cgm だが，
        // 完全一致が優先されて Origin のまま保たれる．
        assert_eq!(
            provenance_of_domain("stockmark-tech.hatenablog.com"),
            Provenance::Origin
        );
        assert_eq!(provenance_of_domain("hatenablog.com"), Provenance::Cgm);
        assert_eq!(
            provenance_of_domain("someone-else.hatenablog.com"),
            Provenance::Cgm
        );
    }

    #[test]
    fn test_parent_domain_fallback() {
        // 表に無いサブドメインは親に落として分類する
        assert_eq!(
            provenance_of_domain("pc.watch.impress.co.jp"),
            Provenance::Reporting
        );
        assert_eq!(
            provenance_of_domain("some-new-subdomain.gigazine.net"),
            Provenance::Reporting
        );
    }

    #[test]
    fn test_parent_fallback_stops_at_public_suffix() {
        // co.jp まで削っても一致しないものは Unknown．
        // 「co.jp だから reporting」のような誤分類を起こさない．
        assert_eq!(
            provenance_of_domain("totally-unknown-company.co.jp"),
            Provenance::Unknown
        );
    }

    #[test]
    fn test_normalize_host() {
        // domain() が URL 断片を返すサイトがあるため寛容に受ける
        assert_eq!(
            provenance_of_domain("www.itmedia.co.jp/enterprise"),
            Provenance::Reporting
        );
        assert_eq!(
            provenance_of_domain("https://gigazine.net/news/rss_2.0/"),
            Provenance::Reporting
        );
        assert_eq!(provenance_of_domain("  OpenAI.com  "), Provenance::Origin);
        assert_eq!(
            provenance_of_domain("gigazine.net:443"),
            Provenance::Reporting
        );
    }

    #[test]
    fn test_needs_grounding() {
        assert!(!Provenance::Origin.needs_grounding());
        assert!(Provenance::Reporting.needs_grounding());
        assert!(Provenance::Unknown.needs_grounding());
    }

    #[test]
    fn test_cgm_author_slug() {
        assert_eq!(
            cgm_author_slug("https://zenn.dev/someuser/articles/abc123").as_deref(),
            Some("zenn.dev/someuser")
        );
        assert_eq!(
            cgm_author_slug("https://qiita.com/someuser/items/abc123").as_deref(),
            Some("qiita.com/someuser")
        );
        // medium は @ を落とす
        assert_eq!(
            cgm_author_slug("https://medium.com/@someuser/title-abc").as_deref(),
            Some("medium.com/someuser")
        );
        // CGM 以外は None
        assert_eq!(cgm_author_slug("https://gigazine.net/news/foo"), None);
        assert_eq!(cgm_author_slug("https://zenn.dev"), None);
    }

    #[test]
    fn test_serde_roundtrip_matches_skill_vocabulary() {
        // skill 側の判定ログ (_meta/credibility.jsonl) と語彙を揃える
        assert_eq!(
            serde_json::to_string(&Provenance::Origin).unwrap(),
            "\"origin\""
        );
        assert_eq!(
            serde_json::to_string(&Provenance::Aggregator).unwrap(),
            "\"aggregator\""
        );
        let p: Provenance = serde_json::from_str("\"cgm\"").unwrap();
        assert_eq!(p, Provenance::Cgm);
    }
}
