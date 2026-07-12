//! Integration tests for PLATO Core room lifecycle and wire protocol.

use plato_core::room::Room;
use plato_core::wire::*;
use plato_core::{AlarmOp, MAX_HISTORY, PROTOCOL_VERSION};

// ─── Room Lifecycle Tests ────────────────────────────────────────

#[test]
fn test_full_room_lifecycle() {
    let mut room = Room::new("engine_room", 0.2);

    // Setup
    room.add_sensor("coolant_temp_c", 85.0);
    room.add_sensor("oil_pressure_psi", 45.0);
    room.add_sensor("rpm", 1800.0);

    room.add_actuator("radiator_fan", 0.0);
    room.add_actuator("oil_pump", 1.0);

    room.add_alarm("overheat", "coolant_temp_c", AlarmOp::Gt, 95.0, 30.0);
    room.add_alarm("low_oil", "oil_pressure_psi", AlarmOp::Lt, 20.0, 60.0);

    // Initial state
    assert!(room.history.is_empty());
    assert_eq!(room.seq, 0);

    // Tick 1: normal operation
    let (snap, alarms) = room.tick_and_check();
    assert_eq!(snap.seq, 1);
    assert_eq!(snap.sensors.len(), 3);
    assert_eq!(snap.actuators.len(), 2);
    assert!(alarms.is_empty());

    // Simulate overheating
    room.set_sensor("coolant_temp_c", 98.0);

    // Tick 2: alarm should fire
    let (snap, alarms) = room.tick_and_check();
    assert_eq!(snap.seq, 2);
    assert_eq!(alarms.len(), 1);
    assert_eq!(alarms[0].0, "overheat");

    // Tick 3: cooldown active — alarm should NOT fire again
    let (_, alarms) = room.tick_and_check();
    assert!(alarms.is_empty());

    // Activate radiator fan to cool down
    assert!(room.set_actuator("radiator_fan", 1.0));
    room.set_sensor("coolant_temp_c", 80.0);

    // Tick 4: temperature back to normal
    let (snap, _) = room.tick_and_check();
    assert_eq!(snap.sensor("coolant_temp_c"), Some(80.0));
    assert_eq!(snap.actuator("radiator_fan"), Some(1.0));
}

#[test]
fn test_history_retention() {
    let mut room = Room::new("test", 10.0);
    room.add_sensor("x", 0.0);

    // Fill beyond MAX_HISTORY
    for i in 0..(MAX_HISTORY + 50) {
        room.set_sensor("x", i as f64);
        room.tick();
    }

    assert_eq!(room.history.len(), MAX_HISTORY);
    // Oldest entry should have been removed
    let oldest = &room.history[0];
    assert!(oldest.seq > 50);
}

#[test]
fn test_multiple_alarms_same_sensor() {
    let mut room = Room::new("test", 1.0);
    room.add_sensor("temp", 50.0);

    room.add_alarm("too_cold", "temp", AlarmOp::Lt, 40.0, 10.0);
    room.add_alarm("too_hot", "temp", AlarmOp::Gt, 80.0, 10.0);
    room.add_alarm("just_right_high", "temp", AlarmOp::Ge, 50.0, 10.0);

    let (_snap, alarms) = room.tick_and_check();
    // "just_right_high" should fire (50 >= 50)
    assert_eq!(alarms.len(), 1);
    assert_eq!(alarms[0].0, "just_right_high");
}

#[test]
fn test_alarm_with_missing_sensor() {
    let mut room = Room::new("test", 1.0);
    room.add_alarm("phantom", "nonexistent", AlarmOp::Gt, 50.0, 10.0);

    let snap = room.tick();
    let triggered = room.check_alarms(&snap);
    assert!(triggered.is_empty()); // No crash, just no trigger
}

#[test]
fn test_all_alarm_operators() {
    let ops = [
        (AlarmOp::Lt, 10.0, 5.0, true),   // 5 < 10
        (AlarmOp::Lt, 10.0, 15.0, false),
        (AlarmOp::Gt, 10.0, 15.0, true),  // 15 > 10
        (AlarmOp::Gt, 10.0, 5.0, false),
        (AlarmOp::Le, 10.0, 10.0, true),  // 10 <= 10
        (AlarmOp::Ge, 10.0, 10.0, true),  // 10 >= 10
        (AlarmOp::Eq, 10.0, 10.0, true),  // 10 == 10
        (AlarmOp::Ne, 10.0, 11.0, true),  // 10 != 11
        (AlarmOp::Ne, 10.0, 10.0, false),
    ];

    for (op, threshold, sensor, expected) in ops {
        assert!(
            op.evaluate(sensor, threshold) == expected,
            "Failed: {:?} evaluate({}, {}) should be {}",
            op,
            sensor,
            threshold,
            expected
        );
    }
}

