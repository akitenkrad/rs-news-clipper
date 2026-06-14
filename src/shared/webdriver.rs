use request::Client;
use std::error::Error;
use std::path::PathBuf;
use std::time::Duration;
use thirtyfour::{ChromiumLikeCapabilities, DesiredCapabilities, WebDriver};
use tokio::process::Child;

type BoxError = Box<dyn Error + Send + Sync>;

const VERSIONS_URL: &str =
    "https://googlechromelabs.github.io/chrome-for-testing/last-known-good-versions-with-downloads.json";

pub struct ManagedChromeDriver {
    process: Option<Child>,
    driver: WebDriver,
}

impl ManagedChromeDriver {
    pub async fn launch() -> Result<Self, BoxError> {
        let (platform, binary_name) = detect_platform()?;

        let client = Client::new();
        let versions: serde_json::Value = client.get(VERSIONS_URL).send().await?.json().await?;

        let version = versions["channels"]["Stable"]["version"]
            .as_str()
            .ok_or("Stable.version not found")?
            .to_string();

        let downloads = versions["channels"]["Stable"]["downloads"]["chromedriver"]
            .as_array()
            .ok_or("chromedriver downloads array not found")?;
        let download_url = downloads
            .iter()
            .find(|entry| entry["platform"].as_str() == Some(platform))
            .and_then(|entry| entry["url"].as_str())
            .ok_or_else(|| format!("no chromedriver download for platform {}", platform))?
            .to_string();

        let base_dir = dirs::cache_dir()
            .ok_or("cache_dir not available")?
            .join("news_clipper")
            .join("chromedriver")
            .join(&version);
        let binary_path = base_dir
            .join(format!("chromedriver-{}", platform))
            .join(binary_name);

        if !binary_path.exists() {
            download_and_extract(&client, &download_url, &base_dir, &binary_path).await?;
        }

        let port = pick_free_port()?;
        let mut process = tokio::process::Command::new(&binary_path)
            .arg(format!("--port={}", port))
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .spawn()?;

        if let Err(e) = wait_until_ready(&client, port).await {
            let _ = process.kill().await;
            let _ = process.wait().await;
            return Err(e);
        }

        let mut caps = DesiredCapabilities::chrome();
        caps.add_arg("--headless=new")?;
        // Bot 検出回避: navigator.webdriver=true を抑止し，本物の Chrome の UA を装う．
        caps.add_arg("--disable-blink-features=AutomationControlled")?;
        caps.add_arg(
            "--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
             AppleWebKit/537.36 (KHTML, like Gecko) Chrome/148.0.0.0 Safari/537.36",
        )?;
        let driver = match WebDriver::new(&format!("http://127.0.0.1:{}", port), caps).await {
            Ok(d) => d,
            Err(e) => {
                let _ = process.kill().await;
                let _ = process.wait().await;
                return Err(Box::new(e));
            }
        };

        Ok(Self {
            process: Some(process),
            driver,
        })
    }

    pub fn driver(&self) -> &WebDriver {
        &self.driver
    }

    pub async fn close(mut self) -> Result<(), BoxError> {
        self.driver.clone().quit().await?;
        if let Some(mut p) = self.process.take() {
            let _ = p.kill().await;
            let _ = p.wait().await;
        }
        Ok(())
    }
}

impl Drop for ManagedChromeDriver {
    fn drop(&mut self) {
        // Drop は async kill を呼べないので start_kill のみ．
        if let Some(p) = self.process.as_mut() {
            let _ = p.start_kill();
        }
    }
}

fn detect_platform() -> Result<(&'static str, &'static str), BoxError> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Ok(("mac-arm64", "chromedriver"))
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Ok(("mac-x64", "chromedriver"))
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok(("linux64", "chromedriver"))
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Ok(("win64", "chromedriver.exe"))
    }
    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "x86_64"),
    )))]
    {
        Err("unsupported platform for ManagedChromeDriver".into())
    }
}

async fn download_and_extract(
    client: &Client,
    url: &str,
    base_dir: &PathBuf,
    binary_path: &PathBuf,
) -> Result<(), BoxError> {
    std::fs::create_dir_all(base_dir)?;
    let zip_path = base_dir.join("chromedriver.zip");

    let bytes = client.get(url).send().await?.bytes().await?;
    std::fs::write(&zip_path, &bytes)?;

    let file = std::fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    archive.extract(base_dir)?;

    let _ = std::fs::remove_file(&zip_path);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if binary_path.exists() {
            std::fs::set_permissions(binary_path, std::fs::Permissions::from_mode(0o755))?;
        }
    }

    if !binary_path.exists() {
        return Err(format!(
            "chromedriver binary not found after extraction: {:?}",
            binary_path
        )
        .into());
    }
    Ok(())
}

fn pick_free_port() -> Result<u16, BoxError> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

async fn wait_until_ready(client: &Client, port: u16) -> Result<(), BoxError> {
    let url = format!("http://127.0.0.1:{}/status", port);
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        if let Ok(resp) = client.get(&url).send().await
            && let Ok(json) = resp.json::<serde_json::Value>().await
            && json["value"]["ready"].as_bool() == Some(true)
        {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            return Err("chromedriver did not become ready within 30s".into());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}
