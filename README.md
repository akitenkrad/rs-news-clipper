# news-clipper

Rustで実装されたニュースクリッピングライブラリ．複数のテックブログやニュースサイトから記事を自動収集します．

## 機能

- 50以上のニュースソースから記事を取得
- RSSフィード解析とWebスクレイピング
- 記事のプロパティ分析（AI関連、セキュリティ関連など）

## 対応サイト

テックブログ、AIニュース、セキュリティ情報など多数のソースに対応：

- **テックブログ**: Cookpad, CyberAgent, DeNA, LINE, Mercari, Yahoo! Japan など
- **AIニュース**: AI-DB, AINews, AI Scholar, MIT AI など
- **セキュリティ**: AWS Security Blog, CrowdStrike, IPA, JPCERT, Trend Micro など
- **メディア**: ITmedia, Gigazine, TechCrunch, Gizmodo, 日経xTECH など
- **開発者向け**: GitHub Blog, Google Developers Blog, Rust Blog, Qiita, Zenn など

## 使用方法

```rust
use news_clipper::models::get_all_sites;

#[tokio::main]
async fn main() {
    let sites = get_all_sites().await.unwrap();
    
    for mut site in sites {
        let articles = site.get_articles().await.unwrap();
        for article in articles {
            println!("{}: {}", article.site.name, article.title);
        }
    }
}
```
