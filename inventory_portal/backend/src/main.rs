use axum::{
    body::{to_bytes, Body},
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, Request, State},
    handler::Handler,
    http::{
        header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
        StatusCode,
    },
    middleware::{from_fn, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::env;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

const RUSTDESK_WINDOWS_DOWNLOAD_PATH: &str = "/api/v1/downloads/rustdesk/windows/latest";
const RUSTDESK_WINDOWS_STORED_FILENAME: &str = "rustdesk-windows-latest.exe";
const DEFAULT_MAX_UPLOAD_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    device_token: String,
    admin_password: String,
    jwt_secret: String,
    uploads_dir: PathBuf,
    max_upload_bytes: u64,
    ci_upload_token: String,
    rustdeskweb_api_url: String,
    rustdeskweb_api_token: String,
    ad_domain: String,
    preset_address_book_name: String,
    collection_cache: Arc<tokio::sync::Mutex<Option<CollectionRef>>>,
}

/// Resolved lejianwen rustdesk-api address book collection (user + collection).
#[derive(Clone, Copy, Debug)]
struct CollectionRef {
    user_id: u64,
    collection_id: u64,
}

#[derive(Debug, Deserialize)]
struct AdAssignPayload {
    rustdesk_id: String,
    #[serde(default)]
    ad_domain: String,
    #[serde(default)]
    ad_user: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    hostname: String,
    #[serde(default)]
    platform: String,
}

#[derive(Debug, Serialize)]
struct AdAssignResponse {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct DeviceRow {
    rustdesk_id: String,
    hostname: String,
    os_info: String,
    username: String,
    ip_public: String,
    ip_local: String,
    temporary_password: String,
    computer_summary: String,
    app_version: String,
    updated_at: i64,
    ad_status: Option<String>,
    ad_message: Option<String>,
    ad_updated_at: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ReportPayload {
    #[serde(alias = "id")]
    rustdesk_id: String,
    hostname: Option<String>,
    #[serde(alias = "os_info")]
    os: Option<String>,
    username: Option<String>,
    cpu: Option<String>,
    memory: Option<String>,
    computer_summary: Option<String>,
    ip_public: Option<String>,
    ip_local: Option<String>,
    temporary_password: Option<String>,
    #[serde(alias = "version")]
    app_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceDto {
    rustdesk_id: String,
    hostname: String,
    os_info: String,
    username: String,
    ip_public: String,
    ip_local: String,
    temporary_password: String,
    computer_summary: String,
    app_version: String,
    updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ad_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ad_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ad_updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct ApiError {
    code: &'static str,
    message: String,
}

/// Keep errors machine-readable across handlers and Axum extractor rejections.
async fn json_error_middleware(request: Request<Body>, next: Next) -> Response {
    let response = next.run(request).await;
    let status = response.status();
    if status.is_success() {
        return response;
    }
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap_or_default();
    let body_json = serde_json::from_slice::<serde_json::Value>(&bytes).ok();
    let message = body_json
        .as_ref()
        .and_then(|value| value.get("message"))
        .and_then(|value| value.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| String::from_utf8_lossy(&bytes).trim().to_owned());
    let code = match status {
        StatusCode::BAD_REQUEST => "bad_request",
        StatusCode::UNPROCESSABLE_ENTITY => "validation_error",
        StatusCode::UNAUTHORIZED => "unauthorized",
        StatusCode::FORBIDDEN => "forbidden",
        StatusCode::NOT_FOUND => "not_found",
        StatusCode::METHOD_NOT_ALLOWED => "method_not_allowed",
        StatusCode::UNSUPPORTED_MEDIA_TYPE => "unsupported_media_type",
        StatusCode::CONFLICT => "conflict",
        StatusCode::PAYLOAD_TOO_LARGE => "payload_too_large",
        StatusCode::SERVICE_UNAVAILABLE => "service_unavailable",
        StatusCode::BAD_GATEWAY => "bad_gateway",
        _ => "internal_server_error",
    };
    let message = if message.is_empty() {
        status
            .canonical_reason()
            .unwrap_or("Request failed")
            .to_owned()
    } else {
        message
    };
    (status, Json(ApiError { code, message })).into_response()
}

#[derive(Debug, Deserialize)]
struct LoginBody {
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
    expires_in: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
}

#[derive(Debug, sqlx::FromRow)]
struct BuildRow {
    id: i64,
    flavor: String,
    platform: String,
    version: String,
    file_name: String,
    file_path: String,
    file_size: i64,
    sha256: String,
    status: String,
    uploaded_at: i64,
    approved_at: Option<i64>,
    approved_by: String,
}

#[derive(Debug, Serialize)]
struct BuildDto {
    id: i64,
    flavor: String,
    platform: String,
    version: String,
    file_name: String,
    file_size: i64,
    sha256: String,
    status: String,
    uploaded_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    approved_at: Option<String>,
    approved_by: String,
}

impl From<BuildRow> for BuildDto {
    fn from(r: BuildRow) -> Self {
        Self {
            id: r.id,
            flavor: r.flavor,
            platform: r.platform,
            version: r.version,
            file_name: r.file_name,
            file_size: r.file_size,
            sha256: r.sha256,
            status: r.status,
            uploaded_at: format_ts(r.uploaded_at),
            approved_at: r.approved_at.map(format_ts),
            approved_by: r.approved_by,
        }
    }
}

#[derive(Debug, Serialize)]
struct BuildMetaDto {
    available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    download_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha256: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BuildQuery {
    #[serde(default)]
    flavor: String,
    #[serde(default)]
    platform: String,
}

/// Сервер из общего реестра (`servers`): источник выпадающего списка в клиенте.
#[derive(Debug, sqlx::FromRow)]
struct ServerRow {
    id: i64,
    name: String,
    host: String,
    public_key: String,
    created_by: String,
    created_at: i64,
    updated_at: i64,
}

#[derive(Debug, Serialize)]
struct ServerDto {
    id: i64,
    name: String,
    host: String,
    public_key: String,
    created_by: String,
    created_at: String,
    updated_at: String,
}

impl From<ServerRow> for ServerDto {
    fn from(r: ServerRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            host: r.host,
            public_key: r.public_key,
            created_by: r.created_by,
            created_at: format_ts(r.created_at),
            updated_at: format_ts(r.updated_at),
        }
    }
}

#[derive(Debug, Deserialize)]
struct ServerPayload {
    #[serde(default)]
    name: String,
    host: String,
    /// Public key сервера; принимаем и короткое имя `key`.
    #[serde(default, alias = "key")]
    public_key: String,
}

const SERVERS_COLUMNS: &str = "id, name, host, public_key, created_by, created_at, updated_at";

/// Приводит адрес сервера к виду `host:port`; схема, путь и пробелы недопустимы.
fn normalize_server_host(raw: &str) -> Option<String> {
    let h = raw.trim();
    if h.is_empty() || h.contains('/') || h.chars().any(char::is_whitespace) {
        return None;
    }
    if let Ok(address) = h.parse::<std::net::SocketAddr>() {
        return Some(address.to_string());
    }
    let (hostname, port) = h.split_once(':')?;
    if hostname.is_empty() || hostname.contains(':') {
        return None;
    }
    let port = port.parse::<u16>().ok()?;
    if port == 0
        || hostname.starts_with('.')
        || hostname.ends_with('.')
        || hostname.split('.').any(|label| {
            label.is_empty()
                || label.starts_with('-')
                || label.ends_with('-')
                || !label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
    {
        return None;
    }
    Some(format!("{}:{port}", hostname.to_ascii_lowercase()))
}

/// RAII-защитник: гарантирует удаление временного файла, если обработка прервалась
struct TempFileGuard {
    path: PathBuf,
    active: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, active: true }
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.active {
            let p = self.path.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::remove_file(p).await;
            });
        }
    }
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn format_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|d| d.to_rfc3339())
        .unwrap_or_default()
}

const BUILDS_COLUMNS: &str = "id, flavor, platform, version, file_name, file_path, file_size, sha256, status, uploaded_at, approved_at, approved_by";

fn norm_flavor(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_lowercase().as_str() {
        "" | "normal" => Some("normal"),
        "cashdesk" => Some("cashdesk"),
        _ => None,
    }
}

fn norm_platform(s: &str) -> Option<&'static str> {
    match s.trim().to_ascii_lowercase().as_str() {
        "" | "windows" => Some("windows"),
        "linux" => Some("linux"),
        "macos" | "mac" | "darwin" => Some("macos"),
        "android" => Some("android"),
        _ => None,
    }
}

