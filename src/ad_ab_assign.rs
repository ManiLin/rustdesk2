//! Auto-assign domain PCs to the shared address book.
//!
//! The rustdeskweb admin token is **not** on the client anymore: the client
//! only reports its AD identity to the inventory portal, which performs the
//! address book assignment using its own server-side token.

use crate::app_build_config;
use hbb_common::{
    allow_err,
    config::{self, Config},
    log, tokio,
};
#[cfg(windows)]
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const FIRST_DELAY: Duration = Duration::from_secs(3);
const RETRY_DELAY: Duration = Duration::from_secs(15);
const INTERVAL: Duration = Duration::from_secs(300);
const STATUS_KEY: &str = "ad_ab_assign_alias";

static LOGGED_DISABLED: AtomicBool = AtomicBool::new(false);
static LOGGED_NO_PORTAL: AtomicBool = AtomicBool::new(false);
static LOGGED_NOT_IN_DOMAIN: AtomicBool = AtomicBool::new(false);
static LOGGED_NOT_INSTALLED: AtomicBool = AtomicBool::new(false);
static LOGGED_NO_DISPLAY_NAME: AtomicBool = AtomicBool::new(false);

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn start() {
    if !app_build_config::ad_address_book_features_enabled() {
        if !LOGGED_DISABLED.swap(true, Ordering::SeqCst) {
            log::info!(
                "ad_ab_assign: disabled (cashdesk={}, incoming_only={})",
                app_build_config::is_cashdesk_ui_build(),
                config::is_incoming_only()
            );
        }
        return;
    }
    std::thread::spawn(|| {
        if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            rt.block_on(run_loop());
        }
    });
}

#[cfg(any(target_os = "android", target_os = "ios"))]
pub fn start() {}

async fn run_loop() {
    tokio::time::sleep(FIRST_DELAY).await;
    loop {
        let stop_service = config::option2bool("stop-service", &Config::get_option("stop-service"));
        if stop_service {
            tokio::time::sleep(Duration::from_secs(30)).await;
            continue;
        }
        allow_err!(try_auto_assign_address_book().await);
        let assigned = !config::Status::get(STATUS_KEY).is_empty();
        tokio::time::sleep(if assigned { INTERVAL } else { RETRY_DELAY }).await;
    }
}

/// Portal base URL derived from `inventory-report-url`, e.g. `https://host`.
fn portal_base_url() -> String {
    let report = Config::get_inventory_report_url();
    if report.is_empty() {
        return String::new();
    }
    match report.find("/api/v1/report") {
        Some(idx) => report[..idx].trim_end_matches('/').to_owned(),
        None => report.trim_end_matches('/').to_owned(),
    }
}

fn portal_token() -> String {
    let t = Config::get_option(config::keys::OPTION_INVENTORY_REPORT_TOKEN);
    if t.is_empty() {
        config::DEFAULT_INVENTORY_REPORT_TOKEN.to_owned()
    } else {
        t
    }
}

/// Register this device in the shared address book through the inventory portal.
pub async fn try_auto_assign_address_book() -> hbb_common::ResultType<()> {
    if config::Config::no_register_device() {
        log::info!("ad_ab_assign: skip, device registration is disabled");
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        return Ok(());
    }

    #[cfg(windows)]
    {
        let base = portal_base_url();
        if base.is_empty() {
            if !LOGGED_NO_PORTAL.swap(true, Ordering::SeqCst) {
                log::info!(
                    "ad_ab_assign: skip, inventory portal URL is not configured \
                     (set inventory-report-url or rebuild with INVENTORY_REPORT_URL)"
                );
            }
            return Ok(());
        }

        let in_domain = crate::platform::is_target_ad_domain();
        let installed = crate::platform::is_installed();
        let active_user = crate::platform::get_active_username();
        let display_name = crate::platform::get_active_user_display_name();
        if !in_domain {
            if !LOGGED_NOT_IN_DOMAIN.swap(true, Ordering::SeqCst) {
                log::info!("ad_ab_assign: skip, this PC is not in target AD domain");
            }
            return Ok(());
        }
        if !installed {
            if !LOGGED_NOT_INSTALLED.swap(true, Ordering::SeqCst) {
                log::info!("ad_ab_assign: skip, RustDesk service is not installed");
            }
            return Ok(());
        }

        let alias = match display_name {
            Some(a) if !a.is_empty() => a,
            _ => {
                if !LOGGED_NO_DISPLAY_NAME.swap(true, Ordering::SeqCst) {
                    log::info!(
                        "ad_ab_assign: skip, active AD displayName is empty (active_user=\"{}\")",
                        active_user
                    );
                }
                return Ok(());
            }
        };

        let peer_id = Config::get_id();
        let status_value = format!("{}:{}", base, alias);
        if config::Status::get(STATUS_KEY) == status_value {
            // Already assigned; still revalidate periodically (no-op fast path).
        }

        let url = format!("{}/api/v1/ad/assign", base);
        let body = json!({
            "rustdesk_id": peer_id,
            "ad_domain": app_build_config::DEFAULT_AD_DOMAIN_FROM_BUILD,
            "ad_user": active_user,
            "display_name": alias,
            "username": active_user,
            "hostname": crate::common::whoami_hostname(),
            "platform": "Windows",
        })
        .to_string();
        let auth = format!("Authorization: Bearer {}", portal_token());
        let resp = match crate::post_request(url, body, &auth).await {
            Ok(r) => r,
            Err(e) => {
                log::warn!("ad_ab_assign: portal request failed: {}", e);
                return Ok(());
            }
        };

        let status = serde_json::from_str::<Value>(&resp)
            .ok()
            .and_then(|v| v.get("status").and_then(|s| s.as_str()).map(|s| s.to_owned()))
            .unwrap_or_default();
        match status.as_str() {
            "assigned" => {
                config::Status::set(STATUS_KEY, status_value);
                log::info!("ad_ab_assign: устройство {} добавлено в адресную книгу", peer_id);
            }
            "skipped" => {
                log::debug!("ad_ab_assign: portal skipped: {}", resp);
            }
            "error" => {
                log::warn!("ad_ab_assign: portal error: {}", resp);
            }
            other => {
                log::warn!("ad_ab_assign: неожиданный ответ портала: {} ({})", other, resp);
            }
        }
    }

    Ok(())
}
