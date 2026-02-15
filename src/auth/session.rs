use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sentry::{protocol as sentry_protocol, Hub as SentryHub, Scope as SentryScope};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// Session status matching JavaScript implementation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Ok,
    Exited,
    Crashed,
    Abnormal,
}

/// Session object matching JavaScript makeSession
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Session ID (UUID v4)
    pub sid: String,

    /// Whether this is the initial session
    pub init: bool,

    /// Current timestamp (Unix timestamp in seconds)
    pub timestamp: f64,

    /// Session start timestamp
    pub started: f64,

    /// Session duration in seconds
    pub duration: Option<f64>,

    /// Session status
    pub status: SessionStatus,

    /// Error count
    pub errors: u32,

    /// Whether to ignore duration calculation
    #[serde(rename = "ignoreDuration")]
    pub ignore_duration: bool,

    /// Release version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,

    /// Environment
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,

    /// User ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,

    /// IP Address
    #[serde(rename = "ipAddress", skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,

    /// User Agent
    #[serde(rename = "userAgent", skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    /// Abnormal mechanism
    #[serde(rename = "abnormal_mechanism", skip_serializing_if = "Option::is_none")]
    pub abnormal_mechanism: Option<String>,
}

/// Session creation data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<SessionUser>,

    #[serde(rename = "userAgent", skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,

    #[serde(rename = "ipAddress", skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
}

/// User information for sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUser {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,

    #[serde(rename = "ip_address", skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
}

/// Session update data
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<SessionUser>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub abnormal_mechanism: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<u32>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub init: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub did: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<SessionStatus>,
}

impl Session {
    /// Create a new session matching JavaScript makeSession
    pub fn new(initial_data: Option<SessionData>) -> Self {
        let timestamp = Utc::now().timestamp() as f64;
        let mut session = Self {
            sid: generate_uuid(),
            init: true,
            timestamp,
            started: timestamp,
            duration: None,
            status: SessionStatus::Ok,
            errors: 0,
            ignore_duration: false,
            release: None,
            environment: None,
            did: None,
            ip_address: None,
            user_agent: None,
            abnormal_mechanism: None,
        };

        if let Some(data) = initial_data {
            session.apply_initial_data(data);
        }

        session
    }

    /// Apply initial session data
    fn apply_initial_data(&mut self, data: SessionData) {
        self.release = data.release;
        self.environment = data.environment.or_else(|| Some("production".to_string()));
        self.user_agent = data.user_agent;
        self.did = data.did.or_else(|| {
            data.user.as_ref().and_then(|u| {
                u.id.clone()
                    .or_else(|| u.email.clone())
                    .or_else(|| u.username.clone())
            })
        });
        self.ip_address = data.ip_address.or_else(|| {
            data.user.as_ref().and_then(|u| u.ip_address.clone())
        });
    }

    /// Update session matching JavaScript updateSession
    pub fn update(&mut self, updates: SessionUpdate) {
        // Update user information
        if let Some(user) = updates.user {
            if self.ip_address.is_none() {
                if let Some(ip) = user.ip_address {
                    self.ip_address = Some(ip);
                }
            }
            if self.did.is_none() && updates.did.is_none() {
                self.did = user.id
                    .or(user.email)
                    .or(user.username);
            }
        }

        // Update timestamp
        self.timestamp = updates.timestamp.unwrap_or_else(|| Utc::now().timestamp() as f64);

        // Update various session properties
        if let Some(mechanism) = updates.abnormal_mechanism {
            self.abnormal_mechanism = Some(mechanism);
        }

        if let Some(errors) = updates.errors {
            self.errors = errors;
        }

        if let Some(sid) = updates.sid {
            if sid.len() == 32 {
                self.sid = sid;
            } else {
                self.sid = generate_uuid();
            }
        }

        if let Some(init) = updates.init {
            self.init = init;
        }

        if self.did.is_none() {
            if let Some(did) = updates.did {
                self.did = Some(did);
            }
        }

        if let Some(started) = updates.started {
            self.started = started;
        }

        // Calculate duration
        if self.ignore_duration {
            self.duration = None;
        } else if let Some(duration) = updates.duration {
            self.duration = Some(duration);
        } else {
            let session_length = 60.0; // Default session length
            let duration = (self.timestamp - self.started).round();
            self.duration = Some(duration.max(0.0).min(session_length));
        }

        if let Some(status) = updates.status {
            self.status = status;
        }
    }

