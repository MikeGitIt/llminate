use llminate::auth::session::{
    Session, SessionData, SessionManager, SessionStatus, SessionUpdate, SessionUser,
    SessionAggregatesManager, capture_session, capture_session_client, close_session,
    create_session_envelope, end_session_internal, make_session,
    send_session_update, serialize_session, start_session, update_session,
};
use serde_json;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[test]
fn test_full_session_lifecycle() {
    // Create a new session with initial data
    let session_data = SessionData {
        release: Some("1.0.0".to_string()),
        environment: Some("production".to_string()),
        user: Some(SessionUser {
            id: Some("user123".to_string()),
            email: Some("test@example.com".to_string()),
            username: Some("testuser".to_string()),
            ip_address: Some("192.168.1.1".to_string()),
        }),
        user_agent: Some("Mozilla/5.0".to_string()),
        did: None,
        ip_address: None,
    };

    let mut session = make_session(Some(session_data));

    // Verify initial state
    assert!(session.init);
    assert_eq!(session.status, SessionStatus::Ok);
    assert_eq!(session.errors, 0);
    assert_eq!(session.release, Some("1.0.0".to_string()));
    assert_eq!(session.environment, Some("production".to_string()));
    assert_eq!(session.did, Some("user123".to_string())); // Should use user.id
    assert_eq!(session.ip_address, Some("192.168.1.1".to_string()));

    // Update session with errors
    update_session(&mut session, SessionUpdate {
        errors: Some(3),
        ..Default::default()
    });
    assert_eq!(session.errors, 3);

    // Close session normally
    close_session(&mut session, None);
    assert_eq!(session.status, SessionStatus::Exited);

    // Serialize for transport
    let envelope = serialize_session(&session);
    assert_eq!(envelope.sid, session.sid);
    assert_eq!(envelope.status, SessionStatus::Exited);
    assert_eq!(envelope.errors, 3);
}

#[test]
fn test_session_manager_concurrent_access() {
    let manager = Arc::new(SessionManager::new());
    let manager1 = manager.clone();
    let manager2 = manager.clone();

    // Thread 1: Set session
    let handle1 = thread::spawn(move || {
        let session = make_session(Some(SessionData {
            release: Some("1.0.0".to_string()),
            ..Default::default()
        }));
        manager1.set_session(Some(session)).unwrap();
    });

    // Thread 2: Try to read session multiple times
    let handle2 = thread::spawn(move || {
        for _ in 0..10 {
            let _ = manager2.get_session();
            thread::sleep(Duration::from_millis(10));
        }
    });

    handle1.join().unwrap();
    handle2.join().unwrap();

    // Final check
    let session = manager.get_session().unwrap();
    assert!(session.is_some());
    if let Some(s) = session {
        assert_eq!(s.release, Some("1.0.0".to_string()));
    }
}

#[test]
fn test_session_aggregates() {
    let manager = SessionAggregatesManager::new(Duration::from_secs(60));

    // Create sessions with different statuses
    let mut session_base = make_session(Some(SessionData {
        release: Some("2.0.0".to_string()),
        environment: Some("staging".to_string()),
        ..Default::default()
    }));

    // Add exited session
    session_base.status = SessionStatus::Exited;
    manager.add(&session_base).unwrap();

    // Add crashed session
    session_base.status = SessionStatus::Crashed;
    manager.add(&session_base).unwrap();
    manager.add(&session_base).unwrap(); // Add another crashed

    // Add abnormal session
    session_base.status = SessionStatus::Abnormal;
    manager.add(&session_base).unwrap();

    // Flush and check aggregates
    let aggregates = manager.flush().unwrap();
    assert_eq!(aggregates.len(), 1);

    let key = "2.0.0:staging";
    let aggregate = aggregates.get(key).unwrap();
    assert_eq!(aggregate.exited, 1);
    assert_eq!(aggregate.crashed, 2);
    assert_eq!(aggregate.errored, 1);

    // After flush, should be empty
    let empty_aggregates = manager.flush().unwrap();
    assert!(empty_aggregates.is_empty());
}

#[test]
fn test_session_validation() {
    // Session without release should fail client capture
    let session_no_release = make_session(None);
    let result = capture_session_client(&session_no_release);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("missing or non-string release"));

    // Session with release should pass validation
    let session_with_release = make_session(Some(SessionData {
        release: Some("1.0.0".to_string()),
        ..Default::default()
    }));
    let result = capture_session_client(&session_with_release);
    assert!(result.is_ok());
}

