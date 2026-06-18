//! ZS-AI 服务端。
//!
//! 职责：接收 HTTP 请求 → 创建 Context → 注入配置 → 调用流水线 → 返回响应。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};

use axum::{
    extract::{Path, State, WebSocketUpgrade},
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use std::sync::mpsc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use zsai_core::{AppConfig, Context, DynamicLoader, run_pipeline};
use zsai_core::hot_reload::HotReloadManager;

// ============================================================
// AppState
// ============================================================

#[derive(Clone)]
struct AppState {
    loader: Arc<RwLock<DynamicLoader>>,
    config: Arc<AppConfig>,
    sessions: Arc<RwLock<SessionStore>>,
    _hot_reload: Option<Arc<RwLock<HotReloadManager>>>,
}

// ============================================================
// Session Store
// ============================================================

/// 内存中的 session 元信息。
#[derive(Debug, Clone, Serialize)]
struct SessionInfo {
    session_id: String,
    character: String,
    model: String,
    created_at: String,
    message_count: usize,
}

/// 简单的内存 session 管理器（后续可替换为持久化存储）。
#[derive(Default)]
struct SessionStore {
    sessions: HashMap<String, SessionInfo>,
}

impl SessionStore {
    fn create(&mut self, character: &str, model: &str) -> SessionInfo {
        let id = uuid_v7();
        let info = SessionInfo {
            session_id: id.clone(),
            character: character.to_string(),
            model: model.to_string(),
            created_at: chrono_now(),
            message_count: 0,
        };
        info!(session_id = %id, character = %character, "Session 已创建");
        self.sessions.insert(id, info.clone());
        info
    }

    fn get(&self, id: &str) -> Option<&SessionInfo> {
        self.sessions.get(id)
    }

    #[allow(dead_code)]
    fn update_count(&mut self, id: &str, count: usize) {
        if let Some(info) = self.sessions.get_mut(id) {
            info.message_count = count;
        }
    }

    fn delete(&mut self, id: &str) -> bool {
        let existed = self.sessions.remove(id).is_some();
        if existed {
            // 删除磁盘上的历史文件
            let path = format!("./data/history/{}.json", id);
            let _ = std::fs::remove_file(&path);
            info!(session_id = %id, "Session 已删除");
        }
        existed
    }
}

// ============================================================
// Helpers
// ============================================================

fn uuid_v7() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{:016x}-{:04x}", ts, rand_u16())
}

fn rand_u16() -> u16 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    RandomState::new().build_hasher().finish() as u16
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = duration.as_secs();
    let minutes = (secs / 60) % 60;
    let hours = (secs / 3600) % 24;
    let days = secs / 86400;
    let year = 1970 + (days / 365) as i64;
    let doy = (days % 365) as u32;
    let month = (doy / 30 + 1).min(12);
    let day = (doy % 30 + 1).min(31);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", year, month, day, hours, minutes, secs % 60)
}

/// 注入角色卡路径到 Context（优先显式路径，否则用 session 默认值）。
fn inject_character_card(ctx: &mut Context, card_path: &str, default: &str) {
    let path = if !card_path.is_empty() { card_path } else { default };
    ctx.set_custom_value("character_card.path", Value::String(path.to_string()));
}

/// 注入 API 配置到 Context。
fn inject_api_config(ctx: &mut Context, config: &AppConfig, model: &str) {
    let api_key = if !config.api.api_key.is_empty() {
        config.api.api_key.clone()
    } else {
        std::env::var("DEEPSEEK_API_KEY").unwrap_or_default()
    };
    let model = if !model.is_empty() { model } else { &config.api.model };
    ctx.set_custom_value("api_client.config", serde_json::json!({
        "api_type": config.api.api_type,
        "api_key": api_key,
        "base_url": config.api.base_url,
        "model": model,
        "stream": config.api.stream,
        "reasoning_effort": config.plugins.get("api_client")
            .and_then(|v| v.get("reasoning_effort"))
            .and_then(|v| v.as_str())
            .unwrap_or("medium"),
        "thinking_type": config.plugins.get("api_client")
            .and_then(|v| v.get("thinking_type"))
            .and_then(|v| v.as_str())
            .unwrap_or(""),
    }));
}

/// 执行流水线并返回统计。
fn run(ctx: &mut Context, state: &AppState) -> zsai_core::PipelineStats {
    let loader = state.loader.read().unwrap();
    let stage_map = loader.build_stage_map(&state.config.stages.order);
    run_pipeline(ctx, &state.config.stages, &stage_map)
}

