use axum::{
    body::Body,
    handler::Handler,
    extract::{DefaultBodyLimit, Multipart, State},
    http::{
        header::{AUTHORIZATION, CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_TYPE},
        StatusCode,
    },
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
}

#[derive(Debug, Deserialize)]
struct ReportPayload {
    rustdesk_id: String,
    hostname: Option<String>,
    os: Option<String>,
    username: Option<String>,
    cpu: Option<String>,
    memory: Option<String>,
    computer_summary: Option<String>,
    ip_public: Option<String>,
    ip_local: Option<String>,
    temporary_password: Option<String>,
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

#[derive(Debug, Serialize)]
struct DownloadAssetDto {
    available: bool,
    file_name: Option<String>,
    file_size: Option<u64>,
    uploaded_at: Option<String>,
    download_path: String,
    /// Версия, объявленная при загрузке (для клиентского автообновления).
    #[serde(skip_serializing_if = "Option::is_none")]
    published_version: Option<String>,
}

#[derive(Debug, Serialize)]
struct SoftwareUpdateMetaDto {
    available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    version: Option<String>,
    download_path: String,
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

fn system_time_to_ts(value: SystemTime) -> i64 {
    value
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or_else(|_| now_ts())
}

fn rustdesk_windows_binary_path(state: &AppState) -> PathBuf {
    state
        .uploads_dir
        .join(RUSTDESK_WINDOWS_STORED_FILENAME)
}

fn filename_has_exe_extension(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("exe"))
        .unwrap_or(false)
}

async fn get_published_version(pool: &SqlitePool) -> Option<String> {
    sqlx::query_scalar::<_, String>("SELECT version FROM rustdesk_windows_release WHERE id = 1")
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn get_download_asset_info(state: &AppState) -> anyhow::Result<DownloadAssetDto> {
    let path = rustdesk_windows_binary_path(state);
    let published_version = get_published_version(&state.pool).await;
    if !tokio::fs::try_exists(&path).await? {
        return Ok(DownloadAssetDto {
            available: false,
            file_name: None,
            file_size: None,
            uploaded_at: None,
            download_path: RUSTDESK_WINDOWS_DOWNLOAD_PATH.to_string(),
            published_version: None,
        });
    }

    let metadata = tokio::fs::metadata(&path).await?;
    let uploaded_at = metadata
        .modified()
        .ok()
        .map(system_time_to_ts)
        .unwrap_or_else(now_ts);

    Ok(DownloadAssetDto {
        available: true,
        file_name: Some("rustdesk.exe".to_string()),
        file_size: Some(metadata.len()),
        uploaded_at: Some(format_ts(uploaded_at)),
        download_path: RUSTDESK_WINDOWS_DOWNLOAD_PATH.to_string(),
        published_version,
    })
}

fn verify_device_token(state: &AppState, auth_header: Option<&str>) -> bool {
    let Some(h) = auth_header else {
        return false;
    };
    let Some(t) = h.strip_prefix("Bearer ") else {
        return false;
    };
    t == state.device_token
}

fn verify_admin_jwt(state: &AppState, auth_header: Option<&str>) -> bool {
    let Some(h) = auth_header else {
        return false;
    };
    let Some(token) = h.strip_prefix("Bearer ") else {
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
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if !verify_device_token(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    if p.rustdesk_id.is_empty() {
        return (StatusCode::BAD_REQUEST, "rustdesk_id required").into_response();
    }
    let os_info = p.os.clone().unwrap_or_default();
    let computer_summary = p
        .computer_summary
        .clone()
        .unwrap_or_else(|| {
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
    .bind(&p.rustdesk_id)
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

async fn login_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<LoginBody>,
) -> impl IntoResponse {
    if body.password != state.admin_password {
        return (StatusCode::UNAUTHORIZED, "bad password").into_response();
    }
    match issue_jwt(&state) {
        Ok((token, expires_in)) => (
            StatusCode::OK,
            Json(LoginResponse { token, expires_in }),
        )
            .into_response(),
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
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }
    let rows: Result<Vec<DeviceRow>, _> = sqlx::query_as(
        "SELECT rustdesk_id, hostname, os_info, username, ip_public, ip_local, temporary_password, computer_summary, app_version, updated_at FROM devices ORDER BY updated_at DESC",
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

async fn admin_download_info_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
) -> impl IntoResponse {
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    match get_download_asset_info(&state).await {
        Ok(info) => Json(info).into_response(),
        Err(e) => {
            tracing::error!("download info: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "download info error").into_response()
        }
    }
}

async fn admin_upload_rustdesk_handler(
    State(state): State<Arc<AppState>>,
    headers: axum::http::HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let auth = headers
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if !verify_admin_jwt(&state, auth) {
        return (StatusCode::UNAUTHORIZED, "unauthorized").into_response();
    }

    let temp_name = format!(
        ".rustdesk-upload-{}-{}.tmp",
        now_ts(),
        std::process::id()
    );
    let temp_path = state.uploads_dir.join(&temp_name);
    let final_path = rustdesk_windows_binary_path(&state);

    let mut form_version = String::new();
    let mut uploaded_filename = None::<String>;
    let mut total_size = 0_u64;
    let mut file_written = false;

    loop {
        let next_field = match multipart.next_field().await {
            Ok(field) => field,
            Err(e) => {
                tracing::warn!("multipart error: {}", e);
                let _ = tokio::fs::remove_file(&temp_path).await;
                return (StatusCode::BAD_REQUEST, "invalid multipart payload").into_response();
            }
        };

        let Some(mut field) = next_field else {
            break;
        };

        match field.name() {
            Some("version") => {
                if let Ok(t) = field.text().await {
                    form_version = t;
                }
            }
            Some("file") => {
                let filename = field.file_name().unwrap_or("rustdesk.exe").to_string();
                if !filename_has_exe_extension(&filename) {
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return (
                        StatusCode::BAD_REQUEST,
                        "only .exe files are allowed",
                    )
                        .into_response();
                }

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
                            let _ = tokio::fs::remove_file(&temp_path).await;
                            return (StatusCode::BAD_REQUEST, "invalid upload chunk").into_response();
                        }
                    };

                    let Some(chunk) = chunk else {
                        break;
                    };

                    total_size += chunk.len() as u64;
                    if total_size > state.max_upload_bytes {
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return (
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "file is too large",
                        )
                            .into_response();
                    }

                    if let Err(e) = file.write_all(&chunk).await {
                        tracing::error!("write upload file: {}", e);
                        let _ = tokio::fs::remove_file(&temp_path).await;
                        return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
                    }
                }

                if let Err(e) = file.flush().await {
                    tracing::error!("flush upload file: {}", e);
                    let _ = tokio::fs::remove_file(&temp_path).await;
                    return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
                }

                uploaded_filename = Some(filename);
                file_written = true;
            }
            _ => {}
        }
    }

    if !file_written || total_size == 0 {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return (StatusCode::BAD_REQUEST, "file is required").into_response();
    }

    let version_trim = form_version.trim();
    if version_trim.is_empty() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return (StatusCode::BAD_REQUEST, "version is required").into_response();
    }

    if tokio::fs::try_exists(&final_path).await.unwrap_or(false) {
        if let Err(e) = tokio::fs::remove_file(&final_path).await {
            tracing::error!("remove old rustdesk.exe: {}", e);
            let _ = tokio::fs::remove_file(&temp_path).await;
            return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
        }
    }

    if let Err(e) = tokio::fs::rename(&temp_path, &final_path).await {
        tracing::error!("move rustdesk.exe upload: {}", e);
        let _ = tokio::fs::remove_file(&temp_path).await;
        return (StatusCode::INTERNAL_SERVER_ERROR, "upload error").into_response();
    }

    let ts = now_ts();
    if let Err(e) = sqlx::query(
        r#"
        INSERT INTO rustdesk_windows_release (id, version, updated_at)
        VALUES (1, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            version = excluded.version,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(version_trim)
    .bind(ts)
    .execute(&state.pool)
    .await
    {
        tracing::error!("save published version: {}", e);
        let _ = tokio::fs::remove_file(&final_path).await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "failed to save version metadata",
        )
            .into_response();
    }

    tracing::info!(
        file_name = uploaded_filename.as_deref().unwrap_or("rustdesk.exe"),
        bytes = total_size,
        version = %version_trim,
        "rustdesk.exe uploaded"
    );

    match get_download_asset_info(&state).await {
        Ok(info) => (StatusCode::OK, Json(info)).into_response(),
        Err(e) => {
            tracing::error!("download info after upload: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "upload saved but status failed").into_response()
        }
    }
}

async fn public_software_update_meta_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let path_ok = tokio::fs::try_exists(rustdesk_windows_binary_path(&state))
        .await
        .unwrap_or(false);
    let published = get_published_version(&state.pool).await;
    let available = path_ok && published.is_some();
    let body = SoftwareUpdateMetaDto {
        available,
        version: if available { published } else { None },
        download_path: RUSTDESK_WINDOWS_DOWNLOAD_PATH.to_string(),
    };
    Json(body).into_response()
}

async fn public_download_rustdesk_handler(
    State(state): State<Arc<AppState>>,
) -> Response {
    let path = rustdesk_windows_binary_path(&state);
    let exists = tokio::fs::try_exists(&path).await.unwrap_or(false);
    if !exists {
        return (StatusCode::NOT_FOUND, "file not found").into_response();
    }

    let bytes = match tokio::fs::read(&path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::error!("read rustdesk.exe for download: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "download error").into_response();
        }
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/octet-stream")
        .header(CONTENT_DISPOSITION, "attachment; filename=\"rustdesk.exe\"")
        .header(CONTENT_LENGTH, bytes.len().to_string())
        .header("Cache-Control", "no-store, no-cache, must-revalidate")
        .header("X-Content-Type-Options", "nosniff")
        .body(Body::from(bytes))
        .unwrap_or_else(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, "download response error").into_response()
        })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            env::var("RUST_LOG").unwrap_or_else(|_| "inventory_portal_api=info,tower_http=info".into()),
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

    tokio::fs::create_dir_all(&uploads_dir).await?;

    let state = Arc::new(AppState {
        pool,
        device_token,
        admin_password,
        jwt_secret,
        uploads_dir,
        max_upload_bytes,
    });

    // Axum по умолчанию режет тело запроса для Multipart (~2 МБ) — без этого большой rustdesk.exe не доходит до хендлера.
    let upload_body_limit = usize::try_from(max_upload_bytes).unwrap_or(usize::MAX);

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/v1/health", get(|| async { "ok" }))
        .route("/api/v1/report", post(report_handler))
        .route("/api/v1/auth/login", post(login_handler))
        .route("/api/v1/devices", get(devices_handler))
        .route(
            "/api/v1/admin/downloads/rustdesk",
            get(admin_download_info_handler).post(
                admin_upload_rustdesk_handler.layer(DefaultBodyLimit::max(upload_body_limit)),
            ),
        )
        .route(
            "/api/v1/downloads/rustdesk/windows/meta",
            get(public_software_update_meta_handler),
        )
        .route(
            "/api/v1/downloads/rustdesk/windows/latest",
            get(public_download_rustdesk_handler),
        )
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state);

    let addr: SocketAddr = env::var("BIND_ADDR")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string())
        .parse()?;
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