#[test]
fn test_session_envelope_creation() {
    let session = make_session(Some(SessionData {
        release: Some("3.0.0".to_string()),
        environment: Some("production".to_string()),
        user: Some(SessionUser {
            id: Some("user456".to_string()),
            email: None,
            username: None,
            ip_address: Some("10.0.0.1".to_string()),
        }),
        user_agent: Some("TestAgent/2.0".to_string()),
        ..Default::default()
    }));

    let envelope = create_session_envelope(&session).unwrap();

    // Verify envelope structure
    assert_eq!(envelope.sdk.name, "sentry.rust");
    assert!(envelope.sent_at.len() > 0);
    assert_eq!(envelope.session.sid, session.sid);
    assert_eq!(envelope.session.attrs.release, Some("3.0.0".to_string()));
    assert_eq!(envelope.session.attrs.environment, Some("production".to_string()));

    // Verify it serializes to valid JSON
    let json = serde_json::to_string(&envelope).unwrap();
    assert!(json.contains("\"release\":\"3.0.0\""));
    assert!(json.contains("\"environment\":\"production\""));
    assert!(json.contains("sentry.rust"));
}

#[test]
fn test_session_duration_calculation() {
    let mut session = make_session(None);

    // Set started time in the past
    session.started = session.timestamp - 30.0; // 30 seconds ago

    // Update without explicit duration
    let timestamp = session.timestamp + 30.0; // 30 seconds later
    update_session(&mut session, SessionUpdate {
        timestamp: Some(timestamp),
        ..Default::default()
    });

    // Duration should be calculated
    assert!(session.duration.is_some());
    if let Some(duration) = session.duration {
        assert!(duration >= 30.0 && duration <= 60.0); // Capped at 60
    }

    // Test with ignore_duration flag
    session.ignore_duration = true;
    let timestamp = session.timestamp + 10.0;
    update_session(&mut session, SessionUpdate {
        timestamp: Some(timestamp),
        ..Default::default()
    });
    assert!(session.duration.is_none());
}

#[test]
fn test_session_user_fallback() {
    // Test did fallback priority: id > email > username
    let session_data = SessionData {
        user: Some(SessionUser {
            id: None,
            email: Some("user@example.com".to_string()),
            username: Some("username".to_string()),
            ip_address: None,
        }),
        ..Default::default()
    };

    let session = make_session(Some(session_data));
    assert_eq!(session.did, Some("user@example.com".to_string()));

    // Test with only username
    let session_data2 = SessionData {
        user: Some(SessionUser {
            id: None,
            email: None,
            username: Some("onlyusername".to_string()),
            ip_address: None,
        }),
        ..Default::default()
    };

    let session2 = make_session(Some(session_data2));
    assert_eq!(session2.did, Some("onlyusername".to_string()));
}

#[test]
fn test_abnormal_session_mechanism() {
    let mut session = make_session(None);

    update_session(&mut session, SessionUpdate {
        abnormal_mechanism: Some("anr_foreground".to_string()),
        status: Some(SessionStatus::Abnormal),
        ..Default::default()
    });

    assert_eq!(session.abnormal_mechanism, Some("anr_foreground".to_string()));
    assert_eq!(session.status, SessionStatus::Abnormal);
}

#[test]
fn test_session_sid_update() {
    let mut session = make_session(None);
    let original_sid = session.sid.clone();

    // Update with valid 32-char SID
    let new_sid = "12345678901234567890123456789012".to_string();
    update_session(&mut session, SessionUpdate {
        sid: Some(new_sid.clone()),
        ..Default::default()
    });
    assert_eq!(session.sid, new_sid);

    // Update with invalid SID (not 32 chars) - should generate new UUID
    update_session(&mut session, SessionUpdate {
        sid: Some("short".to_string()),
        ..Default::default()
    });
    assert_ne!(session.sid, "short");
    assert_eq!(session.sid.len(), 36); // UUID format
    assert_ne!(session.sid, original_sid);
}

#[test]
fn test_capture_session_api() {
    // Test end_session flag
    let result = capture_session(true);
    assert!(result.is_ok());

    // Test normal session update
    let result = capture_session(false);
    assert!(result.is_ok());
}