// ============================================================
// Request / Response types
// ============================================================

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    character: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct SessionCreateRequest {
    #[serde(default)]
    character: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Deserialize)]
struct CardUpdateRequest {
    data: Value,
}

#[derive(Deserialize, Default)]
struct ConfigQuery {
    #[serde(default)]
    session_id: Option<String>,
}

// ============================================================
// Handlers
// ============================================================

/// POST /api/chat
async fn chat_handler(
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<ChatRequest>,
) -> (StatusCode, Json<Value>) {
    let session_id = body.session_id.unwrap_or_else(|| uuid_v7());
    let mut ctx = Context::new(&session_id);

    ctx.user_input = Some(body.message.clone());
    inject_character_card(&mut ctx, &body.character.unwrap_or_default(), &state.config.session.default_character);
    inject_api_config(&mut ctx, &state.config, &body.model.unwrap_or_default());

    let stats = run(&mut ctx, &state);

    let status = if ctx.abort { StatusCode::INTERNAL_SERVER_ERROR } else { StatusCode::OK };
    (status, Json(serde_json::json!({
        "session_id": session_id,
        "reply": ctx.ai_response,
        "error": ctx.get_custom_value("error").and_then(|v| v.get("message")).and_then(|m| m.as_str()),
        "usage": ctx.get_custom_value("api_client.token_usage"),
        "pipeline": {
            "stages": stats.stages_executed,
            "plugins_ok": stats.plugins_executed,
            "plugins_failed": stats.plugins_failed,
        }
    })))
}

/// POST /api/session
async fn session_create_handler(
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<SessionCreateRequest>,
) -> (StatusCode, Json<Value>) {
    let character = body.character.unwrap_or_else(|| state.config.session.default_character.clone());
    let model = body.model.unwrap_or_else(|| state.config.api.model.clone());
    let mut sessions = state.sessions.write().unwrap();
    let info = sessions.create(&character, &model);
    (StatusCode::CREATED, Json(serde_json::json!(info)))
}

/// GET /api/session/:id
async fn session_get_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    let sessions = state.sessions.read().unwrap();
    let hist_path = format!("./data/history/{}.json", session_id);
    let msg_count = if let Ok(content) = std::fs::read_to_string(&hist_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
            val.get("message_count").and_then(|v| v.as_u64()).unwrap_or(0) as usize
        } else { 0 }
    } else { 0 };

    match sessions.get(&session_id) {
        Some(info) => {
            let mut info = info.clone();
            info.message_count = msg_count;
            (StatusCode::OK, Json(serde_json::json!(info)))
        }
        None => {
            let info = SessionInfo {
                session_id: session_id.clone(),
                character: "unknown".into(),
                model: "unknown".into(),
                created_at: String::new(),
                message_count: msg_count,
            };
            (StatusCode::OK, Json(serde_json::json!(info)))
        }
    }
}

/// DELETE /api/session/:id
async fn session_delete_handler(
    State(state): State<AppState>,
    Path(session_id): Path<String>,
) -> StatusCode {
    let mut sessions = state.sessions.write().unwrap();
    if sessions.delete(&session_id) {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}

/// GET /api/card
async fn card_get_handler(
    State(state): State<AppState>,
    axum::extract::Query(query): axum::extract::Query<ConfigQuery>,
) -> (StatusCode, Json<Value>) {
    let session_id = query.session_id.unwrap_or_default();
    let mut ctx = Context::new(&session_id);
    inject_character_card(&mut ctx, "", &state.config.session.default_character);

    run(&mut ctx, &state);

    match ctx.get_custom_value("character_card.data") {
        Some(card) => (StatusCode::OK, Json(card.clone())),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "未找到角色卡数据"}))),
    }
}

/// PUT /api/card
async fn card_put_handler(
    axum::extract::Json(body): axum::extract::Json<CardUpdateRequest>,
) -> (StatusCode, Json<Value>) {
    let card_path = format!("./data/characters/_api_edit_{}.json", uuid_v7());
    if let Some(parent) = std::path::Path::new(&card_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&body.data) {
        Ok(json) => {
            if std::fs::write(&card_path, &json).is_ok() {
                (StatusCode::OK, Json(serde_json::json!({
                    "status": "ok",
                    "path": card_path,
                    "data": body.data
                })))
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "无法写入文件"})))
            }
        }
        Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("JSON 序列化失败: {}", e)}))),
    }
}

