use axum::{
    extract::State,
    http::{header::AUTHORIZATION, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::env;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Clone)]
struct AppState {
    pool: SqlitePool,
    device_token: String,
    admin_password: String,
    jwt_secret: String,
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

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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

    let state = Arc::new(AppState {
        pool,
        device_token,
        admin_password,
        jwt_secret,
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/v1/health", get(|| async { "ok" }))
        .route("/api/v1/report", post(report_handler))
        .route("/api/v1/auth/login", post(login_handler))
        .route("/api/v1/devices", get(devices_handler))
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