#[test]
fn test_start_session_closes_existing() {
    // Create initial session
    let initial_data = SessionData {
        release: Some("1.0.0".to_string()),
        environment: Some("test".to_string()),
        ..Default::default()
    };

    let session1 = start_session(Some(initial_data.clone())).unwrap();
    assert_eq!(session1.status, SessionStatus::Ok);

    // Start new session - should close previous
    let session2 = start_session(Some(initial_data)).unwrap();
    assert_eq!(session2.status, SessionStatus::Ok);
    assert_ne!(session1.sid, session2.sid); // Different sessions
}

#[test]
fn test_session_json_serialization() {
    let session = make_session(Some(SessionData {
        release: Some("1.2.3".to_string()),
        environment: Some("prod".to_string()),
        user: Some(SessionUser {
            id: Some("123".to_string()),
            email: Some("test@test.com".to_string()),
            username: None,
            ip_address: Some("1.2.3.4".to_string()),
        }),
        user_agent: Some("Chrome/100".to_string()),
        ..Default::default()
    }));

    // Serialize to JSON
    let json = serde_json::to_string(&session).unwrap();

    // Deserialize back
    let deserialized: Session = serde_json::from_str(&json).unwrap();

    // Verify round-trip
    assert_eq!(deserialized.sid, session.sid);
    assert_eq!(deserialized.release, session.release);
    assert_eq!(deserialized.environment, session.environment);
    assert_eq!(deserialized.did, session.did);
    assert_eq!(deserialized.ip_address, session.ip_address);
    assert_eq!(deserialized.user_agent, session.user_agent);
}

#[test]
fn test_session_listener_notifications() {
    let manager = SessionManager::new();
    let notified = Arc::new(std::sync::Mutex::new(false));
    let notified_clone = notified.clone();

    // Add listener
    manager.add_listener(move || {
        *notified_clone.lock().unwrap() = true;
    }).unwrap();

    // Setting session should trigger listener
    let session = make_session(None);
    manager.set_session(Some(session)).unwrap();

    // Give listener time to execute
    thread::sleep(Duration::from_millis(10));

    assert!(*notified.lock().unwrap());
}

#[test]
fn test_multiple_session_aggregates() {
    let manager = SessionAggregatesManager::new(Duration::from_secs(60));

    // Add sessions for different releases/environments
    let session1 = make_session(Some(SessionData {
        release: Some("1.0.0".to_string()),
        environment: Some("prod".to_string()),
        ..Default::default()
    }));

    let mut session2 = make_session(Some(SessionData {
        release: Some("1.0.0".to_string()),
        environment: Some("staging".to_string()),
        ..Default::default()
    }));
    session2.status = SessionStatus::Crashed;

    let mut session3 = make_session(Some(SessionData {
        release: Some("2.0.0".to_string()),
        environment: Some("prod".to_string()),
        ..Default::default()
    }));
    session3.status = SessionStatus::Exited;

    manager.add(&session1).unwrap();
    manager.add(&session2).unwrap();
    manager.add(&session3).unwrap();

    let aggregates = manager.flush().unwrap();

    // Should have 3 different aggregate keys
    assert_eq!(aggregates.len(), 3);
    assert!(aggregates.contains_key("1.0.0:prod"));
    assert!(aggregates.contains_key("1.0.0:staging"));
    assert!(aggregates.contains_key("2.0.0:prod"));

    // Check specific aggregate
    let staging_aggregate = aggregates.get("1.0.0:staging").unwrap();
    assert_eq!(staging_aggregate.crashed, 1);
}

#[cfg(test)]
mod sentry_integration_tests {
    use super::*;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_sentry() {
        INIT.call_once(|| {
            // Initialize Sentry for testing
            // In production, you'd use a real DSN
            let _guard = sentry::init((
                "https://public@sentry.io/1",
                sentry::ClientOptions {
                    release: Some("test-release".into()),
                    environment: Some("test".into()),
                    ..Default::default()
                },
            ));
        });
    }

    #[test]
    fn test_sentry_session_start() {
        init_sentry();

        // Start a session through our API
        let session = start_session(Some(SessionData {
            release: Some("test-release".to_string()),
            environment: Some("test".to_string()),
            ..Default::default()
        }));

        assert!(session.is_ok());
        let session = session.unwrap();
        assert_eq!(session.release, Some("test-release".to_string()));
    }

    #[test]
    fn test_sentry_session_end() {
        init_sentry();

        // End session through our API
        let result = end_session_internal();
        assert!(result.is_ok());
    }

    #[test]
    fn test_sentry_session_update() {
        init_sentry();

        // Send session update
        let result = send_session_update();
        assert!(result.is_ok());
    }
}