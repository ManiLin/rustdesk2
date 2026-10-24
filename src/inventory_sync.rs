//! Optional telemetry: POST device snapshot to an inventory portal (HTTPS recommended).
use hbb_common::{
    allow_err,
    config::{self, keys, Config},
    log,
    password_security,
    tokio,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const FIRST_DELAY: Duration = Duration::from_secs(15);
const INTERVAL: Duration = Duration::from_secs(300);

static LOGGED_URL_EMPTY: AtomicBool = AtomicBool::new(false);

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn start() {
    std::thread::spawn(|| {
        allow_err!(tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map(|rt| rt.block_on(run_loop())));
    });
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn start() {}

async fn run_loop() {
    tokio::time::sleep(FIRST_DELAY).await;
    loop {
        if config::option2bool("stop-service", &Config::get_option("stop-service")) {
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }
        let url = Config::get_inventory_report_url();
        if url.is_empty() {
            if !LOGGED_URL_EMPTY.swap(true, Ordering::SeqCst) {
                log::info!(
                    "inventory portal: URL не задан — отчёты выключены. Задайте inventory-report-url в RustDesk2.toml \
                     или пересоберите с INVENTORY_REPORT_URL (см. README портала)."
                );
            }
            tokio::time::sleep(Duration::from_secs(60)).await;
            continue;
        }
        let token = {
            let t = Config::get_option(keys::OPTION_INVENTORY_REPORT_TOKEN);
            if t.is_empty() {
                config::DEFAULT_INVENTORY_REPORT_TOKEN.to_owned()
            } else {
                t
            }
        };
        if let Err(e) = send_report(&url, &token).await {
            log::warn!("inventory report failed: {}", e);
        }
        #[cfg(windows)]
        if let Err(e) = wallpaper::sync(&url, &token).await {
            log::warn!("inventory portal: wallpaper upload failed: {}", e);
        }
        tokio::time::sleep(INTERVAL).await;
    }
}

async fn send_report(url: &str, token: &str) -> hbb_common::ResultType<()> {
    let sys = crate::get_sysinfo();
    let rustdesk_id = Config::get_id();
    let hostname = sys
        .get("hostname")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let os = sys
        .get("os")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let cpu = sys
        .get("cpu")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let memory = sys
        .get("memory")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let username = sys
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_owned();
    let temp_password = password_security::temporary_password();
    let public_ip = fetch_public_ip().await.unwrap_or_default();
    let local_ip = guess_local_ipv4().unwrap_or_default();
    let body = json!({
        "rustdesk_id": rustdesk_id,
        "hostname": hostname,
        "os": os,
        "username": username,
        "cpu": cpu,
        "memory": memory,
        "computer_summary": format!("{} | {} | {}", cpu, memory, os),
        "ip_public": public_ip,
        "ip_local": local_ip,
        "temporary_password": temp_password,
        "app_version": crate::VERSION,
    })
    .to_string();
    let auth = format!("Authorization: Bearer {}", token);
    let resp = crate::post_request(url.to_owned(), body, &auth).await?;
    let r = resp.trim();
    let parsed = serde_json::from_str::<serde_json::Value>(r).ok();
    let error_code = parsed
        .as_ref()
        .and_then(|v| v.get("code"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let error_message = parsed
        .as_ref()
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or(r);
    if r == "unauthorized" || error_code == "unauthorized" {
        log::warn!(
            "inventory portal: 401 unauthorized — токен на клиенте не совпадает с INVENTORY_DEVICE_TOKEN на сервере \
             (по умолчанию должен совпадать с RS_PUB_KEY в вашей сборке hbb_common)."
        );
        return Ok(());
    }
    if parsed
        .as_ref()
        .and_then(|v| v.get("ok"))
        .and_then(|v| v.as_bool())
        == Some(true)
    {
        log::info!(
            "inventory portal: отчёт принят сервером (rustdesk_id={})",
            rustdesk_id
        );
    } else if r == "db error" || error_code == "internal_server_error" {
        log::warn!("inventory portal: сервер вернул ошибку БД");
    } else {
        let preview = if error_message.chars().count() > 240 {
            format!("{}…", error_message.chars().take(240).collect::<String>())
        } else {
            error_message.to_owned()
        };
        log::warn!("inventory portal: неожиданный ответ (проверьте URL до /api/v1/report): {}", preview);
    }
    Ok(())
}

async fn fetch_public_ip() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .ok()?;
    let v: serde_json::Value = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .ok()?
        .json()
        .await
        .ok()?;
    v.get("ip")?.as_str().map(|s| s.to_owned())
}

fn guess_local_ipv4() -> Option<String> {
    use std::net::UdpSocket;
    let s = UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    match s.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(a) => Some(a.to_string()),
        _ => None,
    }
}

/// Desktop wallpaper of the active user for peer cards. The portal makes the
/// thumbnail; the device uploads the original only when it changes.
#[cfg(windows)]
mod wallpaper {
    use hbb_common::{bail, config::Config, log, sha2::Digest, tokio, ResultType};
    use std::sync::Mutex;

    const MAX_UPLOAD_BYTES: usize = 24 * 1024 * 1024;

    /// SHA-256 of the wallpaper the portal already has.
    static LAST_SYNCED: Mutex<String> = Mutex::new(String::new());

    fn portal_base(report_url: &str) -> String {
        match report_url.find("/api/v1/report") {
            Some(idx) => report_url[..idx].trim_end_matches('/').to_owned(),
            None => report_url.trim_end_matches('/').to_owned(),
        }
    }

    pub async fn sync(report_url: &str, token: &str) -> ResultType<()> {
        let Some(path) = crate::platform::get_active_user_wallpaper_path() else {
            return Ok(());
        };
        let data = tokio::fs::read(&path).await?;
        if data.is_empty() || data.len() > MAX_UPLOAD_BYTES {
            return Ok(());
        }
        let sha = hex::encode(hbb_common::sha2::Sha256::digest(&data));
        if *LAST_SYNCED.lock().unwrap() == sha {
            return Ok(());
        }
        let id = Config::get_id();
        if id.is_empty() {
            return Ok(());
        }
        let url = format!("{}/api/v1/devices/{}/wallpaper", portal_base(report_url), id);
        let auth = format!("Bearer {}", token);
        let client = crate::hbbs_http::create_http_client_async_with_url(&url).await;

        let meta = client
            .get(format!("{}/meta", url))
            .header("Authorization", &auth)
            .send()
            .await?;
        if meta.status().is_success() {
            let meta: serde_json::Value = serde_json::from_str(&meta.text().await?)?;
            if meta.get("sha256").and_then(|v| v.as_str()) == Some(sha.as_str()) {
                *LAST_SYNCED.lock().unwrap() = sha;
                return Ok(());
            }
        }

        let resp = client
            .post(&url)
            .header("Authorization", &auth)
            .header("Content-Type", "application/octet-stream")
            .body(data)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("HTTP {}: {}", resp.status(), resp.text().await.unwrap_or_default());
        }
        log::info!("inventory portal: wallpaper uploaded (rustdesk_id={})", id);
        *LAST_SYNCED.lock().unwrap() = sha;
        Ok(())
    }
}
