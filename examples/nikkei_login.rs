//! 日経の自動ログイン用資格情報は，次のコマンドで macOS キーチェーンへ登録する．
//!
//! ```text
//! security add-generic-password -s news-clipper-nikkei -a user -w
//! security add-generic-password -s news-clipper-nikkei -a password -w
//! ```
//!
//! `-w` を値なしで付けると，値を対話入力できる．

use news_clipper::models::sites::nikkei_cyber::{auto_login, auto_login_with_fresh_profile, manual_login};

#[tokio::main]
async fn main() {
    let mut auto = false;
    let mut fresh_profile = false;
    for argument in std::env::args().skip(1) {
        match argument.as_str() {
            "--auto" if !auto => auto = true,
            "--fresh-profile" if !fresh_profile => fresh_profile = true,
            _ => {
                eprintln!("使い方: cargo run --example nikkei_login -- [--auto [--fresh-profile]]");
                std::process::exit(2);
            }
        }
    }
    if fresh_profile && !auto {
        eprintln!("--fresh-profile は --auto と一緒に指定してください．");
        std::process::exit(2);
    }

    if auto {
        if fresh_profile {
            println!("使い捨てプロファイルの Chrome で日経の自動ログインを試します．");
        } else {
            println!("Chrome で日経の自動ログインを試します．");
        }
        let result = if fresh_profile {
            auto_login_with_fresh_profile().await
        } else {
            auto_login().await
        };
        match result {
            Ok(_) => println!("日経の Cookie を macOS キーチェーンに保存しました．"),
            Err(error) => {
                eprintln!("日経の自動ログインに失敗しました: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    println!("Chrome で日経にログインしてください（最大 5 分待ちます）．");
    match manual_login().await {
        Ok(_) => println!("日経の Cookie を macOS キーチェーンに保存しました．"),
        Err(error) => {
            eprintln!("日経のログインに失敗しました: {error}");
            std::process::exit(1);
        }
    }
}
