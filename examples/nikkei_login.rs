use news_clipper::models::sites::nikkei_cyber::manual_login;

#[tokio::main]
async fn main() {
    println!("Chrome で日経にログインしてください（最大 5 分待ちます）．");
    match manual_login().await {
        Ok(_) => println!("日経の Cookie を macOS キーチェーンに保存しました．"),
        Err(error) => {
            eprintln!("日経のログインに失敗しました: {error}");
            std::process::exit(1);
        }
    }
}
