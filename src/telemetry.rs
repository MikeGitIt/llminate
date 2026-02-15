use crate::error::Result;
use anyhow::Context;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use uuid::Uuid;

static TELEMETRY_CLIENT: Lazy<Arc<TelemetryClient>> = Lazy::new(|| {
    Arc::new(TelemetryClient::new())
});

static SESSION_ID: Lazy<String> = Lazy::new(|| Uuid::new_v4().to_string());

#[derive(Debug)]
pub struct TelemetryClient {
    sender: mpsc::UnboundedSender<TelemetryEvent>,
    user_id: Mutex<Option<String>>,
    session_data: Mutex<HashMap<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TelemetryEvent {
    event_name: String,
    properties: HashMap<String, Value>,
    timestamp: u64,
    session_id: String,
    user_id: Option<String>,
}

impl TelemetryClient {
    fn new() -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<TelemetryEvent>();
        
        // Spawn background task to handle telemetry
        tokio::spawn(async move {
            let mut batch = Vec::new();
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    Some(event) = receiver.recv() => {
                        batch.push(event);
                        
                        // Send batch if it gets too large
                        if batch.len() >= 100 {
                            send_batch(&batch).await;
                            batch.clear();
                        }
                    }
                    _ = interval.tick() => {
                        // Send batch periodically
                        if !batch.is_empty() {
                            send_batch(&batch).await;
                            batch.clear();
                        }
                    }
                }
            }
        });
        
        Self {
            sender,
            user_id: Mutex::new(None),
            session_data: Mutex::new(HashMap::new()),
        }
    }
}

async fn send_batch(events: &[TelemetryEvent]) {
    // Only send telemetry if explicitly enabled
    if std::env::var("LLMINATE_TELEMETRY_DISABLED").is_ok() {
        return;
    }
    
    // In production, this would send to a telemetry endpoint
    if cfg!(debug_assertions) {
        tracing::debug!("Would send telemetry batch: {} events", events.len());
    }
}

/// Initialize telemetry system
pub async fn init() {
    // Set up any global telemetry configuration
    let client = TELEMETRY_CLIENT.clone();
    
    // Try to load user ID from config
    if let Ok(config) = crate::config::get_merged_config() {
        if let Some(user_id) = config.extra.get("userId").and_then(|v| v.as_str()) {
            client.user_id.lock().replace(user_id.to_string());
        }
    }
    
    // Track session start
    track("session_start", json!({
        "version": crate::VERSION,
        "platform": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
    })).await;
}

/// Track an event
pub async fn track<T: Serialize>(event_name: impl Into<String>, properties: T) {
    let client = TELEMETRY_CLIENT.clone();
    
    let mut props = HashMap::new();
    if let Ok(value) = serde_json::to_value(properties) {
        if let Value::Object(map) = value {
            for (k, v) in map {
                props.insert(k, v);
            }
        }
    }
    
    // Add session data
    let session_data = client.session_data.lock();
    for (k, v) in session_data.iter() {
        props.insert(k.clone(), v.clone());
    }
    
    let event = TelemetryEvent {
        event_name: event_name.into(),
        properties: props,
        timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        session_id: SESSION_ID.clone(),
        user_id: client.user_id.lock().clone(),
    };
    
    // Send event (ignore if channel is closed)
    let _ = client.sender.send(event);
}

/// Set user ID for telemetry
pub fn set_user_id(user_id: Option<String>) {
    TELEMETRY_CLIENT.user_id.lock().clone_from(&user_id);
}

/// Add session-wide properties
pub fn set_session_property(key: impl Into<String>, value: impl Serialize) {
    if let Ok(value) = serde_json::to_value(value) {
        TELEMETRY_CLIENT.session_data.lock().insert(key.into(), value);
    }
}

/// Track timing
pub struct Timer {
    start: std::time::Instant,
    event_name: String,
    properties: HashMap<String, Value>,
}

impl Timer {
    pub fn new(event_name: impl Into<String>) -> Self {
        Self {
            start: std::time::Instant::now(),
            event_name: event_name.into(),
            properties: HashMap::new(),
        }
    }
    
    pub fn add_property(mut self, key: impl Into<String>, value: impl Serialize) -> Self {
        if let Ok(value) = serde_json::to_value(value) {
            self.properties.insert(key.into(), value);
        }
        self
    }
    
    pub async fn finish(mut self) {
        let duration_ms = self.start.elapsed().as_millis() as u64;
        self.properties.insert("duration_ms".to_string(), json!(duration_ms));
        track(&self.event_name, self.properties).await;
    }
}

/// Track API calls
pub async fn track_api_call(
    endpoint: &str,
    method: &str,
    status_code: u16,
    duration_ms: u64,
) {
    track("api_call", json!({
        "endpoint": endpoint,
        "method": method,
        "status_code": status_code,
        "duration_ms": duration_ms,
        "success": status_code >= 200 && status_code < 300,
    })).await;
}

/// Track tool usage
pub async fn track_tool_use(
    tool_name: &str,
    success: bool,
    duration_ms: Option<u64>,
) {
    let mut props = json!({
        "tool_name": tool_name,
        "success": success,
    });
    
    if let Some(duration) = duration_ms {
        props["duration_ms"] = json!(duration);
    }
    
    track("tool_use", props).await;
}

/// Track errors
pub async fn track_error(
    error_type: &str,
    message: &str,
    context: Option<HashMap<String, Value>>,
) {
    let mut props = json!({
        "error_type": error_type,
        "message": message,
    });
    
    if let Some(ctx) = context {
        props["context"] = json!(ctx);
    }
    
    track("error", props).await;
}

/// Track feature usage
pub async fn track_feature(
    feature_name: &str,
    action: &str,
    metadata: Option<HashMap<String, Value>>,
) {
    let mut props = json!({
        "feature_name": feature_name,
        "action": action,
    });
    
    if let Some(meta) = metadata {
        props["metadata"] = json!(meta);
    }
    
    track("feature_use", props).await;
}

/// Track session metrics
pub async fn track_session_end(
    total_messages: u32,
    total_duration_ms: u64,
    total_tokens_input: u64,
    total_tokens_output: u64,
    total_cost_usd: f64,
) {
    track("session_end", json!({
        "total_messages": total_messages,
        "total_duration_ms": total_duration_ms,
        "total_tokens_input": total_tokens_input,
        "total_tokens_output": total_tokens_output,
        "total_cost_usd": total_cost_usd,
        "average_response_time_ms": if total_messages > 0 {
            total_duration_ms / total_messages as u64
        } else {
            0
        },
    })).await;
}

/// Get current session ID
pub fn get_session_id() -> &'static str {
    &SESSION_ID
}