fn build_download_path(flavor: &str) -> String {
    format!("{}?flavor={}", RUSTDESK_WINDOWS_DOWNLOAD_PATH, flavor)
}

fn extension_allowed(platform: &str, filename: &str) -> bool {
    let ext = Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match platform {
        "windows" => ext == "exe",
        "android" => ext == "apk",
        "macos" => matches!(ext.as_str(), "dmg" | "pkg"),
        "linux" => matches!(ext.as_str(), "deb" | "rpm" | "zst" | "appimage" | "gz"),
        _ => true,
    }
}

fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("build.bin");
    let cleaned: String = base
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "build.bin".to_string()
    } else {
        cleaned
    }
}

async fn sha256_file(path: &Path) -> anyhow::Result<String> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

async fn get_published_build(pool: &SqlitePool, flavor: &str, platform: &str) -> Option<BuildRow> {
    sqlx::query_as::<_, BuildRow>(&format!(
        "SELECT {BUILDS_COLUMNS} FROM builds WHERE status = 'published' AND flavor = ? AND platform = ? ORDER BY approved_at DESC, id DESC LIMIT 1"
    ))
    .bind(flavor)
    .bind(platform)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
}

async fn get_build(pool: &SqlitePool, id: i64) -> Option<BuildRow> {
    sqlx::query_as::<_, BuildRow>(&format!("SELECT {BUILDS_COLUMNS} FROM builds WHERE id = ?"))
        .bind(id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
}

/// Migrate the legacy single-file release (`rustdesk_windows_release` + fixed
/// filename) into the builds table as an already-published normal/windows build.
async fn migrate_legacy_release(pool: &SqlitePool, uploads_dir: &Path) {
    let legacy_path = uploads_dir.join(RUSTDESK_WINDOWS_STORED_FILENAME);
    let Ok(Some(version)) = sqlx::query_scalar::<_, String>(
        "SELECT version FROM rustdesk_windows_release WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    else {
        return;
    };
    let version = version.trim().to_string();
    if version.is_empty() {
        return;
    }
    let has_published: Option<i64> = sqlx::query_scalar(
        "SELECT id FROM builds WHERE flavor = 'normal' AND platform = 'windows' AND status = 'published' LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if has_published.is_some() || !tokio::fs::try_exists(&legacy_path).await.unwrap_or(false) {
        return;
    }
    let size = tokio::fs::metadata(&legacy_path)
        .await
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    let sha = sha256_file(&legacy_path).await.unwrap_or_default();
    let ts = now_ts();
    let r = sqlx::query(
        "INSERT INTO builds (flavor, platform, version, file_name, file_path, file_size, sha256, status, uploaded_at, approved_at, approved_by) \
         VALUES ('normal', 'windows', ?, 'rustdesk.exe', ?, ?, ?, 'published', ?, ?, 'legacy')",
    )
    .bind(&version)
    .bind(legacy_path.to_string_lossy().to_string())
    .bind(size)
    .bind(&sha)
    .bind(ts)
    .bind(ts)
    .execute(pool)
    .await;
    match r {
        Ok(_) => tracing::info!(version = %version, "migrated legacy release into builds"),
        Err(e) => tracing::warn!("legacy release migration failed: {}", e),
    }
}

/// Извлекает токен из заголовка `Authorization: Bearer <token>`.
fn extract_bearer_token(auth_header: Option<&str>) -> Option<&str> {
    let h = auth_header?.trim();
    if let Some(t) = h.strip_prefix("Bearer ") {
        Some(t.trim())
    } else if let Some(t) = h.strip_prefix("bearer ") {
        Some(t.trim())
    } else {
        None
    }
}

fn verify_device_token(state: &AppState, auth_header: Option<&str>) -> bool {
    let Some(t) = extract_bearer_token(auth_header) else {
        return false;
    };
    constant_time_eq(t, &state.device_token)
}

fn admin_auth_error(state: &AppState, auth_header: Option<&str>) -> Response {
    if verify_device_token(state, auth_header) {
        (StatusCode::FORBIDDEN, "administrator access required").into_response()
    } else {
        (StatusCode::UNAUTHORIZED, "unauthorized").into_response()
    }
}

/// Constant-time byte comparison to avoid leaking the token via timing.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

fn verify_admin_jwt(state: &AppState, auth_header: Option<&str>) -> bool {
    let Some(token) = extract_bearer_token(auth_header) else {
        return false;
    };
    let key = DecodingKey::from_secret(state.jwt_secret.as_bytes());
    decode::<Claims>(token, &key, &Validation::default()).is_ok()
}

fn issue_jwt(state: &AppState) -> anyhow::Result<(String, i64)> {
    let exp = now_ts() + 86400 * 7;
    let claims = Claims {
        sub: "admin".into(),
        exp: exp as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )?;
    Ok((token, 86400 * 7))
}

async fn report_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(p): Json<ReportPayload>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_device_token(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let rustdesk_id = p.rustdesk_id.trim();
    if rustdesk_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "rustdesk_id required").into_response();
    }
    let os_info = p.os.clone().unwrap_or_default();
    let computer_summary = p.computer_summary.clone().unwrap_or_else(|| {
        format!(
            "{} | {} | {}",
            p.cpu.clone().unwrap_or_default(),
            p.memory.clone().unwrap_or_default(),
            p.os.clone().unwrap_or_default()
        )
    });
    let ts = now_ts();
    let r = sqlx::query(
        r#"
        INSERT INTO devices (
            rustdesk_id, hostname, os_info, username, ip_public, ip_local,
            temporary_password, computer_summary, app_version, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(rustdesk_id) DO UPDATE SET
            hostname = excluded.hostname,
            os_info = excluded.os_info,
            username = excluded.username,
            ip_public = excluded.ip_public,
            ip_local = excluded.ip_local,
            temporary_password = excluded.temporary_password,
            computer_summary = excluded.computer_summary,
            app_version = excluded.app_version,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(rustdesk_id)
    .bind(p.hostname.clone().unwrap_or_default())
    .bind(&os_info)
    .bind(p.username.clone().unwrap_or_default())
    .bind(p.ip_public.clone().unwrap_or_default())
    .bind(p.ip_local.clone().unwrap_or_default())
    .bind(p.temporary_password.clone().unwrap_or_default())
    .bind(&computer_summary)
    .bind(p.app_version.clone().unwrap_or_default())
    .bind(ts)
    .execute(&state.pool)
    .await;

    match r {
        Ok(_) => (StatusCode::OK, Json(serde_json::json!({"ok": true}))).into_response(),
        Err(e) => {
            tracing::error!("db error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// AD -> shared address book via lejianwen rustdesk-api admin endpoints.
// The rustdeskweb admin token lives only here, never on clients.
// ---------------------------------------------------------------------------

fn rustdeskweb_code_ok(v: &serde_json::Value) -> bool {
    v.get("code").and_then(|c| c.as_i64()) == Some(0)
}

fn rustdeskweb_message(v: &serde_json::Value) -> String {
    v.get("message")
        .and_then(|m| m.as_str())
        .map(|s| s.to_owned())
        .unwrap_or_default()
}

fn short(s: &str) -> String {
    const MAX: usize = 300;
    if s.chars().count() <= MAX {
        s.to_owned()
    } else {
        format!("{}…", s.chars().take(MAX).collect::<String>())
    }
}

async fn rustdeskweb_request(
    state: &AppState,
    method: reqwest::Method,
    path: &str,
    body: Option<serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    if state.rustdeskweb_api_token.is_empty() {
        anyhow::bail!("rustdeskweb token is not configured");
    }
    let url = format!(
        "{}{}",
        state.rustdeskweb_api_url.trim_end_matches('/'),
        path
    );
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?;
    let mut req = client
        .request(method, &url)
        .header("api-token", &state.rustdeskweb_api_token);
    if let Some(body) = body {
        req = req.json(&body);
    }
    let resp = req.send().await?;
    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("rustdeskweb HTTP {}: {}", status, short(&text));
    }
    serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|e| anyhow::anyhow!("rustdeskweb invalid JSON: {} ({})", e, short(&text)))
}

async fn resolve_collection(state: &AppState) -> anyhow::Result<CollectionRef> {
    if let Some(c) = *state.collection_cache.lock().await {
        return Ok(c);
    }
    let name = state.preset_address_book_name.trim();
    if name.is_empty() {
        anyhow::bail!("preset address book name is empty");
    }
    let v = rustdeskweb_request(
        state,
        reqwest::Method::GET,
        "/api/admin/address_book_collection/list?page=1&page_size=500",
        None,
    )
    .await?;
    if !rustdeskweb_code_ok(&v) {
        anyhow::bail!("collection list failed: {}", rustdeskweb_message(&v));
    }
    let list = v
        .pointer("/data/list")
        .or_else(|| v.get("list"))
        .and_then(|l| l.as_array())
        .cloned()
        .unwrap_or_default();
    for item in &list {
        let cname = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if cname.eq_ignore_ascii_case(name) {
            let user_id = item.get("user_id").and_then(|n| n.as_u64()).unwrap_or(0);
            let collection_id = item.get("id").and_then(|n| n.as_u64()).unwrap_or(0);
            if user_id > 0 {
                let found = CollectionRef {
                    user_id,
                    collection_id,
                };
                *state.collection_cache.lock().await = Some(found);
                tracing::info!(
                    name,
                    user_id,
                    collection_id,
                    "ad assign: collection resolved"
                );
                return Ok(found);
            }
        }
    }
    let names: Vec<&str> = list
        .iter()
        .filter_map(|i| i.get("name").and_then(|n| n.as_str()))
        .collect();
    anyhow::bail!("collection '{}' not found; available: {:?}", name, names);
}

async fn upsert_address_book_entry(
    state: &AppState,
    peer_id: &str,
    payload: &AdAssignPayload,
    collection: CollectionRef,
) -> anyhow::Result<()> {
    let alias = payload.display_name.trim();
    if alias.is_empty() {
        anyhow::bail!("display name is empty");
    }
    let list_path = format!(
        "/api/admin/address_book/list?page=1&page_size=10&user_id={}&collection_id={}&id={}",
        collection.user_id, collection.collection_id, peer_id
    );
    let v = rustdeskweb_request(state, reqwest::Method::GET, &list_path, None).await?;
    let row_id = if rustdeskweb_code_ok(&v) {
        v.pointer("/data/list")
            .and_then(|l| l.as_array())
            .and_then(|l| l.first())
            .and_then(|i| i.get("row_id"))
            .and_then(|n| n.as_u64())
    } else {
        None
    };

    if let Some(row_id) = row_id {
        let body = serde_json::json!({
            "row_id": row_id,
            "id": peer_id,
            "user_id": collection.user_id,
            "collection_id": collection.collection_id,
            "alias": alias,
        });
        let v = rustdeskweb_request(
            state,
            reqwest::Method::POST,
            "/api/admin/address_book/update",
            Some(body),
        )
        .await?;
        if rustdeskweb_code_ok(&v) {
            return Ok(());
        }
        anyhow::bail!("update failed: {}", rustdeskweb_message(&v));
    }

    let body = serde_json::json!({
        "id": peer_id,
        "user_id": collection.user_id,
        "collection_id": collection.collection_id,
        "alias": alias,
        "username": payload.username,
        "hostname": payload.hostname,
        "platform": payload.platform,
    });
    let v = rustdeskweb_request(
        state,
        reqwest::Method::POST,
        "/api/admin/address_book/create",
        Some(body),
    )
    .await?;
    if rustdeskweb_code_ok(&v) {
        return Ok(());
    }
    let msg = rustdeskweb_message(&v);
    if msg.contains("ItemExists") || msg.contains("exists") {
        return Ok(());
    }
    anyhow::bail!("create failed: {}", msg);
}

async fn record_ad_assignment(state: &AppState, p: &AdAssignPayload, status: &str, message: &str) {
    let r = sqlx::query(
        r#"
        INSERT INTO ad_assignments (rustdesk_id, alias, ad_user, status, message, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(rustdesk_id) DO UPDATE SET
            alias = excluded.alias,
            ad_user = excluded.ad_user,
            status = excluded.status,
            message = excluded.message,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(p.rustdesk_id.trim())
    .bind(p.display_name.trim())
    .bind(p.ad_user.trim())
    .bind(status)
    .bind(message)
    .bind(now_ts())
    .execute(&state.pool)
    .await;
    if let Err(e) = r {
        tracing::warn!("ad assign: failed to store status: {}", e);
    }
}

async fn ad_assign_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(p): Json<AdAssignPayload>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_device_token(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    if p.rustdesk_id.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "rustdesk_id required").into_response();
    }
    if state.rustdeskweb_api_token.is_empty() {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(AdAssignResponse {
                status: "disabled",
                message: Some("rustdeskweb token is not configured".into()),
            }),
        )
            .into_response();
    }
    if !p.ad_domain.trim().is_empty()
        && !p
            .ad_domain
            .trim()
            .eq_ignore_ascii_case(state.ad_domain.trim())
    {
        record_ad_assignment(&state, &p, "skipped", "not in target AD domain").await;
        return Json(AdAssignResponse {
            status: "skipped",
            message: Some("not in target AD domain".into()),
        })
        .into_response();
    }
    if p.display_name.trim().is_empty() {
        record_ad_assignment(&state, &p, "skipped", "empty display name").await;
        return Json(AdAssignResponse {
            status: "skipped",
            message: Some("empty display name".into()),
        })
        .into_response();
    }

    let result = async {
        let collection = resolve_collection(&state).await?;
        upsert_address_book_entry(&state, p.rustdesk_id.trim(), &p, collection).await
    }
    .await;

    match result {
        Ok(()) => {
            record_ad_assignment(&state, &p, "assigned", "").await;
            tracing::info!(peer = %p.rustdesk_id, alias = %p.display_name, "ad assign: assigned");
            Json(AdAssignResponse {
                status: "assigned",
                message: None,
            })
            .into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            record_ad_assignment(&state, &p, "error", &msg).await;
            tracing::warn!(peer = %p.rustdesk_id, "ad assign failed: {}", msg);
            (
                StatusCode::BAD_GATEWAY,
                Json(ApiError {
                    code: "bad_gateway",
                    message: msg,
                }),
            )
                .into_response()
        }
    }
}

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> impl IntoResponse {
    if body.password != state.admin_password {
        return (StatusCode::UNAUTHORIZED, "bad password").into_response();
    }
    match issue_jwt(&state) {
        Ok((token, expires_in)) => {
            (StatusCode::OK, Json(LoginResponse { token, expires_in })).into_response()
        }
        Err(e) => {
            tracing::error!("jwt: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "token error").into_response()
        }
    }
}

async fn devices_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let rows: Result<Vec<DeviceRow>, _> = sqlx::query_as(
        r#"
        SELECT
            rustdesk_id,
            COALESCE(hostname, '') AS hostname,
            COALESCE(os_info, '') AS os_info,
            COALESCE(username, '') AS username,
            COALESCE(ip_public, '') AS ip_public,
            COALESCE(ip_local, '') AS ip_local,
            COALESCE(temporary_password, '') AS temporary_password,
            COALESCE(computer_summary, '') AS computer_summary,
            COALESCE(app_version, '') AS app_version,
            COALESCE(devices.updated_at, 0) AS updated_at,
            ad_assignments.status AS ad_status,
            ad_assignments.message AS ad_message,
            ad_assignments.updated_at AS ad_updated_at
        FROM devices
        LEFT JOIN ad_assignments USING (rustdesk_id)
        ORDER BY updated_at DESC
        "#,
    )
    .fetch_all(&state.pool)
    .await;

    match rows {
        Ok(list) => {
            let out: Vec<DeviceDto> = list
                .into_iter()
                .map(|r| DeviceDto {
                    rustdesk_id: r.rustdesk_id,
                    hostname: r.hostname,
                    os_info: r.os_info,
                    username: r.username,
                    ip_public: r.ip_public,
                    ip_local: r.ip_local,
                    temporary_password: r.temporary_password,
                    computer_summary: r.computer_summary,
                    app_version: r.app_version,
                    updated_at: chrono::DateTime::from_timestamp(r.updated_at, 0)
                        .map(|d| d.to_rfc3339())
                        .unwrap_or_default(),
                    ad_status: r.ad_status.filter(|s| !s.is_empty()),
                    ad_message: r.ad_message.filter(|s| !s.is_empty()),
                    ad_updated_at: r.ad_updated_at.and_then(|ts| {
                        chrono::DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339())
                    }),
                })
                .collect();
            Json(out).into_response()
        }
        Err(e) => {
            tracing::error!("list: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

async fn delete_device_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    AxumPath(id): AxumPath<String>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let trimmed_id = id.trim();
    let r = sqlx::query("DELETE FROM devices WHERE rustdesk_id = ?")
        .bind(trimmed_id)
        .execute(&state.pool)
        .await;

    match r {
        Ok(res) => {
            if res.rows_affected() == 0 {
                (StatusCode::NOT_FOUND, "device not found").into_response()
            } else {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({"ok": true, "deleted": trimmed_id})),
                )
                    .into_response()
            }
        }
        Err(e) => {
            tracing::error!("delete device error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

/// Доступ к реестру серверов: админский JWT или device-токен клиента.
fn verify_device_or_admin(state: &AppState, auth_header: Option<&str>) -> bool {
    verify_admin_jwt(state, auth_header) || verify_device_token(state, auth_header)
}

/// Общий реестр RustDesk-серверов: клиенты тянут его для выпадающего списка.
async fn list_servers_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_device_or_admin(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let rows = sqlx::query_as::<_, ServerRow>(&format!(
        "SELECT {SERVERS_COLUMNS} FROM servers ORDER BY name COLLATE NOCASE ASC, host ASC"
    ))
    .fetch_all(&state.pool)
    .await;
    match rows {
        Ok(list) => {
            let out: Vec<ServerDto> = list.into_iter().map(ServerDto::from).collect();
            Json(out).into_response()
        }
        Err(e) => {
            tracing::error!("list servers: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

/// Добавляет сервер в общий реестр или обновляет существующий (по `host`); только администратор.
async fn upsert_server_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    Json(p): Json<ServerPayload>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let Some(host) = normalize_server_host(&p.host) else {
        return (
            StatusCode::BAD_REQUEST,
            "invalid server host, use host:port",
        )
            .into_response();
    };
    let public_key = p.public_key.trim();
    if public_key.is_empty() {
        return (StatusCode::BAD_REQUEST, "public key is required").into_response();
    }
    let name = p.name.trim();
    let name = if name.is_empty() {
        host.clone()
    } else {
        name.to_string()
    };
    let ts = now_ts();
    let r = sqlx::query(
        r#"
        INSERT INTO servers (name, host, public_key, created_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(host) DO UPDATE SET
            name = excluded.name,
            public_key = excluded.public_key,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&name)
    .bind(&host)
    .bind(public_key)
    .bind("admin")
    .bind(ts)
    .bind(ts)
    .execute(&state.pool)
    .await;
    if let Err(e) = r {
        if e.as_database_error()
            .map(|error| error.is_unique_violation())
            .unwrap_or(false)
        {
            return (
                StatusCode::CONFLICT,
                "another server already uses this host",
            )
                .into_response();
        }
        tracing::error!("upsert server: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
    }
    tracing::info!(host = %host, name = %name, "server registered in shared list");
    let row = sqlx::query_as::<_, ServerRow>(&format!(
        "SELECT {SERVERS_COLUMNS} FROM servers WHERE host = ?"
    ))
    .bind(&host)
    .fetch_optional(&state.pool)
    .await;
    match row {
        Ok(Some(s)) => (StatusCode::OK, Json(ServerDto::from(s))).into_response(),
        Ok(None) => (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response(),
        Err(e) => {
            tracing::error!("read back server: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

/// Удаляет сервер из реестра (только админ).
async fn delete_server_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    AxumPath(id): AxumPath<i64>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let r = sqlx::query("DELETE FROM servers WHERE id = ?")
        .bind(id)
        .execute(&state.pool)
        .await;
    match r {
        Ok(res) => {
            if res.rows_affected() == 0 {
                (StatusCode::NOT_FOUND, "server not found").into_response()
            } else {
                (
                    StatusCode::OK,
                    Json(serde_json::json!({"ok": true, "deleted": id})),
                )
                    .into_response()
            }
        }
        Err(e) => {
            tracing::error!("delete server: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

fn verify_ci_token(state: &AppState, auth_header: Option<&str>) -> bool {
    let Some(t) = extract_bearer_token(auth_header) else {
        return false;
    };
    let expected = if state.ci_upload_token.is_empty() {
        &state.device_token
    } else {
        &state.ci_upload_token
    };
    constant_time_eq(t, expected)
}

async fn admin_list_builds_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let rows = sqlx::query_as::<_, BuildRow>(&format!(
        "SELECT {BUILDS_COLUMNS} FROM builds ORDER BY id DESC"
    ))
    .fetch_all(&state.pool)
    .await;
    match rows {
        Ok(list) => {
            let out: Vec<BuildDto> = list.into_iter().map(BuildDto::from).collect();
            Json(out).into_response()
        }
        Err(e) => {
            tracing::error!("list builds: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response()
        }
    }
}

/// Shared multipart build upload for admin and CI. Always creates a `pending`
/// build; distribution requires an explicit admin approval.
async fn handle_build_upload(
    state: &AppState,
    mut multipart: Multipart,
    uploaded_by: &str,
) -> Response {
    let temp_name = format!(".build-upload-{}-{}.tmp", now_ts(), std::process::id());
    let temp_path = state.uploads_dir.join(&temp_name);
    let mut guard = TempFileGuard::new(temp_path.clone());
    let mut form_version = String::new();
    let mut form_flavor = String::new();
    let mut form_platform = String::new();
    let mut form_sha = String::new();
    let mut uploaded_filename = None::<String>;
    let mut total_size = 0_u64;
    let mut file_written = false;

    loop {
        let next_field = match multipart.next_field().await {
            Ok(field) => field,
            Err(e) => {
                tracing::warn!("multipart error: {}", e);
                return (StatusCode::BAD_REQUEST, "invalid multipart payload").into_response();
            }
        };
        let Some(mut field) = next_field else {
            break;
        };
        match field.name() {
            Some("version") => form_version = field.text().await.unwrap_or_default(),
            Some("flavor") => form_flavor = field.text().await.unwrap_or_default(),
            Some("platform") => form_platform = field.text().await.unwrap_or_default(),
            Some("sha256") => form_sha = field.text().await.unwrap_or_default(),
            Some("file") => {
                let filename = field.file_name().unwrap_or("build.bin").to_string();
                let mut file = match tokio::fs::File::create(&temp_path).await {
                    Ok(file) => file,
                    Err(e) => {
                        tracing::error!("create temp upload file: {}", e);
                        return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
                    }
                };
                loop {
                    let chunk = match field.chunk().await {
                        Ok(chunk) => chunk,
                        Err(e) => {
                            tracing::warn!("multipart chunk error: {}", e);
                            return (StatusCode::BAD_REQUEST, "invalid upload chunk")
                                .into_response();
                        }
                    };
                    let Some(chunk) = chunk else {
                        break;
                    };
                    total_size += chunk.len() as u64;
                    if total_size > state.max_upload_bytes {
                        return (StatusCode::PAYLOAD_TOO_LARGE, "file is too large")
                            .into_response();
                    }
                    if let Err(e) = file.write_all(&chunk).await {
                        tracing::error!("write upload file: {}", e);
                        return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
                    }
                }
                if let Err(e) = file.flush().await {
                    tracing::error!("flush upload file: {}", e);
                    return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
                }
                uploaded_filename = Some(filename);
                file_written = true;
            }
            _ => {}
        }
    }

    if !file_written || total_size == 0 {
        return (StatusCode::BAD_REQUEST, "file is required").into_response();
    }
    let version = form_version.trim().to_string();
    if version.is_empty() {
        return (StatusCode::BAD_REQUEST, "version is required").into_response();
    }
    if form_flavor.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "flavor is required").into_response();
    }
    if form_platform.trim().is_empty() {
        return (StatusCode::BAD_REQUEST, "platform is required").into_response();
    }
    let Some(flavor) = norm_flavor(&form_flavor) else {
        return (StatusCode::BAD_REQUEST, "flavor must be normal or cashdesk").into_response();
    };
    let Some(platform) = norm_platform(&form_platform) else {
        return (
            StatusCode::BAD_REQUEST,
            "platform must be windows, linux, macos, or android",
        )
            .into_response();
    };
    let original = uploaded_filename.as_deref().unwrap_or("build.bin");
    if !extension_allowed(platform, original) {
        return (
            StatusCode::BAD_REQUEST,
            "unsupported file extension for platform",
        )
            .into_response();
    }

    let sha = match sha256_file(&temp_path).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("sha256: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "hash error").into_response();
        }
    };
    if !form_sha.trim().is_empty() && !form_sha.trim().eq_ignore_ascii_case(&sha) {
        return (StatusCode::BAD_REQUEST, "sha256 mismatch").into_response();
    }

    let builds_dir = state.uploads_dir.join("builds");
    if let Err(e) = tokio::fs::create_dir_all(&builds_dir).await {
        tracing::error!("create builds dir: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
    }
    let stored_name = format!(
        "{}-{}-{}",
        now_ts(),
        std::process::id(),
        sanitize_filename(original)
    );
    let final_path = builds_dir.join(&stored_name);
    if let Err(e) = tokio::fs::rename(&temp_path, &final_path).await {
        tracing::error!("move build upload: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
    }

    // Успешно переместили во final_path — отключаем удаление guard'ом
    guard.disarm();

    let r = sqlx::query(
        "INSERT INTO builds (flavor, platform, version, file_name, file_path, file_size, sha256, status, uploaded_at, approved_by) \
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?, '')",
    )
    .bind(flavor)
    .bind(platform)
    .bind(&version)
    .bind(original)
    .bind(final_path.to_string_lossy().to_string())
    .bind(total_size as i64)
    .bind(&sha)
    .bind(now_ts())
    .execute(&state.pool)
    .await;
    let id = match r {
        Ok(res) => res.last_insert_rowid(),
        Err(e) => {
            tracing::error!("insert build: {}", e);
            let _ = tokio::fs::remove_file(&final_path).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
        }
    };
    tracing::info!(
        id,
        flavor,
        platform,
        version = %version,
        bytes = total_size,
        uploaded_by,
        "build uploaded (pending approval)"
    );
    match get_build(&state.pool, id).await {
        Some(b) => (StatusCode::OK, Json(BuildDto::from(b))).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response(),
    }
}

async fn admin_upload_build_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    multipart: Multipart,
) -> Response {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    handle_build_upload(&state, multipart, "admin").await
}

async fn ci_upload_build_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    multipart: Multipart,
) -> Response {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_ci_token(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    handle_build_upload(&state, multipart, "ci").await
}

async fn admin_approve_build_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    AxumPath(id): AxumPath<i64>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let Some(build) = get_build(&state.pool, id).await else {
        return (StatusCode::NOT_FOUND, "build not found").into_response();
    };
    if build.status != "pending" {
        return (StatusCode::CONFLICT, "only pending builds can be approved").into_response();
    }
    let mut tx = match state.pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("begin build approval: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
        }
    };
    if let Err(e) = sqlx::query(
        "UPDATE builds SET status = 'archived' WHERE flavor = ? AND platform = ? AND status = 'published' AND id != ?",
    )
    .bind(&build.flavor)
    .bind(&build.platform)
    .bind(id)
    .execute(&mut *tx)
    .await
    {
        tracing::error!("archive old builds: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
    }
    let updated = sqlx::query(
        "UPDATE builds SET status = 'published', approved_at = ?, approved_by = 'admin' WHERE id = ? AND status = 'pending'",
    )
    .bind(now_ts())
    .bind(id)
    .execute(&mut *tx)
    .await;
    match updated {
        Ok(result) if result.rows_affected() == 1 => {}
        Ok(_) => {
            return (StatusCode::CONFLICT, "build is no longer pending").into_response();
        }
        Err(e) => {
            tracing::error!("approve build: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
        }
    }
    if let Err(e) = tx.commit().await {
        tracing::error!("commit build approval: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
    }
    tracing::info!(id, "build approved");
    match get_build(&state.pool, id).await {
        Some(b) => Json(BuildDto::from(b)).into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response(),
    }
}

async fn admin_reject_build_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    AxumPath(id): AxumPath<i64>,
) -> impl IntoResponse {
    let auth = headers.get(AUTHORIZATION).and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return admin_auth_error(&state, auth);
    }
    let result =
        sqlx::query("UPDATE builds SET status = 'rejected' WHERE id = ? AND status = 'pending'")
            .bind(id)
            .execute(&state.pool)
            .await;
    match result {
        Ok(result) if result.rows_affected() == 0 => {
            return if get_build(&state.pool, id).await.is_some() {
                (StatusCode::CONFLICT, "only pending builds can be rejected").into_response()
            } else {
                (StatusCode::NOT_FOUND, "build not found").into_response()
            };
        }
        Err(e) => {
            tracing::error!("reject build: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "db error").into_response();
        }
        _ => {}
    }
    match get_build(&state.pool, id).await {
        Some(b) => Json(BuildDto::from(b)).into_response(),
        None => (StatusCode::NOT_FOUND, "build not found").into_response(),
    }
}

async fn public_software_update_meta_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BuildQuery>,
) -> impl IntoResponse {
    let Some(flavor) = norm_flavor(&q.flavor) else {
        return (StatusCode::BAD_REQUEST, "flavor must be normal or cashdesk").into_response();
    };
    let Some(platform) = norm_platform(&q.platform) else {
        return (StatusCode::BAD_REQUEST, "unsupported platform").into_response();
    };
    let download_path = build_download_path(flavor);
    match get_published_build(&state.pool, flavor, platform).await {
        Some(b) if tokio::fs::try_exists(&b.file_path).await.unwrap_or(false) => {
            Json(BuildMetaDto {
                available: true,
                version: Some(b.version),
                download_path,
                sha256: (!b.sha256.is_empty()).then_some(b.sha256),
            })
            .into_response()
        }
        _ => Json(BuildMetaDto {
            available: false,
            version: None,
            download_path,
            sha256: None,
        })
        .into_response(),
    }
}

async fn public_download_rustdesk_head_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BuildQuery>,
) -> Response {
    let Some(flavor) = norm_flavor(&q.flavor) else {
        return (StatusCode::BAD_REQUEST, "flavor must be normal or cashdesk").into_response();
    };
    let Some(platform) = norm_platform(&q.platform) else {
        return (StatusCode::BAD_REQUEST, "unsupported platform").into_response();
    };
    let Some(build) = get_published_build(&state.pool, flavor, platform).await else {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    };
    let path = PathBuf::from(&build.file_path);
    let Ok(metadata) = tokio::fs::metadata(&path).await else {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    };
    let filename = sanitize_filename(&build.file_name);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(CONTENT_LENGTH, metadata.len().to_string())
        .header("Cache-Control", "no-store, no-cache, must-revalidate")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::empty())
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "download response error").into_response()
        })
}

async fn public_download_rustdesk_handler(
    State(state): State<Arc<AppState>>,
    Query(q): Query<BuildQuery>,
) -> Response {
    let Some(flavor) = norm_flavor(&q.flavor) else {
        return (StatusCode::BAD_REQUEST, "flavor must be normal or cashdesk").into_response();
    };
    let Some(platform) = norm_platform(&q.platform) else {
        return (StatusCode::BAD_REQUEST, "unsupported platform").into_response();
    };
    let Some(build) = get_published_build(&state.pool, flavor, platform).await else {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    };
    let path = PathBuf::from(&build.file_path);
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return (StatusCode::NOT_FOUND, "file not found").into_response(),
    };
    let file = match tokio::fs::File::open(&path).await {
        Ok(f) => f,
        Err(e) => {
            tracing::error!("open build for download: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "download error").into_response();
        }
    };
    let stream = tokio_util::io::ReaderStream::new(file);
    let body = Body::from_stream(stream);
    let filename = sanitize_filename(&build.file_name);
    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(CONTENT_LENGTH, metadata.len().to_string())
        .header("Cache-Control", "no-store, no-cache, must-revalidate")
        .header("X-Content-Type-Options", "nosniff")
        .body(body)
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "download response error").into_response()
        })
}

fn build_app(state: Arc<AppState>, upload_body_limit: usize) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/api/v1/health", get(|| async { "ok" }))
        .route("/api/v1/report", post(report_handler))
        .route("/api/v1/ad/assign", post(ad_assign_handler))
        .route("/api/v1/auth/login", post(login_handler))
        .route("/api/v1/devices", get(devices_handler))
        .route(
            "/api/v1/devices/{id}",
            axum::routing::delete(delete_device_handler),
        )
        .route(
            "/api/v1/servers",
            get(list_servers_handler).post(upsert_server_handler),
        )
        .route(
            "/api/v1/admin/servers/{id}",
            axum::routing::delete(delete_server_handler),
        )
        .route(
            "/api/v1/admin/builds",
            get(admin_list_builds_handler)
                .post(admin_upload_build_handler.layer(DefaultBodyLimit::max(upload_body_limit))),
        )
        .route(
            "/api/v1/admin/builds/{id}/approve",
            post(admin_approve_build_handler),
        )
        .route(
            "/api/v1/admin/builds/{id}/reject",
            post(admin_reject_build_handler),
        )
        .route(
            "/api/v1/ci/builds",
            post(ci_upload_build_handler.layer(DefaultBodyLimit::max(upload_body_limit))),
        )
        .route(
            "/api/v1/downloads/rustdesk/windows/meta",
            get(public_software_update_meta_handler),
        )
        .route(
            "/api/v1/downloads/rustdesk/windows/latest",
            get(public_download_rustdesk_handler).head(public_download_rustdesk_head_handler),
        )
        .layer(from_fn(json_error_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            env::var("RUST_LOG")
                .unwrap_or_else(|_| "inventory_portal_api=info,tower_http=info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:/data/inventory.db?mode=rwc".to_string());
    let device_token = env::var("INVENTORY_DEVICE_TOKEN").unwrap_or_else(|_| {
        tracing::warn!("INVENTORY_DEVICE_TOKEN not set, using same default as RustDesk RS_PUB_KEY fork default");
        "ykYXbcaCNMz4wTqV0cw4K02a4jJRMIrFgB72a+4wSmk=".to_string()
    });
    let admin_password =
        env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin-change-me".to_string());
    if admin_password == "admin-change-me" {
        tracing::warn!(
            "ADMIN_PASSWORD is set to default 'admin-change-me' — please set ADMIN_PASSWORD in production"
        );
    }
    let jwt_secret = env::var("JWT_SECRET").unwrap_or_else(|_| {
        tracing::warn!("JWT_SECRET not set, using dev default");
        "dev-secret-change-in-production-min-32-chars!!".to_string()
    });
    let uploads_dir =
        PathBuf::from(env::var("UPLOAD_DIR").unwrap_or_else(|_| "/data/downloads".to_string()));
    let max_upload_bytes = env::var("MAX_UPLOAD_BYTES")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_UPLOAD_BYTES);
    let ci_upload_token = env::var("CI_UPLOAD_TOKEN").unwrap_or_default();
    let rustdeskweb_api_url = env::var("RUSTDESKWEB_API_URL")
        .unwrap_or_else(|_| "https://tnremdeskapi.pxy2.tatnefturs.ru".to_string());
    let rustdeskweb_api_token = env::var("RUSTDESKWEB_API_TOKEN").unwrap_or_default();
    let ad_domain = env::var("AD_DOMAIN").unwrap_or_else(|_| "corp.tatnefturs.tatar".to_string());
    let preset_address_book_name = env::var("PRESET_ADDRESS_BOOK_NAME")
        .unwrap_or_else(|_| "corp.tatnefturs.tatar".to_string());
    if rustdeskweb_api_token.is_empty() {
        tracing::warn!("RUSTDESKWEB_API_TOKEN not set: AD address book assignment is disabled");
    }

    let opts = database_url
        .parse::<SqliteConnectOptions>()?
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(opts)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            rustdesk_id TEXT PRIMARY KEY NOT NULL,
            hostname TEXT NOT NULL DEFAULT '',
            os_info TEXT NOT NULL DEFAULT '',
            username TEXT NOT NULL DEFAULT '',
            ip_public TEXT NOT NULL DEFAULT '',
            ip_local TEXT NOT NULL DEFAULT '',
            temporary_password TEXT NOT NULL DEFAULT '',
            computer_summary TEXT NOT NULL DEFAULT '',
            app_version TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS rustdesk_windows_release (
            id INTEGER PRIMARY KEY NOT NULL,
            version TEXT NOT NULL,
            updated_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS ad_assignments (
            rustdesk_id TEXT PRIMARY KEY NOT NULL,
            alias TEXT NOT NULL DEFAULT '',
            ad_user TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            message TEXT NOT NULL DEFAULT '',
            updated_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS builds (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            flavor TEXT NOT NULL DEFAULT 'normal',
            platform TEXT NOT NULL DEFAULT 'windows',
            version TEXT NOT NULL,
            file_name TEXT NOT NULL DEFAULT '',
            file_path TEXT NOT NULL DEFAULT '',
            file_size INTEGER NOT NULL DEFAULT 0,
            sha256 TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'pending',
            uploaded_at INTEGER NOT NULL,
            approved_at INTEGER,
            approved_by TEXT NOT NULL DEFAULT ''
        )
        "#,
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS servers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL DEFAULT '',
            host TEXT NOT NULL UNIQUE,
            public_key TEXT NOT NULL DEFAULT '',
            created_by TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        )
        "#,
    )
    .execute(&pool)
    .await?;

    // Migrate the legacy single-file release into the builds table (once).
    migrate_legacy_release(&pool, &uploads_dir).await;

    tokio::fs::create_dir_all(&uploads_dir).await?;

    // Очистка брошенных временных файлов загрузки при перезапуске
    if let Ok(mut entries) = tokio::fs::read_dir(&uploads_dir).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let is_temp = (name.starts_with(".build-upload-")
                || name.starts_with(".rustdesk-upload-"))
                && name.ends_with(".tmp");
            if is_temp {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }

    let state = Arc::new(AppState {
        pool,
        device_token,
        admin_password,
        jwt_secret,
        uploads_dir,
        max_upload_bytes,
        ci_upload_token,
        rustdeskweb_api_url,
        rustdeskweb_api_token,
        ad_domain,
        preset_address_book_name,
        collection_cache: Arc::new(tokio::sync::Mutex::new(None)),
    });

    // Axum по умолчанию режет тело запроса для Multipart (~2 МБ) — без этого большой rustdesk.exe не доходит до хендлера.
    let upload_body_limit = usize::try_from(max_upload_bytes).unwrap_or(usize::MAX);

    let app = build_app(state, upload_body_limit);

    let addr: SocketAddr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        admin_auth_error, build_app, issue_jwt, norm_flavor, norm_platform, normalize_server_host,
        AppState, Body, Request, SqlitePoolOptions, StatusCode,
    };
    use axum::body::to_bytes;
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tower::ServiceExt;

    async fn test_state() -> Arc<AppState> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory database");
        sqlx::query(
            "CREATE TABLE servers (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL DEFAULT '', host TEXT NOT NULL UNIQUE, public_key TEXT NOT NULL DEFAULT '', created_by TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("servers table");
        sqlx::query(
            "CREATE TABLE builds (id INTEGER PRIMARY KEY AUTOINCREMENT, flavor TEXT NOT NULL DEFAULT 'normal', platform TEXT NOT NULL DEFAULT 'windows', version TEXT NOT NULL, file_name TEXT NOT NULL DEFAULT '', file_path TEXT NOT NULL DEFAULT '', file_size INTEGER NOT NULL DEFAULT 0, sha256 TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT 'pending', uploaded_at INTEGER NOT NULL, approved_at INTEGER, approved_by TEXT NOT NULL DEFAULT '')",
        )
        .execute(&pool)
        .await
        .expect("builds table");
        sqlx::query(
            "CREATE TABLE devices (rustdesk_id TEXT PRIMARY KEY, hostname TEXT NOT NULL DEFAULT '', os_info TEXT NOT NULL DEFAULT '', username TEXT NOT NULL DEFAULT '', ip_public TEXT NOT NULL DEFAULT '', ip_local TEXT NOT NULL DEFAULT '', temporary_password TEXT NOT NULL DEFAULT '', computer_summary TEXT NOT NULL DEFAULT '', app_version TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("devices table");
        sqlx::query(
            "CREATE TABLE ad_assignments (rustdesk_id TEXT PRIMARY KEY, alias TEXT NOT NULL DEFAULT '', ad_user TEXT NOT NULL DEFAULT '', status TEXT NOT NULL DEFAULT '', message TEXT NOT NULL DEFAULT '', updated_at INTEGER NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("AD assignments table");
        Arc::new(AppState {
            pool,
            device_token: "device-secret".into(),
            admin_password: "admin-secret".into(),
            jwt_secret: "jwt-secret".into(),
            uploads_dir: PathBuf::new(),
            max_upload_bytes: 1024,
            ci_upload_token: String::new(),
            rustdeskweb_api_url: String::new(),
            rustdeskweb_api_token: String::new(),
            ad_domain: String::new(),
            preset_address_book_name: String::new(),
            collection_cache: Arc::new(tokio::sync::Mutex::new(None)),
        })
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let body = to_bytes(response.into_body(), 1024 * 1024)
            .await
            .expect("response body");
        serde_json::from_slice(&body).expect("JSON response")
    }

    #[test]
    fn server_host_requires_a_valid_host_and_port() {
        assert_eq!(
            normalize_server_host(" RustDesk.Example.com:21116 "),
            Some("rustdesk.example.com:21116".into())
        );
        assert_eq!(
            normalize_server_host("[2001:db8::1]:21116"),
            Some("[2001:db8::1]:21116".into())
        );
        assert_eq!(normalize_server_host("example.com"), None);
        assert_eq!(normalize_server_host("example.com:0"), None);
        assert_eq!(normalize_server_host("example.com:70000"), None);
        assert_eq!(normalize_server_host("https://example.com:21116"), None);
        assert_eq!(normalize_server_host("bad host:21116"), None);
    }

    #[test]
    fn build_query_values_are_normalized_or_rejected() {
        assert_eq!(norm_flavor(" CASHDESK "), Some("cashdesk"));
        assert_eq!(norm_flavor(""), Some("normal"));
        assert_eq!(norm_flavor("staging"), None);
        assert_eq!(norm_platform("Darwin"), Some("macos"));
        assert_eq!(norm_platform(""), Some("windows"));
        assert_eq!(norm_platform("freebsd"), None);
    }

    #[tokio::test]
    async fn device_token_is_forbidden_from_admin_routes() {
        let state = test_state().await;
        assert_eq!(
            admin_auth_error(&state, Some("Bearer device-secret")).status(),
            super::StatusCode::FORBIDDEN
        );
        assert_eq!(
            admin_auth_error(&state, Some("Bearer wrong-secret")).status(),
            super::StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn server_registry_api_preserves_read_access_and_restricts_writes() {
        let state = test_state().await;
        let app = build_app(state.clone(), 1024);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/servers")
                    .header("authorization", "Bearer device-secret")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response_json(response).await, json!([]));

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/servers")
                    .header("authorization", "Bearer device-secret")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"Branch","host":"branch.example:21116","public_key":"key"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(response_json(response).await["code"], "forbidden");

        let (admin_token, _) = issue_jwt(&state).expect("admin JWT");
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/servers")
                    .header("authorization", format!("Bearer {admin_token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"host":"not-a-host","public_key":"key"}"#))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(response_json(response).await["code"], "bad_request");

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/servers")
                    .header("authorization", format!("Bearer {admin_token}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        r#"{"name":"Branch","host":"BRANCH.example:21116","public_key":"key"}"#,
                    ))
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response_json(response).await["host"],
            "branch.example:21116"
        );
    }

    #[tokio::test]
    async fn build_status_changes_only_allow_pending_transitions() {
        let state = test_state().await;
        for (id, status) in [(1, "published"), (2, "pending")] {
            sqlx::query("INSERT INTO builds (id, flavor, platform, version, status, uploaded_at) VALUES (?, 'normal', 'windows', ?, ?, 1)")
                .bind(id)
                .bind(format!("1.4.{id}"))
                .bind(status)
                .execute(&state.pool)
                .await
                .expect("insert build");
        }
        let (admin_token, _) = issue_jwt(&state).expect("admin JWT");
        let app = build_app(state.clone(), 1024);

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/builds/1/approve")
                    .header("authorization", format!("Bearer {admin_token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(response_json(response).await["code"], "conflict");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/admin/builds/2/approve")
                    .header("authorization", format!("Bearer {admin_token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response_json(response).await["status"], "published");

        let statuses: Vec<(i64, String)> =
            sqlx::query_as("SELECT id, status FROM builds ORDER BY id")
                .fetch_all(&state.pool)
                .await
                .expect("build statuses");
        assert_eq!(
            statuses,
            vec![(1, "archived".into()), (2, "published".into())]
        );
    }

    #[tokio::test]
    async fn device_list_includes_ad_assignment_result_when_present() {
        let state = test_state().await;
        sqlx::query("INSERT INTO devices (rustdesk_id, hostname, updated_at) VALUES ('device-1', 'workstation', 1), ('device-2', 'laptop', 1)")
            .execute(&state.pool)
            .await
            .expect("insert devices");
        sqlx::query("INSERT INTO ad_assignments (rustdesk_id, alias, ad_user, status, message, updated_at) VALUES ('device-1', 'Workstation', 'CORP\\\\user', 'error', 'upstream unavailable', 2)")
            .execute(&state.pool)
            .await
            .expect("insert AD assignment");
        let (admin_token, _) = issue_jwt(&state).expect("admin JWT");
        let app = build_app(state, 1024);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/devices")
                    .header("authorization", format!("Bearer {admin_token}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::OK);
        let devices = response_json(response).await;
        assert_eq!(devices[0]["ad_status"], "error");
        assert_eq!(devices[0]["ad_message"], "upstream unavailable");
        assert_eq!(devices[0]["ad_updated_at"], "1970-01-01T00:00:02+00:00");
        assert!(devices[1].get("ad_status").is_none());
    }
}
