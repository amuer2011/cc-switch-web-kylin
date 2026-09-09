use axum::{
    extract::{DefaultBodyLimit, State},
    http::StatusCode,
    response::Json,
    routing::post,
    Router,
};
use cc_switch_lib::{
    app_config::{AppType, McpServer},
    config,
    database::{Database, ModelPricingRecord},
    provider::Provider,
    services::{
        mcp::McpService,
        provider::ProviderService,
        stream_check::{HealthStatus, StreamCheckConfig, StreamCheckResult, StreamCheckService},
        webdav_sync as webdav_sync_service,
        PromptService,
    },
    settings,
    store::AppState,
};
use libc::getuid;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};
use tokio::sync::oneshot;

/// Global shutdown signal sender — set in main(), used by cmd_shutdown
static SHUTDOWN_TX: LazyLock<Mutex<Option<oneshot::Sender<()>>>> = LazyLock::new(|| Mutex::new(None));

/// Desktop environment vars passed by native-shell (DISPLAY, XAUTHORITY, DBUS, etc.)
/// Used by the terminal launcher to ensure spawned terminals can connect to the user's X session.
static TERMINAL_ENV: LazyLock<Mutex<std::collections::HashMap<String, String>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

#[derive(Deserialize)]
struct InvokeRequest {
    command: String,
    args: Option<Value>,
}

#[derive(serde::Serialize)]
struct InvokeResponse {
    success: bool,
    data: Option<Value>,
    error: Option<String>,
}

async fn handle_invoke(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<InvokeRequest>,
) -> (StatusCode, Json<InvokeResponse>) {
    let result = route_command(&state, &payload.command, payload.args.as_ref()).await;
    match result {
        Ok(data) => (StatusCode::OK, Json(InvokeResponse {
            success: true,
            data: Some(data),
            error: None,
        })),
        Err(err) => (StatusCode::OK, Json(InvokeResponse {
            success: false,
            data: None,
            error: Some(err),
        })),
    }
}

// ===================== Arg helpers =====================

fn required_str<'a>(args: Option<&'a Value>, key: &str) -> Result<&'a str, String> {
    args
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("Missing required argument: {key}"))
}

fn optional_str<'a>(args: Option<&'a Value>, key: &str) -> Option<&'a str> {
    args
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
}

fn optional_str_owned(args: Option<&Value>, key: &str) -> Option<String> {
    optional_str(args, key).map(String::from)
}

fn parse_app_type(args: Option<&Value>) -> Result<AppType, String> {
    let s = required_str(args, "app")?;
    AppType::from_str(s).map_err(|_| format!("Invalid app type: {s}"))
}

fn parse_provider(args: Option<&Value>) -> Result<Provider, String> {
    let v = args
        .and_then(|a| a.get("provider"))
        .ok_or_else(|| "Missing required argument: provider".to_string())?;
    serde_json::from_value(v.clone()).map_err(|e| format!("Invalid provider: {e}"))
}

fn parse_mcp_server(args: Option<&Value>) -> Result<McpServer, String> {
    let v = args
        .and_then(|a| a.get("server"))
        .ok_or_else(|| "Missing required argument: server".to_string())?;
    serde_json::from_value(v.clone()).map_err(|e| format!("Invalid MCP server: {e}"))
}

fn bool_or_default(args: Option<&Value>, key: &str, default: bool) -> bool {
    args
        .and_then(|a| a.get(key))
        .and_then(|v| v.as_bool())
        .unwrap_or(default)
}

fn i64_or_default(args: Option<&Value>, key: &str) -> Option<i64> {
    args.and_then(|a| a.get(key)).and_then(|v| v.as_i64())
}

// ===================== Provider Commands =====================

async fn cmd_get_providers(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let providers = ProviderService::list(state, app).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(providers).map_err(|e| e.to_string())?)
}

async fn cmd_get_current_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = ProviderService::current(state, app).map_err(|e| e.to_string())?;
    Ok(Value::String(id))
}