/// POST /api/card/import
async fn card_import_handler(
    axum::extract::Json(body): axum::extract::Json<Value>,
) -> (StatusCode, Json<Value>) {
    let json_str = match body.get("json") {
        Some(v) if v.is_string() => v.as_str().unwrap().to_string(),
        _ => {
            // 直接当作角色卡 JSON 处理
            body.to_string()
        }
    };

    // 保存为文件
    let name = body.get("data").and_then(|d| d.get("name"))
        .or_else(|| body.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("imported");
    let path = format!("./data/characters/{}.json", name);
    if let Some(parent) = std::path::Path::new(&path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::write(&path, &json_str) {
        Ok(()) => (StatusCode::CREATED, Json(serde_json::json!({"status": "ok", "path": path, "name": name}))),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": format!("无法保存: {}", e)}))),
    }
}

/// GET /api/config
async fn config_get_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(serde_json::json!({
        "stages": {
            "order": state.config.stages.order,
            "error_stage": state.config.stages.error_stage,
        },
        "api": {
            "api_type": state.config.api.api_type,
            "base_url": state.config.api.base_url,
            "model": state.config.api.model,
            "stream": state.config.api.stream,
            "api_key_configured": !state.config.api.api_key.is_empty()
                || std::env::var("DEEPSEEK_API_KEY").is_ok(),
        },
        "hot_reload": {
            "enabled": state.config.hot_reload.enabled,
            "plugin_dir": state.config.hot_reload.plugin_dir,
        },
        "plugins": state.config.plugins.keys().collect::<Vec<_>>(),
    })))
}

/// PUT /api/config — 更新配置（运行时生效，不写磁盘）。
async fn config_put_handler(
    State(state): State<AppState>,
    axum::extract::Json(body): axum::extract::Json<Value>,
) -> (StatusCode, Json<Value>) {
    // 更新内存中的配置（下次请求生效）
    let mut updated = false;

    if let Some(v) = body.get("api_key").and_then(|v| v.as_str()) {
        // 不更新内存中的 AppConfig（它是不可变的），
        // 而是通过 ctx.custom 在下一次请求中传递
        // 这里仅记录状态
        if !v.is_empty() {
            // 将 key 写入临时环境（仅当前进程生效）
            std::env::set_var("DEEPSEEK_API_KEY", v);
            updated = true;
            info!("API Key 已更新");
        }
    }

    (StatusCode::OK, Json(serde_json::json!({
        "status": if updated { "ok" } else { "no_change" },
        "note": "API Key 已更新（当前进程生效，重启后需重新设置或写入 config.toml）"
    })))
}

/// GET /api/history
async fn history_get_handler(
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let session_id = query.get("session_id").map(|s| s.as_str()).unwrap_or("");
    if session_id.is_empty() {
        // 列出所有 session
        let mut sessions = Vec::new();
        if let Ok(entries) = std::fs::read_dir("./data/history") {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        sessions.push(stem.to_string());
                    }
                }
            }
        }
        return (StatusCode::OK, Json(serde_json::json!({"sessions": sessions})));
    }

    let path = format!("./data/history/{}.json", session_id);
    match std::fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(val) => (StatusCode::OK, Json(val)),
            Err(e) => (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": format!("解析失败: {}", e)}))),
        },
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "历史记录不存在"}))),
    }
}

/// DELETE /api/history
async fn history_delete_handler(
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> StatusCode {
    let session_id = query.get("session_id").map(|s| s.as_str()).unwrap_or("");
    if session_id.is_empty() { return StatusCode::BAD_REQUEST; }

    let path = format!("./data/history/{}.json", session_id);
    match std::fs::remove_file(&path) {
        Ok(()) => StatusCode::NO_CONTENT,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

/// GET /health
async fn health_handler(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let loader = match state.loader.read() {
        Ok(l) => l,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"status": "error"}))),
    };
    let plugin_health: Vec<_> = loader.health_check_all()
        .into_iter()
        .map(|(name, h)| serde_json::json!({"name": name, "healthy": h.healthy, "message": h.message}))
        .collect();
    let all_ok = plugin_health.iter().all(|p| p["healthy"].as_bool().unwrap_or(false));
    (
        if all_ok { StatusCode::OK } else { StatusCode::SERVICE_UNAVAILABLE },
        Json(serde_json::json!({
            "status": if all_ok { "ok" } else { "degraded" },
            "plugin_count": loader.len(),
            "plugins": plugin_health,
        }))
    )
}

