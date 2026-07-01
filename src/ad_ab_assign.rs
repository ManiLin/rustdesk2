//! Auto-assign domain PCs to shared address book via lejianwen rustdesk-api admin endpoints.

use crate::app_build_config::{
    self, DEFAULT_ASSIGN_API_TOKEN_FROM_BUILD, DEFAULT_PRESET_ADDRESS_BOOK_NAME_FROM_BUILD,
};
use hbb_common::{
    allow_err,
    config::{self, Config},
    log,
    tokio,
};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const FIRST_DELAY: Duration = Duration::from_secs(20);
const INTERVAL: Duration = Duration::from_secs(300);
const STATUS_KEY: &str = "ad_ab_assign_alias";
const COLLECTION_CACHE_KEY: &str = "ad_ab_collection_cache";

static LOGGED_NO_TOKEN: AtomicBool = AtomicBool::new(false);
static LOGGED_NO_COLLECTION: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug)]
struct CollectionRef {
    user_id: u64,
    collection_id: u64,
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub fn start() {
    if !app_build_config::ad_address_book_features_enabled() {
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

fn auth_header_json(token: &str) -> String {
    format!(r#"{{"api-token":"{}"}}"#, token)
}

fn parse_http_body(raw: &str) -> hbb_common::ResultType<String> {
    let v: Value = serde_json::from_str(raw)?;
    if let Some(body) = v.get("body").and_then(|b| b.as_str()) {
        Ok(body.to_owned())
    } else {
        Ok(raw.to_owned())
    }
}

fn api_ok(body: &str) -> bool {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v.get("code").and_then(|c| c.as_i64()))
        .map_or(false, |c| c == 0)
}

async fn admin_request(
    method: &str,
    url: String,
    body: Option<String>,
    token: &str,
) -> hbb_common::ResultType<String> {
    let raw = crate::http_request_sync(url, method.to_owned(), body, auth_header_json(token))?;
    parse_http_body(&raw)
}

async fn resolve_collection(api: &str, token: &str, name: &str) -> Option<CollectionRef> {
    if let Some(cached) = read_collection_cache() {
        return Some(cached);
    }
    let url = format!(
        "{}/api/admin/address_book_collection/list?page=1&page_size=500",
        api.trim_end_matches('/')
    );
    let body = match admin_request("get", url, None, token).await {
        Ok(b) => b,
        Err(e) => {
            log::warn!("ad_ab_assign: не удалось получить список коллекций: {}", e);
            return None;
        }
    };
    let v: Value = match serde_json::from_str(&body) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("ad_ab_assign: ответ коллекций не JSON: {} ({})", body, e);
            return None;
        }
    };
    if !api_ok(&body) {
        log::warn!(
            "ad_ab_assign: address_book_collection/list: {}",
            v.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or(&body)
        );
        return None;
    }
    let list = v
        .pointer("/data/list")
        .or_else(|| v.get("list"))
        .and_then(|l| l.as_array());
    let Some(list) = list else {
        log::warn!("ad_ab_assign: пустой список коллекций адресных книг");
        return None;
    };
    for item in list {
        let cname = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if cname.eq_ignore_ascii_case(name) {
            let user_id = item.get("user_id").and_then(|n| n.as_u64()).unwrap_or(0);
            let collection_id = item.get("id").and_then(|n| n.as_u64()).unwrap_or(0);
            if user_id > 0 {
                let found = CollectionRef {
                    user_id,
                    collection_id,
                };
                write_collection_cache(found);
                return Some(found);
            }
        }
    }
    if !LOGGED_NO_COLLECTION.swap(true, Ordering::SeqCst) {
        let names: Vec<_> = list
            .iter()
            .filter_map(|i| i.get("name").and_then(|n| n.as_str()))
            .collect();
        log::warn!(
            "ad_ab_assign: коллекция «{}» не найдена на сервере. Доступные: {:?}",
            name,
            names
        );
    }
    None
}

fn read_collection_cache() -> Option<CollectionRef> {
    let s = config::Status::get(COLLECTION_CACHE_KEY);
    let mut parts = s.split(':');
    let user_id = parts.next()?.parse().ok()?;
    let collection_id = parts.next()?.parse().ok()?;
    if user_id == 0 {
        return None;
    }
    Some(CollectionRef {
        user_id,
        collection_id,
    })
}

fn write_collection_cache(c: CollectionRef) {
    config::Status::set(
        COLLECTION_CACHE_KEY,
        format!("{}:{}", c.user_id, c.collection_id),
    );
}

async fn find_existing_row(
    api: &str,
    token: &str,
    peer_id: &str,
    collection: CollectionRef,
) -> Option<u64> {
    let url = format!(
        "{}/api/admin/address_book/list?page=1&page_size=10&user_id={}&collection_id={}&id={}",
        api.trim_end_matches('/'),
        collection.user_id,
        collection.collection_id,
        peer_id
    );
    let body = admin_request("get", url, None, token).await.ok()?;
    let v: Value = serde_json::from_str(&body).ok()?;
    if !api_ok(&body) {
        return None;
    }
    let list = v.pointer("/data/list").and_then(|l| l.as_array())?;
    list.first()
        .and_then(|i| i.get("row_id"))
        .and_then(|n| n.as_u64())
}

async fn upsert_address_book_entry(
    api: &str,
    token: &str,
    peer_id: &str,
    alias: &str,
    collection: CollectionRef,
) -> hbb_common::ResultType<()> {
    let api = api.trim_end_matches('/');
    if let Some(row_id) = find_existing_row(api, token, peer_id, collection).await {
        let url = format!("{}/api/admin/address_book/update", api);
        let body = json!({
            "row_id": row_id,
            "id": peer_id,
            "user_id": collection.user_id,
            "collection_id": collection.collection_id,
            "alias": alias,
        })
        .to_string();
        let resp = admin_request("post", url, Some(body), token).await?;
        if api_ok(&resp) {
            return Ok(());
        }
        let msg = serde_json::from_str::<Value>(&resp)
            .ok()
            .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_owned()))
            .unwrap_or(resp);
        log::warn!("ad_ab_assign: update: {}", msg);
        return Err(hbb_common::anyhow::anyhow!(msg));
    }

    let url = format!("{}/api/admin/address_book/create", api);
    let username = crate::platform::get_active_username();
    let hostname = crate::common::whoami_hostname();
    let body = json!({
        "id": peer_id,
        "user_id": collection.user_id,
        "collection_id": collection.collection_id,
        "alias": alias,
        "username": username,
        "hostname": hostname,
    })
    .to_string();
    let resp = admin_request("post", url, Some(body), token).await?;
    if api_ok(&resp) {
        return Ok(());
    }
    let msg = serde_json::from_str::<Value>(&resp)
        .ok()
        .and_then(|v| v.get("message").and_then(|m| m.as_str()).map(|s| s.to_owned()))
        .unwrap_or(resp.clone());
    if msg.contains("ItemExists") || msg.contains("exists") {
        return Ok(());
    }
    log::warn!("ad_ab_assign: create: {}", msg);
    Err(hbb_common::anyhow::anyhow!(msg))
}

/// Register this device in the shared address book (lejianwen rustdesk-api admin API).
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

        let Some(collection) = resolve_collection(&api, token, &ab_name).await else {
            return Ok(());
        };

        let peer_id = Config::get_id();
        match upsert_address_book_entry(&api, token, &peer_id, &alias, collection).await {
            Ok(()) => {
                config::Status::set(STATUS_KEY, alias.clone());
                log::info!(
                    "ad_ab_assign: устройство {} добавлено в «{}», alias «{}»",
                    peer_id,
                    ab_name,
                    alias
                );
            }
            Err(e) => log::warn!("ad_ab_assign: не удалось обновить адресную книгу: {}", e),
        }
    }

    Ok(())
}
