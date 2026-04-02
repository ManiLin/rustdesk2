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
    if r == "unauthorized" {
        log::warn!(
            "inventory portal: 401 unauthorized — токен на клиенте не совпадает с INVENTORY_DEVICE_TOKEN на сервере \
             (по умолчанию должен совпадать с RS_PUB_KEY в вашей сборке hbb_common)."
        );
        return Ok(());
    }
    if r.contains("\"ok\"") && r.contains("true") {
        log::info!(
            "inventory portal: отчёт принят сервером (rustdesk_id={})",
            rustdesk_id
        );
    } else if r == "db error" {
        log::warn!("inventory portal: сервер вернул ошибку БД");
    } else {
        log::warn!(
            "inventory portal: неожиданный ответ (проверьте URL до /api/v1/report): {}",
            if r.len() > 240 { format!("{}…", &r[..240]) } else { r.to_owned() }
        );
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