    /// Close session matching JavaScript closeSession
    pub fn close(&mut self, status: Option<SessionStatus>) {
        let updates = SessionUpdate {
            status: Some(status.unwrap_or_else(|| {
                if self.status == SessionStatus::Ok {
                    SessionStatus::Exited
                } else {
                    self.status.clone()
                }
            })),
            ..Default::default()
        };
        self.update(updates);
    }

    /// Serialize session to JSON matching JavaScript serializeSession
    pub fn serialize(&self) -> SessionEnvelope {
        SessionEnvelope {
            sid: self.sid.clone(),
            init: self.init,
            started: self.started,
            timestamp: self.timestamp,
            status: self.status.clone(),
            errors: self.errors,
            duration: self.duration,
            attrs: SessionAttributes {
                release: self.release.clone(),
                environment: self.environment.clone(),
                ip_address: self.ip_address.clone(),
                user_agent: self.user_agent.clone(),
            },
        }
    }
}

/// Session envelope for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEnvelope {
    pub sid: String,
    pub init: bool,
    pub started: f64,
    pub timestamp: f64,
    pub status: SessionStatus,
    pub errors: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    pub attrs: SessionAttributes,
}

/// Session attributes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAttributes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
}

/// Session Manager matching JavaScript SessionManager
pub struct SessionManager {
    session: Arc<RwLock<Option<Session>>>,
    listeners: Arc<RwLock<Vec<Box<dyn Fn() + Send + Sync>>>>,
}