// ─── Wire Protocol Tests ────────────────────────────────────────

#[test]
fn test_protocol_version() {
    assert_eq!(PROTOCOL_VERSION, "0.1");
}

#[test]
fn test_default_port() {
    assert_eq!(plato_core::DEFAULT_PORT, 1234);
}

#[test]
fn test_welcome_message_format() {
    let mut room = Room::new("engine_room", 0.2);
    room.add_sensor("coolant_temp_c", 90.0);
    room.add_sensor("rpm", 1800.0);

    let resp = format_welcome(&room);
    let json = resp.to_json_line();

    // Verify all required fields
    assert!(json.contains(r#""type":"welcome""#));
    assert!(json.contains(r#""room_id":"engine_room""#));
    assert!(json.contains(r#""tick_hz":0.2"#));
    assert!(json.contains(r#""format":"json""#));
    assert!(json.contains("coolant_temp_c"));
    assert!(json.contains("rpm"));

    // Verify it's valid JSON
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "welcome");
    assert!(v["sensors"].is_array());
}

#[test]
fn test_tick_response_matches_spec() {
    let snap = plato_core::TickSnapshot {
        timestamp: 1749234437.0,
        seq: 42,
        sensors: vec![
            ("coolant_temp_c".into(), 96.3),
            ("bilge_cm".into(), 7.0),
            ("rpm".into(), 1790.0),
        ],
        actuators: vec![],
    };

    let resp = format_tick(&snap);
    let json = resp.to_json_line();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(v["type"], "tick");
    assert_eq!(v["seq"], 42);
    assert_eq!(v["t"], 1749234437.0);
    assert_eq!(v["data"]["coolant_temp_c"], 96.3);
    assert_eq!(v["data"]["bilge_cm"], 7.0);
    assert_eq!(v["data"]["rpm"], 1790.0);
}

#[test]
fn test_history_response_matches_spec() {
    let snapshots = vec![
        plato_core::TickSnapshot {
            timestamp: 1749234400.0,
            seq: 30,
            sensors: vec![("temp".into(), 90.0)],
            actuators: vec![],
        },
        plato_core::TickSnapshot {
            timestamp: 1749234405.0,
            seq: 31,
            sensors: vec![("temp".into(), 92.0)],
            actuators: vec![],
        },
    ];

    let resp = format_history(&snapshots);
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "history");
    assert_eq!(v["count"], 2);
    assert_eq!(v["ticks"][0]["seq"], 30);
    assert_eq!(v["ticks"][1]["seq"], 31);
    assert_eq!(v["ticks"][0]["data"]["temp"], 90.0);
}

#[test]
fn test_actuator_ack_format() {
    let resp = format_ack("actuator", "bilge_pump", 1.0);
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "ack");
    assert_eq!(v["command"], "actuator");
    assert_eq!(v["name"], "bilge_pump");
    assert_eq!(v["value"], 1.0);
}

#[test]
fn test_alarm_set_ack_format() {
    let resp = format_ack_alarm("low_rpm");
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "ack");
    assert_eq!(v["command"], "alarm_set");
    assert_eq!(v["id"], "low_rpm");
}

#[test]
fn test_alarm_notification_format() {
    let snap = plato_core::TickSnapshot {
        timestamp: 1749234437.0,
        seq: 42,
        sensors: vec![
            ("coolant_temp_c".into(), 96.3),
            ("bilge_cm".into(), 7.0),
            ("rpm".into(), 1790.0),
        ],
        actuators: vec![],
    };

    let resp = format_alarm_notification("overheat", 1749234437.0, &snap);
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "alarm");
    assert_eq!(v["id"], "overheat");
    assert_eq!(v["triggered_at"], 1749234437.0);
    assert!(v["data"].is_object());
}

#[test]
fn test_alarm_list_response_format() {
    let mut room = Room::new("engine", 0.2);
    room.add_alarm("overheat", "coolant_temp_c", AlarmOp::Gt, 95.0, 30.0);
    room.add_alarm("bilge_high", "bilge_cm", AlarmOp::Gt, 10.0, 60.0);

    let resp = format_alarm_list(&room);
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "alarm_list");
    assert!(v["alarms"].is_array());
    assert_eq!(v["alarms"].as_array().unwrap().len(), 2);
    // Alarms are in insertion order
    let ids: Vec<&str> = v["alarms"].as_array().unwrap()
        .iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&"bilge_high"));
    assert!(ids.contains(&"overheat"));
}

#[test]
fn test_subscribed_response_format() {
    let resp = format_subscribed(0.2);
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "subscribed");
    assert_eq!(v["tick_hz"], 0.2);
}

#[test]
fn test_bye_response_format() {
    let resp = format_bye();
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();
    assert_eq!(v["type"], "bye");
}

