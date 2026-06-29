//! Auto-assign domain PCs to shared address book via `/api/devices/cli` (embedded API token).

use hbb_common::{
    allow_err,
    config::{self, Config, DEFAULT_ASSIGN_API_TOKEN_FROM_BUILD, DEFAULT_PRESET_ADDRESS_BOOK_NAME_FROM_BUILD},
    log,
    tokio,
};
use serde_json::json;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const FIRST_DELAY: Duration = Duration::from_secs(20);
const INTERVAL: Duration = Duration::from_secs(300);
const STATUS_KEY: &str = "ad_ab_assign_alias";

static LOGGED_NO_TOKEN: AtomicBool = AtomicBool::new(false);

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn start() {
    if !config::ad_address_book_features_enabled() {
        return;
    }
    if DEFAULT_ASSIGN_API_TOKEN_FROM_BUILD.is_empty() {
        return;
    }
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
        allow_err!(try_auto_assign_address_book().await);
        tokio::time::sleep(INTERVAL).await;
    }
}

fn assign_api_token() -> &'static str {
    DEFAULT_ASSIGN_API_TOKEN_FROM_BUILD
}

fn address_book_name() -> String {
    let from_config = Config::get_option(config::keys::OPTION_PRESET_ADDRESS_BOOK_NAME);
    if !from_config.is_empty() {
        return from_config;
    }
    DEFAULT_PRESET_ADDRESS_BOOK_NAME_FROM_BUILD.to_owned()
}

/// POST `/api/devices/cli` — same payload as `TnursRemoteDesk.exe --assign`.
pub async fn try_auto_assign_address_book() -> hbb_common::ResultType<()> {
    let token = assign_api_token();
    if token.is_empty() {
        if !LOGGED_NO_TOKEN.swap(true, Ordering::SeqCst) {
            log::info!(
                "ad_ab_assign: токен не задан — авто-привязка к адресной книге выключена."
            );
        }
        return Ok(());
    }

    if config::Config::no_register_device() {
        return Ok(());
    }

    #[cfg(not(windows))]
    {
        let _ = token;
        return Ok(());
    }

    #[cfg(windows)]
    {
        if !crate::platform::is_target_ad_domain() {
            return Ok(());
        }
        if !crate::platform::is_installed() {
            return Ok(());
        }

        let ab_name = address_book_name();
        if ab_name.is_empty() {
            return Ok(());
        }

        let alias = match crate::platform::get_active_user_display_name() {
            Some(a) if !a.is_empty() => a,
            _ => return Ok(()),
        };

        let last = config::Status::get(STATUS_KEY);
        if last == alias {
            return Ok(());
        }

        let api = crate::ui_interface::get_api_server();
        if api.is_empty() || crate::is_public(&api) {
            log::warn!("ad_ab_assign: api-server не настроен");
            return Ok(());
        }
        let url = format!("{}/api/devices/cli", api);

        let body = json!({
            "id": Config::get_id(),
            "uuid": crate::encode64(hbb_common::get_uuid()),
            "address_book_name": ab_name,
            "address_book_alias": alias,
        })
        .to_string();
        let header = format!("Authorization: Bearer {}", token);

        match crate::post_request(url, body, &header).await {
            Ok(text) => {
                let t = text.trim();
                if t.is_empty() || t.eq_ignore_ascii_case("done!") || t.contains("Done") {
                    config::Status::set(STATUS_KEY, alias.clone());
                    log::info!(
                        "ad_ab_assign: устройство привязано к «{}», alias «{}»",
                        ab_name,
                        alias
                    );
                } else {
                    log::warn!("ad_ab_assign: ответ сервера: {}", t);
                }
            }
            Err(e) => log::warn!("ad_ab_assign: запрос не удался: {}", e),
        }
    }

    Ok(())
}
