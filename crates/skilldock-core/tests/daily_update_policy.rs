use chrono::{DateTime, Local, Timelike, Utc};
use serde_json::json;
use skilldock_core::Engine;
use std::fs;

#[tokio::test]
async fn daily_policy_persists_validates_and_can_switch_back() {
    let temp = tempfile::tempdir().unwrap();
    let engine = Engine::new(Some(temp.path().join("config"))).unwrap();
    let library = temp.path().join("library");
    engine.configure(library.to_str().unwrap()).unwrap();
    let mut snapshot = engine.snapshot().unwrap();
    snapshot.sources.push(serde_json::from_value(json!({
        "id":"daily", "name":"Daily test", "kind":"git", "path":"", "url":"https://example.com/skills.git",
        "reference":"main", "version":"", "policy":{"mode":"off","intervalHours":12},
        "lastChecked":"", "nextCheck":"", "status":"healthy", "error":""
    })).unwrap());
    fs::write(
        library.join("state.json"),
        serde_json::to_vec(&snapshot).unwrap(),
    )
    .unwrap();
    engine.execute(json!({"action":"set_policy","sourceId":"daily","mode":"notify","intervalHours":12,"dailyTime":"09:15"})).await.unwrap();
    let saved = engine.snapshot().unwrap();
    assert_eq!(saved.sources[0].policy.daily_time.as_deref(), Some("09:15"));
    let next = DateTime::parse_from_rfc3339(&saved.sources[0].next_check).unwrap();
    assert!(next > Utc::now());
    assert_eq!(
        (
            next.with_timezone(&Local).hour(),
            next.with_timezone(&Local).minute()
        ),
        (9, 15)
    );
    for invalid in [json!("24:00"), json!("9:15"), json!(915)] {
        assert!(engine.execute(json!({"action":"set_policy","sourceId":"daily","mode":"notify","dailyTime":invalid})).await.is_err());
        assert_eq!(engine.snapshot().unwrap().revision, saved.revision);
    }
    engine
        .execute(json!({"action":"set_policy","sourceId":"daily","mode":"off","dailyTime":"09:15"}))
        .await
        .unwrap();
    assert!(engine.snapshot().unwrap().sources[0].next_check.is_empty());
    engine
        .execute(
            json!({"action":"set_policy","sourceId":"daily","mode":"notify","intervalHours":12}),
        )
        .await
        .unwrap();
    let restored = engine.snapshot().unwrap();
    assert!(restored.sources[0].policy.daily_time.is_none());
    let next = DateTime::parse_from_rfc3339(&restored.sources[0].next_check).unwrap();
    assert!((next.with_timezone(&Utc) - Utc::now()).num_seconds() >= 43190);
}