#[test]
fn test_error_response_format() {
    let resp = format_error("actuator 'buzzer' not found");
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "error");
    assert_eq!(v["message"], "actuator 'buzzer' not found");
}

#[test]
fn test_help_response_format() {
    let resp = format_help();
    let v: serde_json::Value = serde_json::from_str(&resp.to_json_line()).unwrap();

    assert_eq!(v["type"], "help");
    let commands = v["commands"].as_array().unwrap();
    assert!(commands.len() >= 9); // all commands listed
    assert!(commands.iter().any(|c| c == "tick"));
    assert!(commands.iter().any(|c| c == "quit"));
}

// ─── Edge Cases ──────────────────────────────────────────────────

#[test]
fn test_empty_room_tick() {
    let mut room = Room::new("empty", 1.0);
    let snap = room.tick();

    assert_eq!(snap.seq, 1);
    assert!(snap.sensors.is_empty());
    assert!(snap.actuators.is_empty());
}

#[test]
fn test_history_with_no_ticks() {
    let room = Room::new("test", 1.0);
    let ticks = room.last_n_ticks(10);
    assert!(ticks.is_empty());
    assert!(room.latest_tick().is_none());
}

#[test]
fn test_clear_history() {
    let mut room = Room::new("test", 1.0);
    room.add_sensor("x", 1.0);
    room.tick();
    room.tick();

    assert_eq!(room.history.len(), 2);
    room.clear_history();
    assert!(room.history.is_empty());
    assert_eq!(room.seq, 2); // seq counter not reset
}

#[test]
fn test_alarm_cooldown_recovery() {
    let mut room = Room::new("test", 1.0);
    room.add_sensor("temp", 100.0);
    // Very short cooldown for testing
    room.add_alarm("hot", "temp", AlarmOp::Gt, 50.0, 0.0);

    // Should fire every tick with 0 cooldown
    let snap = room.tick();
    let t1 = room.check_alarms(&snap);
    assert_eq!(t1.len(), 1);

    let snap = room.tick();
    let t2 = room.check_alarms(&snap);
    assert_eq!(t2.len(), 1); // fires again immediately with 0 cooldown
}

#[test]
fn test_parse_command_edge_cases() {
    // Whitespace handling
    assert!(parse_command("  ").is_err());
    assert_eq!(parse_command(" tick "), Ok(Command::Tick));

    // Case sensitivity
    assert!(parse_command("TICK").is_err());
    assert!(parse_command("Tick").is_err());

    // Boolean values for actuators
    let cmd = parse_command("actuator led true").unwrap();
    if let Command::Actuator { value, .. } = cmd {
        assert!((value - 1.0).abs() < f64::EPSILON);
    }

    let cmd = parse_command("actuator led false").unwrap();
    if let Command::Actuator { value, .. } = cmd {
        assert!((value - 0.0).abs() < f64::EPSILON);
    }
}

#[test]
fn test_full_session_simulation() {
    // Simulate a complete session flow
    let mut room = Room::new("factory_floor", 1.0);
    room.add_sensor("temperature", 20.0);
    room.add_sensor("humidity", 50.0);
    room.add_actuator("heater", 0.0);
    room.add_alarm("temp_high", "temperature", AlarmOp::Gt, 30.0, 10.0);

    // 1. Welcome
    let welcome = format_welcome(&room);
    let wv: serde_json::Value = serde_json::from_str(&welcome.to_json_line()).unwrap();
    assert_eq!(wv["room_id"], "factory_floor");

    // 2. Subscribe
    let sub = format_subscribed(room.tick_hz);
    assert!(sub.to_json_line().contains("subscribed"));

    // 3. Normal tick
    let (snap, alarms) = room.tick_and_check();
    assert!(alarms.is_empty());

    let tick_resp = format_tick(&snap);
    let tv: serde_json::Value = serde_json::from_str(&tick_resp.to_json_line()).unwrap();
    assert_eq!(tv["data"]["temperature"], 20.0);

    // 4. Turn on heater
    assert!(room.set_actuator("heater", 1.0));
    room.set_sensor("temperature", 35.0); // Simulate heating up

    // 5. Alarm tick
    let (snap, alarms) = room.tick_and_check();
    assert_eq!(alarms.len(), 1);

    let alarm_resp = format_alarm_notification("temp_high", snap.timestamp, &snap);
    let av: serde_json::Value = serde_json::from_str(&alarm_resp.to_json_line()).unwrap();
    assert_eq!(av["id"], "temp_high");
    assert_eq!(av["data"]["temperature"], 35.0);

    // 6. History
    let hist_resp = format_history(&room.history);
    let hv: serde_json::Value = serde_json::from_str(&hist_resp.to_json_line()).unwrap();
    assert_eq!(hv["count"], 2);

    // 7. Quit
    let bye = format_bye();
    assert!(bye.to_json_line().contains(r#""type":"bye""#));
}