async fn cmd_add_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let provider = parse_provider(args)?;
    let add_to_live = bool_or_default(args, "addToLive", false);
    ProviderService::add(state, app, provider, add_to_live).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_update_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let original_id = optional_str_owned(args, "originalId");
    let provider = parse_provider(args)?;
    ProviderService::update(state, app, original_id.as_deref(), provider).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_delete_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = required_str(args, "id")?;
    ProviderService::delete(state, app, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_switch_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = required_str(args, "id")?;
    let result = ProviderService::switch(state, app, id).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

async fn cmd_disable_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = required_str(args, "id")?;
    ProviderService::disable(state, app, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_remove_provider_from_live(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = required_str(args, "id")?;
    ProviderService::remove_from_live_config(state, app, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_sync_current_providers_live(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    ProviderService::sync_current_to_live(state).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_update_providers_sort_order(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let updates: Vec<cc_switch_lib::services::ProviderSortUpdate> = args
        .and_then(|a| a.get("updates"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: updates".to_string())?;
    ProviderService::update_sort_order(state, app, updates).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Settings Commands =====================

const APP_CONFIG_DIR_KEY: &str = "app_config_dir_override";

fn app_paths_store_path() -> PathBuf {
    config::get_home_dir().join(".cc-switch").join("app_paths.json")
}

fn resolve_user_path(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if trimmed == "~" {
        config::get_home_dir()
    } else if let Some(relative) = trimmed.strip_prefix("~/") {
        config::get_home_dir().join(relative)
    } else {
        PathBuf::from(trimmed)
    }
}

fn read_app_config_dir_override() -> Option<PathBuf> {
    let content = std::fs::read_to_string(app_paths_store_path()).ok()?;
    let store: Value = serde_json::from_str(&content).ok()?;
    let raw = store.get(APP_CONFIG_DIR_KEY)?.as_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    let path = resolve_user_path(raw);
    if path.is_dir() {
        Some(path)
    } else {
        log::warn!("Configured app directory does not exist: {}", path.display());
        None
    }
}

fn write_app_config_dir_override(path: Option<&str>) -> Result<(), String> {
    let store_path = app_paths_store_path();
    let mut store = std::fs::read_to_string(&store_path)
        .ok()
        .and_then(|content| serde_json::from_str::<Value>(&content).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();

    match path.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => {
            let resolved = resolve_user_path(path);
            if !resolved.is_dir() {
                return Err(format!("配置目录不存在: {}", resolved.display()));
            }
            store.insert(
                APP_CONFIG_DIR_KEY.to_string(),
                Value::String(path.to_string()),
            );
        }
        None => {
            store.remove(APP_CONFIG_DIR_KEY);
        }
    }

    let content = serde_json::to_vec_pretty(&Value::Object(store)).map_err(|e| e.to_string())?;
    config::atomic_write(&store_path, &content).map_err(|e| e.to_string())
}

async fn cmd_get_settings(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let s = settings::get_settings_for_frontend();
    serde_json::to_value(s).map_err(|e| e.to_string())
}

async fn cmd_save_settings(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let mut new_settings: settings::AppSettings = args
        .and_then(|a| a.get("settings"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: settings".to_string())?;
    let existing = settings::get_settings();
    match (&mut new_settings.webdav_sync, &existing.webdav_sync) {
        (None, current) => new_settings.webdav_sync = current.clone(),
        (Some(incoming), Some(current))
            if incoming.password.is_empty() && !current.password.is_empty() =>
        {
            incoming.password = current.password.clone();
        }
        _ => {}
    }
    settings::update_settings(new_settings).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_app_config_dir_override(
    _state: &AppState,
    _args: Option<&Value>,
) -> Result<Value, String> {
    Ok(read_app_config_dir_override()
        .map(|path| Value::String(path.to_string_lossy().to_string()))
        .unwrap_or(Value::Null))
}

async fn cmd_set_app_config_dir_override(
    _state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let path = args
        .and_then(|value| value.get("path"))
        .and_then(Value::as_str);
    write_app_config_dir_override(path)?;
    Ok(Value::Bool(true))
}

fn webdav_settings_from_args(args: Option<&Value>) -> Result<settings::WebDavSyncSettings, String> {
    args.and_then(|value| value.get("settings"))
        .and_then(|value| serde_json::from_value(value.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: settings".to_string())
}

fn require_enabled_webdav_settings() -> Result<settings::WebDavSyncSettings, String> {
    let sync_settings = settings::get_webdav_sync_settings()
        .ok_or_else(|| "未配置 WebDAV 同步".to_string())?;
    if !sync_settings.enabled {
        return Err("WebDAV 同步未启用".to_string());
    }
    Ok(sync_settings)
}

fn preserve_webdav_password(
    mut incoming: settings::WebDavSyncSettings,
    preserve_empty: bool,
) -> settings::WebDavSyncSettings {
    if preserve_empty && incoming.password.is_empty() {
        if let Some(existing) = settings::get_webdav_sync_settings() {
            incoming.password = existing.password;
        }
    }
    incoming
}

async fn cmd_webdav_test_connection(
    _state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let preserve_empty = args
        .and_then(|value| value.get("preserveEmptyPassword"))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let sync_settings = preserve_webdav_password(webdav_settings_from_args(args)?, preserve_empty);
    webdav_sync_service::check_connection(&sync_settings)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "success": true,
        "message": "WebDAV connection ok"
    }))
}

async fn cmd_webdav_sync_save_settings(
    _state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let password_touched = args
        .and_then(|value| value.get("passwordTouched"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let existing = settings::get_webdav_sync_settings();
    let mut sync_settings = preserve_webdav_password(
        webdav_settings_from_args(args)?,
        !password_touched,
    );
    if let Some(existing) = existing {
        sync_settings.status = existing.status;
    }
    sync_settings.normalize();
    sync_settings.validate().map_err(|e| e.to_string())?;
    settings::set_webdav_sync_settings(Some(sync_settings)).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_webdav_sync_upload(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let mut sync_settings = require_enabled_webdav_settings()?;
    webdav_sync_service::run_with_sync_lock(webdav_sync_service::upload(
        &state.db,
        &mut sync_settings,
    ))
    .await
    .map_err(|e| e.to_string())
}

async fn cmd_webdav_sync_download(
    state: &AppState,
    _args: Option<&Value>,
) -> Result<Value, String> {
    let mut sync_settings = require_enabled_webdav_settings()?;
    let mut result = webdav_sync_service::run_with_sync_lock(webdav_sync_service::download(
        &state.db,
        &mut sync_settings,
    ))
    .await
    .map_err(|e| e.to_string())?;

    if let Err(error) = ProviderService::sync_current_to_live(state)
        .and_then(|_| settings::reload_settings())
    {
        log::warn!("[WebDAV] post-download sync warning: {error}");
        if let Some(object) = result.as_object_mut() {
            object.insert("warning".to_string(), Value::String(error.to_string()));
        }
    }
    Ok(result)
}

async fn cmd_webdav_sync_fetch_remote_info(
    _state: &AppState,
    _args: Option<&Value>,
) -> Result<Value, String> {
    let sync_settings = require_enabled_webdav_settings()?;
    let info = webdav_sync_service::fetch_remote_info(&sync_settings)
        .await
        .map_err(|e| e.to_string())?;
    Ok(info.unwrap_or_else(|| serde_json::json!({ "empty": true })))
}

// ===================== MCP Commands =====================

async fn cmd_get_mcp_servers(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let servers = McpService::get_all_servers(state).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(servers).map_err(|e| e.to_string())?)
}

async fn cmd_upsert_mcp_server(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let server = parse_mcp_server(args)?;
    McpService::upsert_server(state, server).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_delete_mcp_server(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let id = required_str(args, "id")?;
    McpService::delete_server(state, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_toggle_mcp_app(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let server_id = required_str(args, "serverId")?;
    let app = parse_app_type(args)?;
    let enabled = bool_or_default(args, "enabled", true);
    McpService::toggle_app(state, server_id, app, enabled).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_import_mcp_from_apps(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let mut total = 0usize;
    total += McpService::import_from_claude(state).unwrap_or(0);
    total += McpService::import_from_codex(state).unwrap_or(0);
    total += McpService::import_from_gemini(state).unwrap_or(0);
    total += McpService::import_from_opencode(state).unwrap_or(0);
    total += McpService::import_from_hermes(state).unwrap_or(0);
    Ok(serde_json::to_value(total).map_err(|e| e.to_string())?)
}

// ===================== Proxy Commands =====================

async fn cmd_get_proxy_status(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let status = state.proxy_service.get_status().await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(status).map_err(|e| e.to_string())?)
}

async fn cmd_get_proxy_config(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let config = state.proxy_service.get_config().await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(config).map_err(|e| e.to_string())?)
}

async fn cmd_update_proxy_config(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let config: cc_switch_lib::proxy::types::ProxyConfig = args
        .and_then(|a| a.get("config"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: config".to_string())?;
    state.proxy_service.update_config(&config).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_start_proxy_server(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let info = state.proxy_service.start_with_takeover().await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(info).map_err(|e| e.to_string())?)
}

async fn cmd_stop_proxy_with_restore(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    state.proxy_service.stop_with_restore().await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_is_proxy_running(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let running = state.proxy_service.is_running().await;
    Ok(Value::Bool(running))
}

async fn cmd_is_live_takeover_active(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let takeover_status = state.proxy_service.get_takeover_status().await.map_err(|e| e.to_string())?;
    let active = takeover_status.claude || takeover_status.codex || takeover_status.gemini;
    Ok(Value::Bool(active))
}

async fn cmd_get_proxy_takeover_status(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let status = state.proxy_service.get_takeover_status().await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(status).map_err(|e| e.to_string())?)
}

async fn cmd_set_proxy_takeover_for_app(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let enabled = bool_or_default(args, "enabled", true);
    state.proxy_service.set_takeover_for_app(app_type, enabled).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_switch_proxy_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let provider_id = required_str(args, "providerId")?;
    state.proxy_service.switch_proxy_target(app_type, provider_id).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_proxy_config_for_app(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let config = state.db.get_proxy_config_for_app(app_type).await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(config).map_err(|e| e.to_string())?)
}

async fn cmd_update_proxy_config_for_app(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let config: cc_switch_lib::proxy::types::AppProxyConfig = args
        .and_then(|a| a.get("config"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: config".to_string())?;
    state.db.update_proxy_config_for_app(config).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_default_cost_multiplier(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let multiplier = state.db.get_default_cost_multiplier(app_type).await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(multiplier).map_err(|e| e.to_string())?)
}

async fn cmd_set_default_cost_multiplier(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let value = required_str(args, "value")?;
    state.db.set_default_cost_multiplier(app_type, value).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_pricing_model_source(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let source = state.db.get_pricing_model_source(app_type).await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(source).map_err(|e| e.to_string())?)
}

async fn cmd_set_pricing_model_source(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let source = required_str(args, "value")?;
    state.db.set_pricing_model_source(app_type, source).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_failover_queue(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let providers = state.db.get_failover_providers(app_type).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(providers).map_err(|e| e.to_string())?)
}

async fn cmd_add_to_failover_queue(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let provider_id = required_str(args, "providerId")?;
    state.db.add_to_failover_queue(app_type, provider_id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_remove_from_failover_queue(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let provider_id = required_str(args, "providerId")?;
    state.db.remove_from_failover_queue(app_type, provider_id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_available_providers_for_failover(
    state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let providers = state
        .db
        .get_available_providers_for_failover(app_type)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(providers).map_err(|e| e.to_string())?)
}

async fn cmd_get_auto_failover_enabled(
    state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?;
    let config = state
        .db
        .get_proxy_config_for_app(app_type)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(config.auto_failover_enabled).map_err(|e| e.to_string())?)
}

async fn cmd_set_auto_failover_enabled(
    state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let app_type = required_str(args, "appType")?.to_string();
    let enabled = bool_or_default(args, "enabled", false);

    // When enabling, auto-add current provider to queue if empty
    let p1_provider_id = if enabled {
        let queue = state
            .db
            .get_failover_queue(&app_type)
            .map_err(|e| e.to_string())?;

        if queue.is_empty() {
            let current_id = crate::settings::get_effective_current_provider(
                &state.db,
                &app_type.parse().map_err(|_| format!("Invalid app type: {app_type}"))?,
            )
            .map_err(|e| e.to_string())?;

            if let Some(ref current_id) = current_id {
                state
                    .db
                    .add_to_failover_queue(&app_type, current_id)
                    .map_err(|e| e.to_string())?;
            }
        }

        // Re-read queue after potential addition
        let queue = state
            .db
            .get_failover_queue(&app_type)
            .map_err(|e| e.to_string())?;
        queue
            .first()
            .map(|item| item.provider_id.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };

    // Update the config in DB
    let mut config = state
        .db
        .get_proxy_config_for_app(&app_type)
        .await
        .map_err(|e| e.to_string())?;
    config.auto_failover_enabled = enabled;
    state
        .db
        .update_proxy_config_for_app(config)
        .await
        .map_err(|e| e.to_string())?;

    // Switch to P1 if enabling
    if enabled && !p1_provider_id.is_empty() {
        state
            .proxy_service
            .switch_proxy_target(&app_type, &p1_provider_id)
            .await?;
    }

    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_global_proxy_config(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let config = state.db.get_global_proxy_config().await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(config).map_err(|e| e.to_string())?)
}

async fn cmd_update_global_proxy_config(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let config: cc_switch_lib::proxy::types::GlobalProxyConfig = args
        .and_then(|a| a.get("config"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: config".to_string())?;
    state.db.update_global_proxy_config(config).await.map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_global_proxy_url(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let url: Option<String> = state.db.get_global_proxy_url().ok().flatten();
    Ok(serde_json::to_value(url).map_err(|e| e.to_string())?)
}

async fn cmd_set_global_proxy_url(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let url = optional_str_owned(args, "url");
    state.db.set_global_proxy_url(url.as_deref()).map_err(|e| e.to_string())?;
    cc_switch_lib::initialize_global_proxy(state);
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Session Commands =====================

async fn cmd_list_sessions(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let sessions = tokio::task::spawn_blocking(cc_switch_lib::session_manager::scan_sessions)
        .await
        .map_err(|e| format!("Failed to scan sessions: {e}"))?;
    Ok(serde_json::to_value(sessions).map_err(|e| e.to_string())?)
}

async fn cmd_get_session_messages(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let provider_id = required_str(args, "providerId")?.to_string();
    let source_path = required_str(args, "sourcePath")?.to_string();
    let messages = tokio::task::spawn_blocking(move || {
        cc_switch_lib::session_manager::load_messages(&provider_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to load messages: {e}"))?
    .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(messages).map_err(|e| e.to_string())?)
}

async fn cmd_delete_session(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let provider_id = required_str(args, "providerId")?.to_string();
    let session_id = required_str(args, "sessionId")?.to_string();
    let source_path = required_str(args, "sourcePath")?.to_string();
    let result = tokio::task::spawn_blocking(move || {
        cc_switch_lib::session_manager::delete_session(&provider_id, &session_id, &source_path)
    })
    .await
    .map_err(|e| format!("Failed to delete session: {e}"))?
    .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": result }))
}

// ===================== Usage Commands =====================

async fn cmd_get_usage_summary(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let start_date = args.and_then(|a| a.get("startDate")).and_then(|v| v.as_i64());
    let end_date = args.and_then(|a| a.get("endDate")).and_then(|v| v.as_i64());
    let app_type = optional_str(args, "appType");
    let summary = state.db.get_usage_summary(start_date, end_date, app_type).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(summary).map_err(|e| e.to_string())?)
}

async fn cmd_get_usage_trends(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let start_date = args.and_then(|a| a.get("startDate")).and_then(|v| v.as_i64());
    let end_date = args.and_then(|a| a.get("endDate")).and_then(|v| v.as_i64());
    let app_type = optional_str(args, "appType");
    let trends = state.db.get_daily_trends(start_date, end_date, app_type).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(trends).map_err(|e| e.to_string())?)
}

async fn cmd_get_request_logs(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let mut filters: cc_switch_lib::services::usage_stats::LogFilters = args
        .and_then(|a| a.get("filters"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    // Frontend sends camelCase field "appType", but LogFilters uses snake_case "app_type".
    // Tauri IPC handles the rename automatically, but web-server receives raw JSON.
    if let Some(fv) = args.and_then(|a| a.get("filters")) {
        if let Some(at) = fv.get("appType").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            filters.app_type = Some(at.to_string());
        }
    }
    let page = args.and_then(|a| a.get("page")).and_then(|v| v.as_u64()).unwrap_or(1) as u32;
    let page_size = args.and_then(|a| a.get("pageSize")).and_then(|v| v.as_u64()).unwrap_or(50) as u32;
    let logs = state.db.get_request_logs(&filters, page, page_size).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(logs).map_err(|e| e.to_string())?)
}

async fn cmd_get_provider_stats(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = optional_str(args, "appType");
    let start_date = i64_or_default(args, "startDate");
    let end_date = i64_or_default(args, "endDate");
    let stats = state.db.get_provider_stats(start_date, end_date, app_type).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(stats).map_err(|e| e.to_string())?)
}

async fn cmd_get_model_stats(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type = optional_str(args, "appType");
    let start_date = i64_or_default(args, "startDate");
    let end_date = i64_or_default(args, "endDate");
    let stats = state.db.get_model_stats(start_date, end_date, app_type).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(stats).map_err(|e| e.to_string())?)
}

// ===================== Model Pricing Commands =====================

async fn cmd_get_model_pricing(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let pricing = state.db.get_model_pricing_records().map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(pricing).map_err(|e| e.to_string())?)
}

async fn cmd_update_model_pricing(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let args = args.ok_or_else(|| "Missing arguments".to_string())?;
    let record = ModelPricingRecord {
        model_id: required_str(Some(args), "modelId")?.to_string(),
        display_name: required_str(Some(args), "displayName")?.to_string(),
        input_cost_per_million: required_str(Some(args), "inputCost")?.to_string(),
        output_cost_per_million: required_str(Some(args), "outputCost")?.to_string(),
        cache_read_cost_per_million: required_str(Some(args), "cacheReadCost")?.to_string(),
        cache_creation_cost_per_million: required_str(Some(args), "cacheCreationCost")?.to_string(),
    };
    state.db.update_model_pricing_record(&record).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_delete_model_pricing(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let model_id = required_str(args, "modelId")?;
    state.db.delete_model_pricing_record(model_id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Prompt Commands =====================

async fn cmd_get_prompts(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let prompts = PromptService::get_prompts(state, app).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(prompts).map_err(|e| e.to_string())?)
}

async fn cmd_upsert_prompt(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let prompt: cc_switch_lib::prompt::Prompt = args
        .and_then(|a| a.get("prompt"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: prompt".to_string())?;
    let id = prompt.id.clone();
    PromptService::upsert_prompt(state, app, &id, prompt).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_delete_prompt(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = required_str(args, "id")?;
    PromptService::delete_prompt(state, app, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_enable_prompt(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let id = required_str(args, "id")?;
    PromptService::enable_prompt(state, app, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Skill Commands =====================

async fn cmd_get_installed_skills(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let skills = cc_switch_lib::services::SkillService::get_all_installed(&state.db)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(skills).map_err(|e| e.to_string())?)
}

async fn cmd_discover_available_skills(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let skill_service = cc_switch_lib::services::SkillService::new();
    let repos = state.db.get_skill_repos().map_err(|e| e.to_string())?;
    let skills = skill_service.discover_available(repos).await.map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(skills).map_err(|e| e.to_string())?)
}

async fn cmd_scan_unmanaged_skills(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let skills = cc_switch_lib::services::SkillService::scan_unmanaged(&state.db)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(skills).map_err(|e| e.to_string())?)
}

async fn cmd_import_skills_from_apps(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let imports: Vec<cc_switch_lib::services::skill::ImportSkillSelection> = args
        .and_then(|a| a.get("imports"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: imports".to_string())?;

    let result = cc_switch_lib::services::SkillService::import_from_apps(&state.db, imports)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

async fn cmd_migrate_skill_storage(
    state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    use cc_switch_lib::services::skill::SkillStorageLocation;

    let target_str = required_str(args, "target")?;
    let target: SkillStorageLocation =
        serde_json::from_value(Value::String(target_str.to_string()))
            .map_err(|e| format!("Invalid storage location: {e}"))?;

    let result =
        cc_switch_lib::services::SkillService::migrate_storage(&state.db, target)
            .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

async fn cmd_install_skills_from_zip(
    state: &AppState,
    args: Option<&Value>,
) -> Result<Value, String> {
    let content = required_str(args, "content")?.to_string();
    let current_app_str = required_str(args, "currentApp")?;
    let app_type = AppType::from_str(current_app_str)
        .map_err(|_| format!("Invalid app type: {current_app_str}"))?;

    // Decode base64 to bytes
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&content)
        .map_err(|e| format!("Base64 decode failed: {e}"))?;

    // Write to temp file
    let temp_dir = std::env::temp_dir().join(format!("cc-switch-zip-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let zip_path = temp_dir.join("upload.zip");
    std::fs::write(&zip_path, &bytes).map_err(|e| e.to_string())?;

    let result = cc_switch_lib::services::SkillService::install_from_zip(
        &state.db,
        &zip_path,
        &app_type,
    )
    .map_err(|e| {
        let _ = std::fs::remove_dir_all(&temp_dir);
        e.to_string()
    })?;

    // Cleanup temp file
    let _ = std::fs::remove_dir_all(&temp_dir);

    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

async fn cmd_install_skill(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let skill: cc_switch_lib::services::DiscoverableSkill = args
        .and_then(|a| a.get("skill"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: skill".to_string())?;
    let current_app_str = required_str(args, "currentApp")?;
    let current_app = AppType::from_str(current_app_str)
        .map_err(|_| format!("Invalid app type: {current_app_str}"))?;
    let skill_service = cc_switch_lib::services::SkillService::new();
    let installed = skill_service.install(&state.db, &skill, &current_app)
        .await
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(installed).map_err(|e| e.to_string())?)
}

async fn cmd_uninstall_skill(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let id = required_str(args, "id")?;
    let result = cc_switch_lib::services::SkillService::uninstall(&state.db, id)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(result).map_err(|e| e.to_string())?)
}

async fn cmd_toggle_skill_app(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let id = required_str(args, "id")?;
    let app_type_str = required_str(args, "app")?;
    let app_type = AppType::from_str(app_type_str)
        .map_err(|_| format!("Invalid app type: {app_type_str}"))?;
    let enabled = bool_or_default(args, "enabled", true);
    cc_switch_lib::services::SkillService::toggle_app(&state.db, id, &app_type, enabled)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Backup Commands =====================

async fn cmd_list_db_backups(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let backups = cc_switch_lib::database::Database::list_backups().map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(backups).map_err(|e| e.to_string())?)
}

async fn cmd_restore_db_backup(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let filename = required_str(args, "filename")?;
    let result = state.db.restore_from_backup(filename).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "result": result }))
}

/// Import SQL content — used by web-server mode where there's no filesystem path
async fn cmd_import_sql_content(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let content = required_str(args, "content")?;
    let backup_id = state
        .db
        .import_sql_string(content)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "success": true,
        "message": "SQL imported successfully",
        "backupId": backup_id,
    }))
}

/// Export SQL as string content — used by web-server mode (triggers browser download)
async fn cmd_export_sql_string(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let sql = state
        .db
        .export_sql_string()
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({
        "success": true,
        "content": sql,
    }))

}


// ===================== Claude Plugin Commands =====================

async fn cmd_apply_claude_plugin_config(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let official = args
        .and_then(|a| a.get("official"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if official {
        cc_switch_lib::claude_plugin::clear_claude_config()
            .map_err(|e| e.to_string())?;
    } else {
        cc_switch_lib::claude_plugin::write_claude_config()
            .map_err(|e| e.to_string())?;
    }
    Ok(Value::Bool(true))
}

async fn cmd_is_claude_plugin_applied(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let applied = cc_switch_lib::claude_plugin::is_claude_config_applied()
        .map_err(|e| e.to_string())?;
    Ok(Value::Bool(applied))
}

async fn cmd_apply_claude_onboarding_skip(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    cc_switch_lib::claude_mcp::set_has_completed_onboarding()
        .map_err(|e| e.to_string())?;
    Ok(Value::Bool(true))
}

async fn cmd_clear_claude_onboarding_skip(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    cc_switch_lib::claude_mcp::clear_has_completed_onboarding()
        .map_err(|e| e.to_string())?;
    Ok(Value::Bool(true))
}

// ===================== Misc Commands =====================

async fn cmd_get_config_dir(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let dir = config::get_app_config_dir();
    Ok(serde_json::json!({ "path": dir.to_string_lossy().to_string() }))
}

async fn cmd_get_tool_versions(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    // Return empty array — tool version detection requires running shell commands
    // (claude --version, codex --version, etc.) which is out of scope for web-server.
    // The UI gracefully shows "Not installed" for all tools.
    Ok(serde_json::json!([]))
}

async fn cmd_read_live_provider_settings(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let settings = ProviderService::read_live_settings(app).map_err(|e| e.to_string())?;
    Ok(settings)
}

async fn cmd_pick_directory(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    // No native file dialog available in web-server mode.
    // The frontend mock layer intercepts this and shows a directory browser dialog.
    // Return null so the caller doesn't silently update to a wrong path.
    Ok(Value::Null)
}

async fn cmd_list_directory(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let path = required_str(args, "path")?;

    let read_dir = std::fs::read_dir(path)
        .map_err(|e| format!("无法读取目录: {e}"))?;

    let mut directories: Vec<String> = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|e| format!("读取目录项失败: {e}"))?;
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            if let Ok(name) = entry.file_name().into_string() {
                directories.push(name);
            }
        }
    }
    directories.sort();

    let parent = if path == "/" {
        None
    } else {
        std::path::Path::new(path).parent().map(|p| p.to_string_lossy().to_string())
    };

    Ok(serde_json::json!({
        "path": path,
        "parent": parent,
        "directories": directories,
    }))
}

async fn cmd_set_terminal_env(_state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    // Called by native-shell at startup to pass its desktop session environment
    if let Some(obj) = args.and_then(|a| a.as_object()) {
        let mut env = TERMINAL_ENV.lock().unwrap();
        for (k, v) in obj {
            if let Some(s) = v.as_str() {
                env.insert(k.clone(), s.to_string());
            }
        }
        log::info!("Terminal env updated: DISPLAY={}, XAUTHORITY present={}",
            env.get("DISPLAY").unwrap_or(&"?".to_string()),
            env.contains_key("XAUTHORITY"));
    }
    Ok(serde_json::json!({"ok": true}))
}

async fn cmd_get_custom_endpoints(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let provider_id = required_str(args, "providerId")?;
    let endpoints = ProviderService::get_custom_endpoints(state, app, provider_id).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(endpoints).map_err(|e| e.to_string())?)
}

async fn cmd_add_custom_endpoint(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let provider_id = required_str(args, "providerId")?;
    let url = required_str(args, "url")?.to_string();
    ProviderService::add_custom_endpoint(state, app, provider_id, url).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_remove_custom_endpoint(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let provider_id = required_str(args, "providerId")?;
    let url = required_str(args, "url")?.to_string();
    ProviderService::remove_custom_endpoint(state, app, provider_id, url).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_get_common_config_snippet(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = required_str(args, "app")?;
    let snippet = state.db.get_config_snippet(app).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "snippet": snippet }))
}

async fn cmd_set_common_config_snippet(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = required_str(args, "app")?;
    let snippet = args.and_then(|a| a.get("snippet")).and_then(|v| v.as_str());
    state.db.set_config_snippet(app, snippet.map(String::from)).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_extract_common_config_snippet(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app = parse_app_type(args)?;
    let snippet = ProviderService::extract_common_config_snippet(state, app).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "snippet": snippet }))
}

// ===================== Universal Provider Commands =====================

async fn cmd_get_universal_providers(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let providers = ProviderService::list_universal(state).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(providers).map_err(|e| e.to_string())?)
}

async fn cmd_upsert_universal_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let provider: cc_switch_lib::provider::UniversalProvider = args
        .and_then(|a| a.get("provider"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: provider".to_string())?;
    ProviderService::upsert_universal(state, provider).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_delete_universal_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let id = required_str(args, "id")?;
    ProviderService::delete_universal(state, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

async fn cmd_sync_universal_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let id = required_str(args, "id")?;
    ProviderService::sync_universal_to_apps(state, id).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Live provider access commands =====================

async fn cmd_get_openclaw_live_provider_ids(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let providers = cc_switch_lib::openclaw_config::get_providers().map_err(|e| e.to_string())?;
    let ids: Vec<String> = providers.keys().cloned().collect();
    Ok(serde_json::json!(ids))
}

async fn cmd_get_hermes_live_provider_ids(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let providers = cc_switch_lib::hermes_config::get_providers().map_err(|e| e.to_string())?;
    let ids: Vec<String> = providers.keys().cloned().collect();
    Ok(serde_json::json!(ids))
}

// ===================== System / Shutdown =====================

async fn cmd_shutdown(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let mut guard = SHUTDOWN_TX.lock().unwrap();
    if let Some(tx) = guard.take() {
        let _ = tx.send(());
        log::info!("Shutdown signal sent, server will stop.");
    }
    Ok(serde_json::json!({"ok": true}))
}

// ===================== Autostart (XDG) =====================

fn autostart_desktop_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/root".to_string());
    std::path::PathBuf::from(home).join(".config/autostart/cc-switch.desktop")
}

fn autostart_desktop_content() -> String {
    r#"[Desktop Entry]
Type=Application
Name=CC Switch
Comment=AI CLI Tools Configuration Manager
Exec=/usr/bin/cc-switch-web
Icon=cc-switch-web
Terminal=false
Categories=Utility;
X-GNOME-Autostart-enabled=true
"#.to_string()
}

async fn cmd_enable_autostart(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let path = autostart_desktop_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create autostart directory: {e}"))?;
    }
    std::fs::write(&path, autostart_desktop_content())
        .map_err(|e| format!("Failed to enable autostart: {e}"))?;
    Ok(serde_json::json!({"ok": true}))
}

async fn cmd_disable_autostart(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let path = autostart_desktop_path();
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to disable autostart: {e}"))?;
    }
    Ok(serde_json::json!({"ok": true}))
}

async fn cmd_get_autostart_status(_state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let enabled = autostart_desktop_path().exists();
    Ok(serde_json::json!({"enabled": enabled}))
}

// ===================== Terminal Command =====================

async fn cmd_open_terminal(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    use std::os::unix::fs::PermissionsExt;

    let app = parse_app_type(args)?;
    let provider_id = args
        .and_then(|a| a.get("providerId"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing required argument: providerId".to_string())?;

    // Get provider settings
    let providers = cc_switch_lib::services::provider::ProviderService::list(state, app.clone())
        .map_err(|e| e.to_string())?;
    let provider = providers.get(provider_id).ok_or_else(|| format!("Provider {provider_id} not found"))?;

    // Build env vars for the terminal script
    let settings_obj = provider.settings_config.as_object().cloned().unwrap_or_default();
    let mut env_vars: Vec<(String, String)> = Vec::new();

    // Extract env block
    if let Some(env) = settings_obj.get("env").and_then(|v| v.as_object()) {
        for (k, v) in env {
            if let Some(s) = v.as_str() {
                env_vars.push((k.clone(), s.to_string()));
            }
        }
    }

    // Claude codex / gemini specific key mappings
    match app {
        AppType::Codex => {
            if let Some(auth) = settings_obj.get("auth").and_then(|v| v.as_str()) {
                env_vars.push(("OPENAI_API_KEY".to_string(), auth.to_string()));
            }
        }
        AppType::Gemini => {
            if let Some(key) = settings_obj.get("api_key").and_then(|v| v.as_str()) {
                env_vars.push(("GEMINI_API_KEY".to_string(), key.to_string()));
            }
        }
        _ => {}
    }

    // Build the terminal environment — prefer values from native-shell (via set_terminal_env),
    // then fall back to our own process env, then auto-detection.
    let saved_env = TERMINAL_ENV.lock().unwrap();
    let display = saved_env.get("DISPLAY").cloned()
        .or_else(|| std::env::var("DISPLAY").ok())
        .unwrap_or_else(|| ":1".to_string());  // Common fallback on Ubuntu 20.04
    let dbus_addr = saved_env.get("DBUS_SESSION_BUS_ADDRESS").cloned()
        .or_else(|| std::env::var("DBUS_SESSION_BUS_ADDRESS").ok())
        .unwrap_or_else(|| {
            let uid = unsafe { getuid() };
            format!("unix:path=/run/user/{uid}/bus")
        });
    let xdg_dir = saved_env.get("XDG_RUNTIME_DIR").cloned()
        .or_else(|| std::env::var("XDG_RUNTIME_DIR").ok())
        .unwrap_or_else(|| {
            let uid = unsafe { getuid() };
            format!("/run/user/{uid}")
        });
    let xauth = saved_env.get("XAUTHORITY").cloned()
        .or_else(|| std::env::var("XAUTHORITY").ok());

    log::info!("Terminal launcher: DISPLAY={display} DBUS={dbus_addr} XAUTH={}",
        xauth.as_deref().unwrap_or("(none)"));

    // Determine preferred terminal
    let preferred = settings::get_preferred_terminal();
    let default_terminals: [(&str, &[&str]); 9] = [
        ("gnome-terminal", &["--"]),
        ("konsole", &["-e"]),
        ("xfce4-terminal", &["-e"]),
        ("mate-terminal", &["--"]),
        ("lxterminal", &["-e"]),
        ("alacritty", &["-e"]),
        ("kitty", &["-e"]),
        ("ghostty", &["-e"]),
        ("x-terminal-emulator", &["-e"]),
    ];

    // Create temp script file
    let temp_dir = std::env::temp_dir();
    let script_file = temp_dir.join(format!("cc_switch_launcher_{}.sh", std::process::id()));
    let mut script = String::from("#!/bin/bash\ntrap 'rm -f \"${BASH_SOURCE[0]}\"' EXIT\n");

    // Add env vars (masks secrets in display)
    for (k, v) in &env_vars {
        script.push_str(&format!("export {}={:?}\n", k, v));
    }

    // Resolve cwd
    let cwd = args
        .and_then(|a| a.get("cwd"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    if let Some(dir) = cwd {
        script.push_str(&format!("cd {:?}\n", dir));
    }

    // Show provider context
    script.push_str("echo ''\n");
    script.push_str("echo '╔═══════════════════════════════════════╗'\n");
    script.push_str("echo '║     CC Switch - Provider Terminal    ║'\n");
    script.push_str(&format!("echo '║  Provider: {provider_id}'\n"));
    script.push_str("echo '╚═══════════════════════════════════════╝'\n");
    script.push_str("echo ''\n");
    // Show which env vars are set (mask sensitive values)
    for (k, _) in &env_vars {
        if k.to_lowercase().contains("key") || k.to_lowercase().contains("token") || k.to_lowercase().contains("secret") {
            script.push_str(&format!("echo \"  {k}=****\"\n"));
        } else {
            script.push_str(&format!("echo \"  {k}=${k}\"\n"));
        }
    }
    script.push_str("echo ''\n");
    script.push_str("echo 'Environment variables set. Starting interactive shell...'\n");
    script.push_str("echo ''\n");

    // Start interactive bash so the user gets their normal prompt (PS1, aliases, etc.)
    script.push_str("exec bash -i\n");

    std::fs::write(&script_file, &script)
        .map_err(|e| format!("Failed to write launch script: {e}"))?;
    std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o755))
        .map_err(|e| format!("Failed to set permissions: {e}"))?;

    // Try terminals in order: preferred first, then defaults
    let terminals: Vec<(&str, &[&str])> = if let Some(ref pref) = preferred {
        let mut list = default_terminals.to_vec();
        list.retain(|(name, _)| *name != pref.as_str());
        let mut result = vec![(pref.as_str(), default_terminals.iter().find(|(n, _)| *n == pref.as_str()).map(|(_, a)| *a).unwrap_or(&["-e"]))];
        result.extend(list);
        result
    } else {
        default_terminals.to_vec()
    };

    let mut last_err = String::new();
    for (term, args_list) in &terminals {
        let mut cmd = ProcessCommand::new(term);
        // Inject desktop session environment
        cmd.env("DISPLAY", &display);
        cmd.env("DBUS_SESSION_BUS_ADDRESS", &dbus_addr);
        cmd.env("XDG_RUNTIME_DIR", &xdg_dir);
        if let Some(ref xa) = xauth {
            cmd.env("XAUTHORITY", xa);
        }
        cmd.args(args_list.iter().chain(std::iter::once(&script_file.to_string_lossy().as_ref())));
        match cmd.spawn() {
            Ok(child) => {
                log::info!("Opened terminal: {term} (PID: {})", child.id());
                return Ok(serde_json::json!({"ok": true, "terminal": term}));
            }
            Err(e) => {
                last_err = format!("Terminal '{term}' failed: {e}");
                log::debug!("{last_err}");
            }
        }
    }

    Err(format!("No terminal emulator could launch a window. Tried: {} | Last error: {last_err}",
        terminals.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")))
}

// ===================== Stream Check Commands =====================

async fn cmd_stream_check_provider(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type_str = required_str(args, "appType")?;
    let app_type = AppType::from_str(app_type_str).map_err(|_| format!("Invalid app type: {app_type_str}"))?;
    let provider_id = required_str(args, "providerId")?.to_string();

    let config = state.db.get_stream_check_config().map_err(|e| e.to_string())?;
    let providers = state.db.get_all_providers(app_type.as_str()).map_err(|e| e.to_string())?;
    let provider = providers.get(&provider_id)
        .ok_or_else(|| format!("Provider {provider_id} not found"))?;

    let result = StreamCheckService::check_with_retry(
        &app_type,
        provider,
        &config,
        None,
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    let _ = state
        .db
        .save_stream_check_log(&provider_id, &provider.name, app_type.as_str(), &result);

    serde_json::to_value(&result).map_err(|e| e.to_string())
}

async fn cmd_stream_check_all_providers(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let app_type_str = required_str(args, "appType")?;
    let app_type = AppType::from_str(app_type_str).map_err(|_| format!("Invalid app type: {app_type_str}"))?;
    let proxy_targets_only = bool_or_default(args, "proxyTargetsOnly", false);

    let config = state.db.get_stream_check_config().map_err(|e| e.to_string())?;
    let providers = state.db.get_all_providers(app_type.as_str()).map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    let allowed_ids: Option<HashSet<String>> = if proxy_targets_only {
        let mut ids = HashSet::new();
        if let Ok(Some(current_id)) = state.db.get_current_provider(app_type.as_str()) {
            ids.insert(current_id);
        }
        if let Ok(queue) = state.db.get_failover_queue(app_type.as_str()) {
            for item in queue {
                ids.insert(item.provider_id);
            }
        }
        Some(ids)
    } else {
        None
    };

    for (id, provider) in &providers {
        if let Some(ids) = &allowed_ids {
            if !ids.contains(id) {
                continue;
            }
        }

        let result = StreamCheckService::check_with_retry(
            &app_type,
            provider,
            &config,
            None,
            None,
            None,
        )
        .await
        .unwrap_or_else(|e| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            StreamCheckResult {
                status: HealthStatus::Failed,
                success: false,
                message: e.to_string(),
                response_time_ms: None,
                http_status: None,
                model_used: String::new(),
                tested_at: now,
                retry_count: 0,
                error_category: None,
            }
        });

        let _ = state
            .db
            .save_stream_check_log(id, &provider.name, app_type.as_str(), &result);

        results.push((id.clone(), result));
    }

    serde_json::to_value(&results).map_err(|e| e.to_string())
}

async fn cmd_get_stream_check_config(state: &AppState, _args: Option<&Value>) -> Result<Value, String> {
    let config = state.db.get_stream_check_config().map_err(|e| e.to_string())?;
    serde_json::to_value(&config).map_err(|e| e.to_string())
}

async fn cmd_save_stream_check_config(state: &AppState, args: Option<&Value>) -> Result<Value, String> {
    let config: StreamCheckConfig = args
        .and_then(|a| a.get("config"))
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .ok_or_else(|| "Missing or invalid argument: config".to_string())?;
    state.db.save_stream_check_config(&config).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "success": true }))
}

// ===================== Route dispatch =====================

async fn route_command(state: &AppState, command: &str, args: Option<&Value>) -> Result<Value, String> {
    match command {
        // Provider
        "get_providers" => cmd_get_providers(state, args).await,
        "get_current_provider" => cmd_get_current_provider(state, args).await,
        "add_provider" => cmd_add_provider(state, args).await,
        "update_provider" => cmd_update_provider(state, args).await,
        "delete_provider" => cmd_delete_provider(state, args).await,
        "switch_provider" => cmd_switch_provider(state, args).await,
        "disable_provider" => cmd_disable_provider(state, args).await,
        "remove_provider_from_live_config" => cmd_remove_provider_from_live(state, args).await,
        "sync_current_providers_live" => cmd_sync_current_providers_live(state, args).await,
        "update_providers_sort_order" => cmd_update_providers_sort_order(state, args).await,

        // Settings
        "get_settings" => cmd_get_settings(state, args).await,
        "save_settings" => cmd_save_settings(state, args).await,
        "get_app_config_dir_override" => cmd_get_app_config_dir_override(state, args).await,
        "set_app_config_dir_override" => cmd_set_app_config_dir_override(state, args).await,
        "webdav_test_connection" => cmd_webdav_test_connection(state, args).await,
        "webdav_sync_save_settings" => cmd_webdav_sync_save_settings(state, args).await,
        "webdav_sync_upload" => cmd_webdav_sync_upload(state, args).await,
        "webdav_sync_download" => cmd_webdav_sync_download(state, args).await,
        "webdav_sync_fetch_remote_info" => cmd_webdav_sync_fetch_remote_info(state, args).await,

        // MCP
        "get_mcp_servers" => cmd_get_mcp_servers(state, args).await,
        "upsert_mcp_server" => cmd_upsert_mcp_server(state, args).await,
        "delete_mcp_server" => cmd_delete_mcp_server(state, args).await,
        "toggle_mcp_app" => cmd_toggle_mcp_app(state, args).await,
        "import_mcp_from_apps" => cmd_import_mcp_from_apps(state, args).await,

        // Proxy
        "get_proxy_status" => cmd_get_proxy_status(state, args).await,
        "get_proxy_config" => cmd_get_proxy_config(state, args).await,
        "update_proxy_config" => cmd_update_proxy_config(state, args).await,
        "start_proxy_server" => cmd_start_proxy_server(state, args).await,
        "stop_proxy_with_restore" => cmd_stop_proxy_with_restore(state, args).await,
        "is_proxy_running" => cmd_is_proxy_running(state, args).await,
        "is_live_takeover_active" => cmd_is_live_takeover_active(state, args).await,
        "get_proxy_takeover_status" => cmd_get_proxy_takeover_status(state, args).await,
        "set_proxy_takeover_for_app" => cmd_set_proxy_takeover_for_app(state, args).await,
        "switch_proxy_provider" => cmd_switch_proxy_provider(state, args).await,
        "get_proxy_config_for_app" => cmd_get_proxy_config_for_app(state, args).await,
        "update_proxy_config_for_app" => cmd_update_proxy_config_for_app(state, args).await,
        "get_default_cost_multiplier" => cmd_get_default_cost_multiplier(state, args).await,
        "set_default_cost_multiplier" => cmd_set_default_cost_multiplier(state, args).await,
        "get_pricing_model_source" => cmd_get_pricing_model_source(state, args).await,
        "set_pricing_model_source" => cmd_set_pricing_model_source(state, args).await,
        "get_failover_queue" => cmd_get_failover_queue(state, args).await,
        "add_to_failover_queue" => cmd_add_to_failover_queue(state, args).await,
        "remove_from_failover_queue" => cmd_remove_from_failover_queue(state, args).await,
        "get_available_providers_for_failover" => cmd_get_available_providers_for_failover(state, args).await,
        "get_auto_failover_enabled" => cmd_get_auto_failover_enabled(state, args).await,
        "set_auto_failover_enabled" => cmd_set_auto_failover_enabled(state, args).await,
        "get_global_proxy_config" => cmd_get_global_proxy_config(state, args).await,
        "update_global_proxy_config" => cmd_update_global_proxy_config(state, args).await,
        "get_global_proxy_url" => cmd_get_global_proxy_url(state, args).await,
        "set_global_proxy_url" => cmd_set_global_proxy_url(state, args).await,

        // Session
        "list_sessions" => cmd_list_sessions(state, args).await,
        "get_session_messages" => cmd_get_session_messages(state, args).await,
        "delete_session" => cmd_delete_session(state, args).await,

        // Usage
        "get_usage_summary" => cmd_get_usage_summary(state, args).await,
        "get_usage_trends" => cmd_get_usage_trends(state, args).await,
        "get_request_logs" => cmd_get_request_logs(state, args).await,
        "get_provider_stats" => cmd_get_provider_stats(state, args).await,
        "get_model_stats" => cmd_get_model_stats(state, args).await,
        "get_model_pricing" => cmd_get_model_pricing(state, args).await,
        "update_model_pricing" => cmd_update_model_pricing(state, args).await,
        "delete_model_pricing" => cmd_delete_model_pricing(state, args).await,

        // Prompts
        "get_prompts" => cmd_get_prompts(state, args).await,
        "upsert_prompt" => cmd_upsert_prompt(state, args).await,
        "delete_prompt" => cmd_delete_prompt(state, args).await,
        "enable_prompt" => cmd_enable_prompt(state, args).await,

        // Skills
        "get_installed_skills" => cmd_get_installed_skills(state, args).await,
        "discover_available_skills" => cmd_discover_available_skills(state, args).await,
        "install_skill" | "install_skill_unified" => cmd_install_skill(state, args).await,
        "uninstall_skill" | "uninstall_skill_unified" => cmd_uninstall_skill(state, args).await,
        "toggle_skill_app" => cmd_toggle_skill_app(state, args).await,
        "scan_unmanaged_skills" => cmd_scan_unmanaged_skills(state, args).await,
        "import_skills_from_apps" => cmd_import_skills_from_apps(state, args).await,
        "migrate_skill_storage" => cmd_migrate_skill_storage(state, args).await,
        "install_skills_from_zip" => cmd_install_skills_from_zip(state, args).await,

        // Backups
        "list_db_backups" => cmd_list_db_backups(state, args).await,
        "restore_db_backup" => cmd_restore_db_backup(state, args).await,
        "import_sql_content" => cmd_import_sql_content(state, args).await,
        "export_sql_string" => cmd_export_sql_string(state, args).await,

        // Universal providers
        "get_universal_providers" => cmd_get_universal_providers(state, args).await,
        "upsert_universal_provider" => cmd_upsert_universal_provider(state, args).await,
        "delete_universal_provider" => cmd_delete_universal_provider(state, args).await,
        "sync_universal_provider" => cmd_sync_universal_provider(state, args).await,

        // Custom endpoints
        "get_custom_endpoints" => cmd_get_custom_endpoints(state, args).await,
        "add_custom_endpoint" => cmd_add_custom_endpoint(state, args).await,
        "remove_custom_endpoint" => cmd_remove_custom_endpoint(state, args).await,

        // Common config snippets
        "get_common_config_snippet" => cmd_get_common_config_snippet(state, args).await,
        "set_common_config_snippet" => cmd_set_common_config_snippet(state, args).await,
        "extract_common_config_snippet" => cmd_extract_common_config_snippet(state, args).await,

        // Live provider IDs
        "get_openclaw_live_provider_ids" => cmd_get_openclaw_live_provider_ids(state, args).await,
        "get_hermes_live_provider_ids" => cmd_get_hermes_live_provider_ids(state, args).await,

        // Misc
        "health" => Ok(serde_json::json!({
            "status": "ok",
            "version": env!("CARGO_PKG_VERSION"),
        })),
        "get_config_dir" => cmd_get_config_dir(state, args).await,
        "get_tool_versions" => cmd_get_tool_versions(state, args).await,
        "read_live_provider_settings" => cmd_read_live_provider_settings(state, args).await,
        "pick_directory" => cmd_pick_directory(state, args).await,
        "list_directory" => cmd_list_directory(state, args).await,
        "set_terminal_env" => cmd_set_terminal_env(state, args).await,

        // System / Shutdown
        "shutdown" => cmd_shutdown(state, args).await,

        // Autostart
        "enable_autostart" => cmd_enable_autostart(state, args).await,
        "disable_autostart" => cmd_disable_autostart(state, args).await,
        "get_autostart_status" => cmd_get_autostart_status(state, args).await,
        // Frontend-compatible aliases (used by settings API)
        "set_auto_launch" => {
            let enabled = args.and_then(|a| a.get("enabled")).and_then(|v| v.as_bool()).unwrap_or(false);
            if enabled {
                cmd_enable_autostart(state, args).await
            } else {
                cmd_disable_autostart(state, args).await
            }
        },
        "get_auto_launch_status" => {
            cmd_get_autostart_status(state, args).await.map(|v| v.get("enabled").cloned().unwrap_or(Value::Bool(false)))
        },

        // Terminal
        "open_provider_terminal" => cmd_open_terminal(state, args).await,

        // Claude Plugin
        "apply_claude_plugin_config" => cmd_apply_claude_plugin_config(state, args).await,
        "is_claude_plugin_applied" => cmd_is_claude_plugin_applied(state, args).await,
        "apply_claude_onboarding_skip" => cmd_apply_claude_onboarding_skip(state, args).await,
        "clear_claude_onboarding_skip" => cmd_clear_claude_onboarding_skip(state, args).await,

        // Stream Check
        "stream_check_provider" => cmd_stream_check_provider(state, args).await,
        "stream_check_all_providers" => cmd_stream_check_all_providers(state, args).await,
        "get_stream_check_config" => cmd_get_stream_check_config(state, args).await,
        "save_stream_check_config" => cmd_save_stream_check_config(state, args).await,

        _ => Err(format!("Unknown command: {command}")),
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    log::info!("CC Switch Web Server starting...");

    if let Some(path) = read_app_config_dir_override() {
        std::env::set_var("CC_SWITCH_APP_CONFIG_DIR", path);
    }

    // Initialize database
    let app_config_dir = config::get_app_config_dir();
    log::info!("App config directory: {}", app_config_dir.display());

    // Ensure the config directory exists
    std::fs::create_dir_all(&app_config_dir).expect("Failed to create config directory");

    let db = Arc::new(Database::init().expect("Failed to initialize database"));
    log::info!("Database initialized successfully");

    // Initialize services and app state
    let state = Arc::new(AppState::new(db.clone()));

    // Initialize default data (providers, MCP, prompts, etc.)
    cc_switch_lib::initialize_db(&state);

    // Direct scan+import: attempt to import any skills found in app directories
    // (e.g. ~/.claude/skills/, ~/.codex/skills/) that weren't caught by
    // the SSOT migration path above.
    if let Ok(unmanaged) = cc_switch_lib::services::SkillService::scan_unmanaged(&state.db) {
        if !unmanaged.is_empty() {
            let imports: Vec<cc_switch_lib::services::skill::ImportSkillSelection> = unmanaged
                .iter()
                .map(|s| {
                    let mut apps = cc_switch_lib::app_config::SkillApps::default();
                    for app_label in &s.found_in {
                        match app_label.as_str() {
                            "claude" => apps.claude = true,
                            "codex" => apps.codex = true,
                            "gemini" => apps.gemini = true,
                            "opencode" => apps.opencode = true,
                            "hermes" => apps.hermes = true,
                            _ => {}
                        }
                    }
                    cc_switch_lib::services::skill::ImportSkillSelection {
                        directory: s.directory.clone(),
                        apps,
                    }
                })
                .collect();
            match cc_switch_lib::services::SkillService::import_from_apps(&state.db, imports) {
                Ok(imported) => {
                    log::info!("✓ Directly imported {} skill(s) from app directories", imported.len());
                }
                Err(e) => {
                    log::warn!("✗ Direct import of skills failed: {e}");
                }
            }
        } else {
            log::info!("○ No unmanaged skills found in app directories");
        }
    }

    // Initialize global proxy
    cc_switch_lib::initialize_global_proxy(&state);

    // Restore proxy state from last session
    cc_switch_lib::restore_proxy_state_on_startup(&state).await;

    // Initialize common config snippets
    cc_switch_lib::initialize_common_config_snippets_non_tauri(&state);

    // Start background tasks
    let bg_db = state.db.clone();
    start_background_tasks(bg_db);

    // Resolve frontend dist directory
    let dist_dir = std::env::var("CC_SWITCH_DIST_DIR").unwrap_or_else(|_| {
        let installed = "/usr/share/cc-switch-web/dist";
        if std::path::Path::new(installed).is_dir() {
            installed.to_string()
        } else {
            "dist".to_string()
        }
    });
    log::info!("Serving static files from: {}", dist_dir);

    // Build axum router
    let app = Router::new()
        .route("/api/invoke", post(handle_invoke))
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024)) // 50 MB limit for SQL import
        .with_state(state.clone())
        .fallback_service(tower_http::services::ServeDir::new(&dist_dir));

    let addr = "127.0.0.1:2891";
    log::info!("HTTP server listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind address");

    // Set up graceful shutdown signal
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    {
        let mut guard = SHUTDOWN_TX.lock().unwrap();
        *guard = Some(shutdown_tx);
    }

    log::info!("CC Switch Web Server ready.");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown_rx.await.ok();
            log::info!("Shutdown signal received, stopping server...");
        })
        .await
        .expect("Server error");

    log::info!("Server stopped.");
}

fn start_background_tasks(db: Arc<Database>) {
    // Periodic maintenance (daily backup)
    let db1 = db.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(24 * 60 * 60));
        interval.tick().await; // Wait one period before first execution
        loop {
            interval.tick().await;
            if let Err(e) = db1.periodic_backup_if_needed() {
                log::warn!("Periodic maintenance failed: {e}");
            }
        }
    });

    // Session usage sync
    tokio::spawn(async move {
        // Wait a bit before initial sync to let the server settle
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
        if let Err(e) = cc_switch_lib::services::session_usage::sync_claude_session_logs(&db) {
            log::warn!("Session usage initial sync failed: {e}");
        }

        let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(e) = cc_switch_lib::services::session_usage::sync_claude_session_logs(&db) {
                log::warn!("Session usage periodic sync failed: {e}");
            }
        }
    });
}