/// GET /ws/chat — WebSocket 流式对话。
async fn ws_chat_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

/// 创建 WebSocket Text 消息。
fn ws_msg(json: Value) -> axum::extract::ws::Message {
    axum::extract::ws::Message::Text(json.to_string().into())
}

async fn handle_ws(mut socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;

    // 等待客户端发送消息
    let user_text = loop {
        match socket.recv().await {
            Some(Ok(Message::Text(text))) => break text.to_string(),
            Some(Ok(Message::Close(_))) => return,
            Some(Err(e)) => {
                warn!(error = %e, "WebSocket 接收错误");
                return;
            }
            _ => {}
        }
    };

    // 解析消息
    let body: Value = match serde_json::from_str(&user_text) {
        Ok(v) => v,
        Err(_) => {
            let _ = socket.send(ws_msg(serde_json::json!({
                "type":"error","title":"JSON解析失败","message":"请求必须是有效JSON"
            }))).await;
            return;
        }
    };

    let message = body.get("message").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if message.is_empty() {
        let _ = socket.send(ws_msg(serde_json::json!({
            "type": "error", "title": "消息为空", "message": "未提供 message 字段"
        }))).await;
        return;
    }

    let session_id = body.get("session_id").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // 创建 channel（std::sync::mpsc 确保 cdylib 之间 TypeId 一致）
    let (tx, rx) = mpsc::channel::<String>();

    // 创建 Context
    let mut ctx = Context::new(if session_id.is_empty() { uuid_v7() } else { session_id });
    ctx.user_input = Some(message);
    inject_character_card(&mut ctx, "", &state.config.session.default_character);
    inject_api_config(&mut ctx, &state.config, "");
    // 强制启用流式
    if let Some(cfg) = ctx.get_custom_value("api_client.config") {
        let mut cfg = cfg.clone();
        cfg["stream"] = serde_json::Value::Bool(true);
        ctx.set_custom_value("api_client.config", cfg);
    }
    ctx.set_opaque("stream_parser.sender", tx);

    // 在独立任务中运行流水线
    let state_clone = state.clone();
    let handle = tokio::task::spawn_blocking(move || {
        let loader = state_clone.loader.read().unwrap();
        let stage_map = loader.build_stage_map(&state_clone.config.stages.order);
        run_pipeline(&mut ctx, &state_clone.config.stages, &stage_map)
    });

    // 用独立线程从 std::sync::mpsc 接收并转发到 WebSocket
    let (ws_tx, mut ws_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        while let Ok(token) = rx.recv() {
            if ws_tx.send(token).is_err() { break; }
        }
    });

    let mut accumulated = String::new();
    while let Some(token) = ws_rx.recv().await {
        accumulated.push_str(&token);
        if socket.send(ws_msg(serde_json::json!({
            "type": "partial", "content": accumulated
        }))).await.is_err() {
            break;
        }
    }

    // 等待流水线完成
    match handle.await {
        Ok(_stats) => {
            let _ = socket.send(ws_msg(serde_json::json!({
                "type": "done", "content": accumulated,
            }))).await;
        }
        Err(e) => {
            let _ = socket.send(ws_msg(serde_json::json!({
                "type": "error", "title": "流水线错误", "message": e.to_string(),
            }))).await;
        }
    }

    let _ = socket.send(Message::Close(None)).await;
}

/// POST /api/prompt/preview — 预览当前 system prompt。
async fn prompt_preview_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    let mut ctx = Context::new("preview");
    inject_character_card(&mut ctx, "", &state.config.session.default_character);
    run(&mut ctx, &state);

    let prompt = ctx.get_custom::<String>("prompt_builder.output").unwrap_or_default();
    let card = ctx.get_custom_value("character_card.data").cloned();
    let tokens = ctx.get_custom::<serde_json::Value>("token_counter.estimate").unwrap_or_default();

    (StatusCode::OK, Json(serde_json::json!({
        "prompt": prompt,
        "prompt_length": prompt.len(),
        "estimated_tokens": tokens.get("total").and_then(|v| v.as_u64()).unwrap_or(0),
        "character": card,
    })))
}