impl SessionManager {
    /// Create new session manager
    pub fn new() -> Self {
        Self {
            session: Arc::new(RwLock::new(None)),
            listeners: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Set current session
    pub fn set_session(&self, session: Option<Session>) -> Result<()> {
        {
            let mut current = self.session.write()
                .map_err(|e| anyhow::anyhow!("Failed to acquire session lock: {}", e))?;
            *current = session;
        }

        self.notify_scope_listeners();
        Ok(())
    }

    /// Get current session
    pub fn get_session(&self) -> Result<Option<Session>> {
        let session = self.session.read()
            .map_err(|e| anyhow::anyhow!("Failed to acquire session lock: {}", e))?;
        Ok(session.clone())
    }

    /// Notify scope listeners
    fn notify_scope_listeners(&self) {
        if let Ok(listeners) = self.listeners.read() {
            for listener in listeners.iter() {
                listener();
            }
        }
    }

    /// Add a scope listener
    pub fn add_listener<F>(&self, listener: F) -> Result<()>
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut listeners = self.listeners.write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire listeners lock: {}", e))?;
        listeners.push(Box::new(listener));
        Ok(())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate UUID v4 matching JavaScript generateUUID
pub fn generate_uuid() -> String {
    Uuid::new_v4().to_string()
}

/// Create a new session matching JavaScript makeSession
pub fn make_session(initial_data: Option<SessionData>) -> Session {
    Session::new(initial_data)
}

/// Update session matching JavaScript updateSession
pub fn update_session(session: &mut Session, updates: SessionUpdate) {
    session.update(updates);
}

/// Close session matching JavaScript closeSession
pub fn close_session(session: &mut Session, status: Option<SessionStatus>) {
    session.close(status);
}

/// Serialize session matching JavaScript serializeSession
pub fn serialize_session(session: &Session) -> SessionEnvelope {
    session.serialize()
}

/// Capture session for reporting - main API entry point
/// Matches JavaScript captureSession (checker316)
pub fn capture_session(end_session: bool) -> Result<()> {
    if end_session {
        end_session_internal()?;
    } else {
        send_session_update()?;
    }
    Ok(())
}

/// End the current session
/// Matches JavaScript endSessionInternal (stringDecoder687)
pub fn end_session_internal() -> Result<()> {
    // In Rust, we use the Sentry SDK's Hub directly
    sentry::configure_scope(|scope| {
        // Close any existing session
        // Note: Sentry-rust handles session management internally
        // This is a simplified version - full implementation would need
        // custom session handling or direct Hub manipulation
        scope.clear();
    });

    send_session_update()?;
    Ok(())
}

/// Send session update to client
/// Matches JavaScript sendSessionUpdate (stringDecoder688)
pub fn send_session_update() -> Result<()> {
    // In Rust, we work with the Sentry SDK's session handling
    // The SDK automatically manages session updates
    sentry::end_session();
    Ok(())
}

/// Start a new session
/// Matches JavaScript startSession (stringDecoder686)
pub fn start_session(session_data: Option<SessionData>) -> Result<Session> {
    let mut session = Session::new(session_data);

    // End any existing session
    end_session_internal()?;

    // Start new session in Sentry
    sentry::start_session();

    Ok(session)
}

/// Client-side session capture
/// Matches JavaScript captureSession in client class
pub fn capture_session_client(session: &Session) -> Result<()> {
    // Validate session has release
    if session.release.is_none() {
        return Err(anyhow::anyhow!(
            "Discarded session because of missing or non-string release"
        ));
    }

    // Send session
    send_session(session)?;

    // Mark session as no longer initial
    let mut session_clone = session.clone();
    session_clone.update(SessionUpdate {
        init: Some(false),
        ..Default::default()
    });

    Ok(())
}

/// Send session to backend
/// Matches JavaScript sendSession
pub fn send_session(session: &Session) -> Result<()> {
    // Create session envelope
    let envelope = create_session_envelope(session)?;

    // In production, this would send to Sentry backend
    // For now, we'll just serialize it
    let _json = serde_json::to_string(&envelope)
        .context("Failed to serialize session envelope")?;

    // The actual sending would be handled by the Sentry transport layer
    // sentry::capture_event() or similar

    Ok(())
}

/// Create session envelope for transport
pub fn create_session_envelope(session: &Session) -> Result<SentrySessionEnvelope> {
    Ok(SentrySessionEnvelope {
        sent_at: Utc::now().to_rfc3339(),
        sdk: SdkInfo {
            name: "sentry.rust".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        },
        session: session.serialize(),
    })
}

/// Sentry session envelope structure
#[derive(Debug, Serialize, Deserialize)]
pub struct SentrySessionEnvelope {
    pub sent_at: String,
    pub sdk: SdkInfo,
    pub session: SessionEnvelope,
}

/// SDK information
#[derive(Debug, Serialize, Deserialize)]
pub struct SdkInfo {
    pub name: String,
    pub version: String,
}

/// Session aggregates manager
/// Manages batched session reporting
pub struct SessionAggregatesManager {
    aggregates: Arc<RwLock<HashMap<String, SessionAggregate>>>,
    flush_interval: std::time::Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionAggregate {
    pub started: DateTime<Utc>,
    pub exited: u32,
    pub errored: u32,
    pub crashed: u32,
}

impl SessionAggregatesManager {
    pub fn new(flush_interval: std::time::Duration) -> Self {
        Self {
            aggregates: Arc::new(RwLock::new(HashMap::new())),
            flush_interval,
        }
    }

    pub fn add(&self, session: &Session) -> Result<()> {
        let key = format!(
            "{}:{}",
            session.release.as_deref().unwrap_or("unknown"),
            session.environment.as_deref().unwrap_or("production")
        );

        let mut aggregates = self.aggregates.write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire aggregates lock: {}", e))?;

        let aggregate = aggregates.entry(key).or_insert_with(|| SessionAggregate {
            started: Utc::now(),
            exited: 0,
            errored: 0,
            crashed: 0,
        });

        match session.status {
            SessionStatus::Exited => aggregate.exited += 1,
            SessionStatus::Crashed => aggregate.crashed += 1,
            SessionStatus::Abnormal => aggregate.errored += 1,
            SessionStatus::Ok => {}
        }

        Ok(())
    }

    pub fn flush(&self) -> Result<HashMap<String, SessionAggregate>> {
        let mut aggregates = self.aggregates.write()
            .map_err(|e| anyhow::anyhow!("Failed to acquire aggregates lock: {}", e))?;

        let current = aggregates.clone();
        aggregates.clear();

        Ok(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_uuid() {
        let uuid1 = generate_uuid();
        let uuid2 = generate_uuid();

        // Should be valid UUIDs
        assert_eq!(uuid1.len(), 36);
        assert_eq!(uuid2.len(), 36);

        // Should be unique
        assert_ne!(uuid1, uuid2);

        // Should have correct format
        assert!(uuid1.contains('-'));
        let parts: Vec<&str> = uuid1.split('-').collect();
        assert_eq!(parts.len(), 5);
    }

    #[test]
    fn test_make_session() {
        let session = make_session(None);

        assert!(session.init);
        assert_eq!(session.status, SessionStatus::Ok);
        assert_eq!(session.errors, 0);
        assert!(!session.ignore_duration);
        assert_eq!(session.sid.len(), 36);
    }

    #[test]
    fn test_make_session_with_data() {
        let data = SessionData {
            release: Some("1.0.0".to_string()),
            environment: Some("staging".to_string()),
            user: Some(SessionUser {
                id: Some("user123".to_string()),
                email: Some("test@example.com".to_string()),
                username: None,
                ip_address: Some("192.168.1.1".to_string()),
            }),
            user_agent: Some("TestAgent/1.0".to_string()),
            did: None,
            ip_address: None,
        };

        let session = make_session(Some(data));

        assert_eq!(session.release, Some("1.0.0".to_string()));
        assert_eq!(session.environment, Some("staging".to_string()));
        assert_eq!(session.did, Some("user123".to_string()));
        assert_eq!(session.ip_address, Some("192.168.1.1".to_string()));
        assert_eq!(session.user_agent, Some("TestAgent/1.0".to_string()));
    }

    #[test]
    fn test_update_session() {
        let mut session = make_session(None);

        let updates = SessionUpdate {
            errors: Some(5),
            status: Some(SessionStatus::Crashed),
            did: Some("user456".to_string()),
            ..Default::default()
        };

        update_session(&mut session, updates);

        assert_eq!(session.errors, 5);
        assert_eq!(session.status, SessionStatus::Crashed);
        assert_eq!(session.did, Some("user456".to_string()));
    }

    #[test]
    fn test_close_session() {
        let mut session = make_session(None);
        assert_eq!(session.status, SessionStatus::Ok);

        close_session(&mut session, None);
        assert_eq!(session.status, SessionStatus::Exited);

        let mut session2 = make_session(None);
        close_session(&mut session2, Some(SessionStatus::Crashed));
        assert_eq!(session2.status, SessionStatus::Crashed);
    }

    #[test]
    fn test_serialize_session() {
        let mut session = make_session(Some(SessionData {
            release: Some("2.0.0".to_string()),
            environment: Some("production".to_string()),
            ..Default::default()
        }));

        session.errors = 3;
        session.duration = Some(45.0);

        let envelope = serialize_session(&session);

        assert_eq!(envelope.sid, session.sid);
        assert_eq!(envelope.init, session.init);
        assert_eq!(envelope.errors, 3);
        assert_eq!(envelope.duration, Some(45.0));
        assert_eq!(envelope.attrs.release, Some("2.0.0".to_string()));
        assert_eq!(envelope.attrs.environment, Some("production".to_string()));
    }

    #[test]
    fn test_session_manager() {
        let manager = SessionManager::new();

        // Initially no session
        assert!(manager.get_session().unwrap().is_none());

        // Set a session
        let session = make_session(None);
        manager.set_session(Some(session.clone())).unwrap();

        // Get the session back
        let retrieved = manager.get_session().unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().sid, session.sid);

        // Clear session
        manager.set_session(None).unwrap();
        assert!(manager.get_session().unwrap().is_none());
    }

    #[test]
    fn test_session_aggregates_manager() {
        let manager = SessionAggregatesManager::new(std::time::Duration::from_secs(60));

        let mut session1 = make_session(Some(SessionData {
            release: Some("1.0.0".to_string()),
            environment: Some("production".to_string()),
            ..Default::default()
        }));
        session1.status = SessionStatus::Exited;

        let mut session2 = session1.clone();
        session2.status = SessionStatus::Crashed;

        manager.add(&session1).unwrap();
        manager.add(&session2).unwrap();

        let aggregates = manager.flush().unwrap();
        assert_eq!(aggregates.len(), 1);

        let key = "1.0.0:production";
        let aggregate = aggregates.get(key).unwrap();
        assert_eq!(aggregate.exited, 1);
        assert_eq!(aggregate.crashed, 1);
        assert_eq!(aggregate.errored, 0);

        // After flush, aggregates should be empty
        let aggregates2 = manager.flush().unwrap();
        assert_eq!(aggregates2.len(), 0);
    }

    #[test]
    fn test_capture_session_client_validation() {
        let session = make_session(None);

        // Should fail without release
        let result = capture_session_client(&session);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("missing or non-string release"));

        // Should succeed with release
        let mut session_with_release = session;
        session_with_release.release = Some("1.0.0".to_string());
        let result = capture_session_client(&session_with_release);
        // Will fail because we don't have actual transport, but validation passes
        assert!(result.is_ok());
    }
}