/// GET /api/history/export — 导出对话历史（Markdown 或 JSON）。
async fn history_export_handler(
    axum::extract::Query(query): axum::extract::Query<HashMap<String, String>>,
) -> (StatusCode, Json<Value>) {
    let session_id = query.get("session_id").map(|s| s.as_str()).unwrap_or("");
    let format = query.get("format").map(|s| s.as_str()).unwrap_or("json");

    if session_id.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(serde_json::json!({"error": "缺少 session_id"})));
    }

    let path = format!("./data/history/{}.json", session_id);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            if format == "markdown" {
                if let Ok(hist) = serde_json::from_str::<Value>(&content) {
                    let mut md = String::new();
                    md.push_str(&format!("# 对话历史 - {}\n\n", session_id));
                    if let Some(msgs) = hist.get("messages").and_then(|v| v.as_array()) {
                        for msg in msgs {
                            let role = msg.get("role").and_then(|v| v.as_str()).unwrap_or("?");
                            let content = msg.get("content").and_then(|v| v.as_str()).unwrap_or("");
                            md.push_str(&format!("**{}**: {}\n\n", role, content));
                        }
                    }
                    return (StatusCode::OK, Json(serde_json::json!({"markdown": md})));
                }
            }
            // 默认 JSON
            let val: Value = serde_json::from_str(&content).unwrap_or(Value::Null);
            (StatusCode::OK, Json(val))
        }
        Err(_) => (StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "历史记录不存在"}))),
    }
}

/// GET /
async fn root_handler(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let loader = match state.loader.read() {
        Ok(l) => l,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "RwLock 中毒"}))),
    };
    (StatusCode::OK, Json(serde_json::json!({
        "name": "ZS-AI",
        "version": "0.1.0",
        "plugin_count": loader.len(),
        "plugins": loader.names(),
        "stages": state.config.stages.order,
    })))
}

// ============================================================
// Main
// ============================================================

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with_target(false)
        .init();

    info!("ZS-AI 服务端启动中...");

    // 加载配置
    let config_path = std::env::args().nth(1).unwrap_or_else(|| "config.toml".to_string());
    let config = match AppConfig::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "主配置加载失败，尝试 config.example.toml");
            AppConfig::load("config.example.toml").unwrap_or_else(|e2| {
                error!(error = %e2, "配置加载失败");
                std::process::exit(1);
            })
        }
    };
    info!(stages = ?config.stages.order, "配置加载成功");

    // 加载插件
    let mut loader = DynamicLoader::new(&config.hot_reload.plugin_dir);
    let loaded = loader.load_all();
    info!(count = loaded.len(), ?loaded, "插件加载完成");

    // 初始化插件
    let plugin_configs: HashMap<String, Value> = config.plugins.iter().map(|(k, v)| {
        let s = serde_json::to_string(v).unwrap_or_default();
        (k.clone(), serde_json::from_str(&s).unwrap_or(Value::Null))
    }).collect();
    loader.init_all(&plugin_configs);

    // 热加载
    let loader_arc = Arc::new(RwLock::new(loader));
    let hot_reload = if config.hot_reload.enabled {
        let mut mgr = HotReloadManager::new(Arc::clone(&loader_arc), &config.hot_reload);
        match mgr.start() { Ok(()) => Some(Arc::new(RwLock::new(mgr))), Err(_) => None }
    } else { None };

    let state = AppState {
        loader: loader_arc,
        config: Arc::new(config),
        sessions: Arc::new(RwLock::new(SessionStore::default())),
        _hot_reload: hot_reload,
    };

    // 路由
    let app = Router::new()
        .route("/", get(root_handler))
        .route("/health", get(health_handler))
        .route("/api/chat", post(chat_handler))
        .route("/ws/chat", get(ws_chat_handler))
        .route("/api/session", post(session_create_handler))
        .route("/api/session/{id}", get(session_get_handler).delete(session_delete_handler))
        .route("/api/card", get(card_get_handler).put(card_put_handler))
        .route("/api/card/import", post(card_import_handler))
        .route("/api/config", get(config_get_handler).put(config_put_handler))
        .route("/api/history", get(history_get_handler).delete(history_delete_handler))
        .route("/api/history/export", get(history_export_handler))
        .route("/api/prompt/preview", post(prompt_preview_handler))
        .with_state(state);

    // 启动
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(9786);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            warn!(error = %e, port = port, "端口 {} 绑定失败，使用随机端口", port);
            tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
                .await
                .expect("无法绑定任何端口")
        }
    };

    let actual_port = listener.local_addr().unwrap().port();
    info!(port = actual_port, "服务已就绪");
    println!("PORT={}", actual_port);

    axum::serve(listener, app).await.unwrap();
}
