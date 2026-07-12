//! PLATO Wire Protocol v0.1 — command parsing and response formatting.
//!
//! This module implements the wire protocol defined in
//! [PLATO_WIRE_PROTOCOL.md](https://github.com/SuperInstance/AI-Writings/blob/main/PLATO_WIRE_PROTOCOL.md).
//!
//! ## Commands (Agent → Room)
//!
//! Commands are plain-text, single-line:
//! - `tick`
//! - `history [N]`
//! - `actuator <name> <value>`
//! - `alarm list`
//! - `alarm set <id> <condition> <cooldown>`
//! - `subscribe`
//! - `unsubscribe`
//! - `help`
//! - `quit`
//!
//! ## Responses (Room → Agent)
//!
//! Responses are single-line JSON objects with a `type` field.

use crate::room::Room;
use crate::{AlarmOp, TickSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

// ─── Command Parsing ─────────────────────────────────────────────

/// Parsed command from an agent.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `tick` — request current sensor data.
    Tick,
    /// `history [N]` — request last N ticks (default 10).
    History(usize),
    /// `actuator <name> <value>` — set an actuator.
    Actuator { name: String, value: f64 },
    /// `alarm list` — list all alarms.
    AlarmList,
    /// `alarm set <id> <condition> <cooldown>` — add an alarm rule.
    AlarmSet {
        id: String,
        sensor: String,
        op: AlarmOp,
        threshold: f64,
        cooldown_sec: f64,
    },
    /// `subscribe` — begin receiving streaming ticks.
    Subscribe,
    /// `unsubscribe` — stop streaming ticks.
    Unsubscribe,
    /// `help` — list available commands.
    Help,
    /// `quit` — disconnect.
    Quit,
}

/// Parse a command line from an agent.
///
/// Returns `Ok(Command)` on success, or `Err(String)` with a human-readable error.
///
/// # Examples
/// ```
/// use plato_core::wire::{parse_command, Command};
///
/// assert_eq!(parse_command("tick"), Ok(Command::Tick));
/// assert_eq!(parse_command("history 20"), Ok(Command::History(20)));
/// assert!(parse_command("garbage").is_err());
/// ```
pub fn parse_command(line: &str) -> Result<Command, String> {
    let line = line.trim();
    if line.is_empty() {
        return Err("empty command".into());
    }

    let parts: Vec<&str> = line.split_whitespace().collect();
    let cmd = parts[0];

    match cmd {
        "tick" => {
            if parts.len() != 1 {
                return Err("tick takes no arguments".into());
            }
            Ok(Command::Tick)
        }

        "history" => {
            let n = if parts.len() == 1 {
                10
            } else if parts.len() == 2 {
                parts[1]
                    .parse::<usize>()
                    .map_err(|_| format!("invalid history count: {}", parts[1]))?
            } else {
                return Err("usage: history [N]".into());
            };
            Ok(Command::History(n))
        }

        "actuator" => {
            if parts.len() != 3 {
                return Err("usage: actuator <name> <value>".into());
            }
            let name = parts[1].to_string();
            let value = parse_f64(parts[2])
                .ok_or_else(|| format!("invalid actuator value: {}", parts[2]))?;
            Ok(Command::Actuator { name, value })
        }

        "alarm" => {
            if parts.len() < 2 {
                return Err("usage: alarm <list|set ...>".into());
            }
            match parts[1] {
                "list" => {
                    if parts.len() != 2 {
                        return Err("alarm list takes no arguments".into());
                    }
                    Ok(Command::AlarmList)
                }
                "set" => parse_alarm_set(&parts[2..]),
                _ => Err(format!("unknown alarm subcommand: {}", parts[1])),
            }
        }

        "subscribe" => {
            if parts.len() != 1 {
                return Err("subscribe takes no arguments".into());
            }
            Ok(Command::Subscribe)
        }

        "unsubscribe" => {
            if parts.len() != 1 {
                return Err("unsubscribe takes no arguments".into());
            }
            Ok(Command::Unsubscribe)
        }

        "help" => {
            if parts.len() != 1 {
                return Err("help takes no arguments".into());
            }
            Ok(Command::Help)
        }

        "quit" => {
            if parts.len() != 1 {
                return Err("quit takes no arguments".into());
            }
            Ok(Command::Quit)
        }

        _ => Err(format!("unknown command: {}", cmd)),
    }
}

/// Parse `alarm set <id> <sensor> <op> <threshold> <cooldown>`
/// parts = ["<id>", "<sensor>", "<op>", "<threshold>", "<cooldown>"]
fn parse_alarm_set(parts: &[&str]) -> Result<Command, String> {
    if parts.len() != 5 {
        return Err(
            "usage: alarm set <id> <sensor> <op> <threshold> <cooldown>".into(),
        );
    }
    let id = parts[0].to_string();
    let sensor = parts[1].to_string();
    let op = AlarmOp::from_str(parts[2])
        .ok_or_else(|| format!("invalid comparison operator: {}", parts[2]))?;
    let threshold = parse_f64(parts[3])
        .ok_or_else(|| format!("invalid threshold: {}", parts[3]))?;
    let cooldown_sec = parse_f64(parts[4])
        .ok_or_else(|| format!("invalid cooldown: {}", parts[4]))?;

    Ok(Command::AlarmSet {
        id,
        sensor,
        op,
        threshold,
        cooldown_sec,
    })
}

/// Parse an f64 from a string, accepting integers, floats, and booleans.
fn parse_f64(s: &str) -> Option<f64> {
    // Handle boolean-like values
    if s.eq_ignore_ascii_case("true") || s == "1" {
        return Some(1.0);
    }
    if s.eq_ignore_ascii_case("false") || s == "0" {
        return Some(0.0);
    }
    s.parse::<f64>().ok()
}

// ─── Response Types ──────────────────────────────────────────────

/// A wire protocol response. Can be serialized to a JSON line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// `{"type":"tick", ...}` — current sensor data.
    Tick {
        t: f64,
        seq: u64,
        data: serde_json::Map<String, Value>,
    },
    /// `{"type":"history", ...}` — last N ticks.
    History {
        count: usize,
        ticks: Vec<Value>,
    },
    /// `{"type":"ack", ...}` — command acknowledgement.
    Ack {
        command: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        value: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<String>,
    },
    /// `{"type":"alarm", ...}` — spontaneous alarm notification.
    Alarm {
        id: String,
        triggered_at: f64,
        data: serde_json::Map<String, Value>,
    },
    /// `{"type":"alarm_list", ...}` — list of all alarms.
    AlarmList {
        alarms: Vec<AlarmEntry>,
    },
    /// `{"type":"subscribed", ...}` — subscription confirmed.
    Subscribed { tick_hz: f64 },
    /// `{"type":"unsubscribed"}` — unsubscription confirmed.
    Unsubscribed,
    /// `{"type":"welcome", ...}` — sent on connect.
    Welcome {
        room_id: String,
        tick_hz: f64,
        sensors: Vec<String>,
        #[serde(rename = "format")]
        format: String,
    },
    /// `{"type":"help", ...}` — list of commands.
    Help { commands: Vec<String> },
    /// `{"type":"bye"}` — disconnection confirmed.
    Bye,
    /// `{"type":"error", ...}` — error response.
    Error { message: String },
}

/// A single alarm entry in an `alarm_list` response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlarmEntry {
    pub id: String,
    pub condition: String,
    pub cooldown_sec: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_triggered: Option<f64>,
    pub state: String,
}

impl Response {
    /// Serialize this response to a single-line JSON string.
    pub fn to_json_line(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| {
            r#"{"type":"error","message":"internal serialization error"}"#.to_string()
        })
    }
}

// ─── Response Formatting Helpers ─────────────────────────────────

/// Format a welcome message for a room.
pub fn format_welcome(room: &Room) -> Response {
    Response::Welcome {
        room_id: room.name.clone(),
        tick_hz: room.tick_hz,
        sensors: room.sensor_names(),
        format: "json".to_string(),
    }
}

/// Format a tick response from a snapshot.
pub fn format_tick(snap: &TickSnapshot) -> Response {
    let mut data = serde_json::Map::new();
    for (name, value) in &snap.sensors {
        data.insert(name.clone(), json!(*value));
    }
    Response::Tick {
        t: snap.timestamp,
        seq: snap.seq,
        data,
    }
}

/// Format a history response from a slice of snapshots.
pub fn format_history(snapshots: &[TickSnapshot]) -> Response {
    let ticks: Vec<Value> = snapshots
        .iter()
        .map(|s| {
            let mut data = serde_json::Map::new();
            for (name, value) in &s.sensors {
                data.insert(name.clone(), json!(*value));
            }
            json!({
                "t": s.timestamp,
                "seq": s.seq,
                "data": data,
            })
        })
        .collect();
    Response::History {
        count: ticks.len(),
        ticks,
    }
}

/// Format an ack response for an actuator command.
pub fn format_ack(command: &str, name: &str, value: f64) -> Response {
    Response::Ack {
        command: command.to_string(),
        name: Some(name.to_string()),
        value: Some(value),
        id: None,
    }
}

/// Format an ack response for an alarm set command.
pub fn format_ack_alarm(id: &str) -> Response {
    Response::Ack {
        command: "alarm_set".to_string(),
        name: None,
        value: None,
        id: Some(id.to_string()),
    }
}

/// Format an alarm list response from a room.
pub fn format_alarm_list(room: &Room) -> Response {
    let alarms = room
        .alarms
        .iter()
        .map(|a| AlarmEntry {
            id: a.id.clone(),
            condition: a.condition_string(),
            cooldown_sec: a.cooldown_sec,
            last_triggered: a.last_triggered,
            state: match a.last_triggered {
                Some(_) => "active".to_string(),
                None => "idle".to_string(),
            },
        })
        .collect();
    Response::AlarmList { alarms }
}

/// Format a spontaneous alarm notification.
pub fn format_alarm_notification(
    id: &str,
    triggered_at: f64,
    snapshot: &TickSnapshot,
) -> Response {
    let mut data = serde_json::Map::new();
    for (name, value) in &snapshot.sensors {
        data.insert(name.clone(), json!(*value));
    }
    Response::Alarm {
        id: id.to_string(),
        triggered_at,
        data,
    }
}

/// Format a subscribed response.
pub fn format_subscribed(tick_hz: f64) -> Response {
    Response::Subscribed { tick_hz }
}

/// Format an unsubscribed response.
pub fn format_unsubscribed() -> Response {
    Response::Unsubscribed
}

/// Format a bye response.
pub fn format_bye() -> Response {
    Response::Bye
}

/// Format a help response.
pub fn format_help() -> Response {
    Response::Help {
        commands: vec![
            "tick".into(),
            "history [N]".into(),
            "actuator <name> <value>".into(),
            "alarm list".into(),
            "alarm set <id> <condition> <cooldown>".into(),
            "subscribe".into(),
            "unsubscribe".into(),
            "help".into(),
            "quit".into(),
        ],
    }
}

/// Format an error response.
pub fn format_error(message: &str) -> Response {
    Response::Error {
        message: message.to_string(),
    }
}

// ─── Response Parsing (agent-side) ───────────────────────────────

/// Parse a JSON response line from a room.
///
/// Returns the typed `Response`, or an `Error` response on failure.
pub fn parse_response(line: &str) -> Response {
    match serde_json::from_str::<Response>(line.trim()) {
        Ok(resp) => resp,
        Err(_) => Response::Error {
            message: format!("failed to parse response: {}", line),
        },
    }
}

// ─── Command Builders (agent-side) ───────────────────────────────

/// Build a `tick` command string.
pub fn cmd_tick() -> String {
    "tick".into()
}

/// Build a `history N` command string.
pub fn cmd_history(n: usize) -> String {
    format!("history {}", n)
}

/// Build an `actuator` command string.
pub fn cmd_actuator(name: &str, value: f64) -> String {
    format!("actuator {} {}", name, value)
}

/// Build an `alarm list` command string.
pub fn cmd_alarm_list() -> String {
    "alarm list".into()
}

/// Build an `alarm set` command string.
pub fn cmd_alarm_set(id: &str, sensor: &str, op: AlarmOp, threshold: f64, cooldown: f64) -> String {
    format!("alarm set {} {} {} {} {}", id, sensor, op.as_str(), threshold, cooldown)
}

/// Build a `subscribe` command string.
pub fn cmd_subscribe() -> String {
    "subscribe".into()
}

/// Build an `unsubscribe` command string.
pub fn cmd_unsubscribe() -> String {
    "unsubscribe".into()
}

/// Build a `help` command string.
pub fn cmd_help() -> String {
    "help".into()
}

/// Build a `quit` command string.
pub fn cmd_quit() -> String {
    "quit".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Command Parsing ─────────────────────────────

    #[test]
    fn test_parse_tick() {
        assert_eq!(parse_command("tick"), Ok(Command::Tick));
        assert_eq!(parse_command("  tick  "), Ok(Command::Tick));
        assert!(parse_command("tick extra").is_err());
    }

    #[test]
    fn test_parse_history() {
        assert_eq!(parse_command("history"), Ok(Command::History(10)));
        assert_eq!(parse_command("history 20"), Ok(Command::History(20)));
        assert_eq!(parse_command("history 0"), Ok(Command::History(0)));
        assert!(parse_command("history abc").is_err());
        assert!(parse_command("history 1 2").is_err());
    }

    #[test]
    fn test_parse_actuator() {
        assert_eq!(
            parse_command("actuator pump 1"),
            Ok(Command::Actuator {
                name: "pump".into(),
                value: 1.0,
            })
        );
        assert_eq!(
            parse_command("actuator throttle 0.5"),
            Ok(Command::Actuator {
                name: "throttle".into(),
                value: 0.5,
            })
        );
        assert!(parse_command("actuator pump").is_err());
        assert!(parse_command("actuator").is_err());
    }

    #[test]
    fn test_parse_alarm_list() {
        assert_eq!(parse_command("alarm list"), Ok(Command::AlarmList));
        assert!(parse_command("alarm list extra").is_err());
        assert!(parse_command("alarm").is_err());
    }

    #[test]
    fn test_parse_alarm_set() {
        let cmd = parse_command("alarm set overheat coolant_temp > 95 30").unwrap();
        match cmd {
            Command::AlarmSet {
                id,
                sensor,
                op,
                threshold,
                cooldown_sec,
            } => {
                assert_eq!(id, "overheat");
                assert_eq!(sensor, "coolant_temp");
                assert_eq!(op, AlarmOp::Gt);
                assert!((threshold - 95.0).abs() < f64::EPSILON);
                assert!((cooldown_sec - 30.0).abs() < f64::EPSILON);
            }
            _ => panic!("expected AlarmSet"),
        }

        // Test all operators
        for (op_str, expected) in [
            ("<", AlarmOp::Lt),
            (">", AlarmOp::Gt),
            ("==", AlarmOp::Eq),
            ("!=", AlarmOp::Ne),
            ("<=", AlarmOp::Le),
            (">=", AlarmOp::Ge),
        ] {
            let cmd =
                parse_command(&format!("alarm set a s {} 1 5", op_str)).unwrap();
            if let Command::AlarmSet { op, .. } = cmd {
                assert_eq!(op, expected, "failed for operator {}", op_str);
            }
        }

        // Invalid operator
        assert!(parse_command("alarm set a s ~= 1 5").is_err());
        // Wrong arg count
        assert!(parse_command("alarm set a s > 5").is_err());
    }

    #[test]
    fn test_parse_subscribe_unsubscribe() {
        assert_eq!(parse_command("subscribe"), Ok(Command::Subscribe));
        assert_eq!(parse_command("unsubscribe"), Ok(Command::Unsubscribe));
        assert!(parse_command("subscribe extra").is_err());
    }

    #[test]
    fn test_parse_help_quit() {
        assert_eq!(parse_command("help"), Ok(Command::Help));
        assert_eq!(parse_command("quit"), Ok(Command::Quit));
    }

    #[test]
    fn test_parse_unknown() {
        assert!(parse_command("frobnicate").is_err());
        assert!(parse_command("").is_err());
    }

    // ─── Response Formatting ──────────────────────────

    #[test]
    fn test_format_welcome() {
        let room = Room::new("engine_room", 0.2);
        let resp = format_welcome(&room);
        let json = resp.to_json_line();

        assert!(json.contains(r#""type":"welcome""#));
        assert!(json.contains(r#""room_id":"engine_room""#));
        assert!(json.contains("0.2"));
    }

    #[test]
    fn test_format_tick_response() {
        let snap = TickSnapshot {
            timestamp: 1749234437.0,
            seq: 42,
            sensors: vec![
                ("coolant_temp_c".into(), 96.3),
                ("bilge_cm".into(), 7.0),
            ],
            actuators: vec![],
        };
        let resp = format_tick(&snap);
        let json = resp.to_json_line();

        assert!(json.contains(r#""type":"tick""#));
        assert!(json.contains(r#""seq":42"#));
        assert!(json.contains("1749234437"));
        assert!(json.contains("coolant_temp_c"));
        assert!(json.contains("96.3"));
    }

    #[test]
    fn test_format_history_response() {
        let snapshots = vec![
            TickSnapshot {
                timestamp: 100.0,
                seq: 1,
                sensors: vec![("temp".into(), 50.0)],
                actuators: vec![],
            },
            TickSnapshot {
                timestamp: 200.0,
                seq: 2,
                sensors: vec![("temp".into(), 55.0)],
                actuators: vec![],
            },
        ];
        let resp = format_history(&snapshots);
        let json = resp.to_json_line();

        assert!(json.contains(r#""type":"history""#));
        assert!(json.contains(r#""count":2"#));
        assert!(json.contains(r#""seq":1"#));
        assert!(json.contains(r#""seq":2"#));
    }

    #[test]
    fn test_format_ack() {
        let resp = format_ack("actuator", "bilge_pump", 1.0);
        let json = resp.to_json_line();
        assert!(json.contains(r#""type":"ack""#));
        assert!(json.contains(r#""command":"actuator""#));
        assert!(json.contains(r#""name":"bilge_pump""#));
        assert!(json.contains(r#""value":1.0"#));
    }

    #[test]
    fn test_format_error() {
        let resp = format_error("actuator 'buzzer' not found");
        let json = resp.to_json_line();
        assert!(json.contains(r#""type":"error""#));
        assert!(json.contains("buzzer"));
    }

    #[test]
    fn test_format_subscribed() {
        let resp = format_subscribed(0.2);
        let json = resp.to_json_line();
        assert!(json.contains(r#""type":"subscribed""#));
        assert!(json.contains("0.2"));
    }

    #[test]
    fn test_format_unsubscribed() {
        let resp = format_unsubscribed();
        let json = resp.to_json_line();
        assert!(json.contains(r#""type":"unsubscribed""#));
    }

    #[test]
    fn test_format_bye() {
        let resp = format_bye();
        let json = resp.to_json_line();
        assert!(json.contains(r#""type":"bye""#));
    }

    #[test]
    fn test_format_help() {
        let resp = format_help();
        let json = resp.to_json_line();
        assert!(json.contains(r#""type":"help""#));
        assert!(json.contains("tick"));
        assert!(json.contains("history"));
        assert!(json.contains("subscribe"));
    }

    #[test]
    fn test_format_alarm_notification() {
        let snap = TickSnapshot {
            timestamp: 1749234437.0,
            seq: 42,
            sensors: vec![("coolant_temp_c".into(), 96.3)],
            actuators: vec![],
        };
        let resp = format_alarm_notification("overheat", 1749234437.0, &snap);
        let json = resp.to_json_line();

        assert!(json.contains(r#""type":"alarm""#));
        assert!(json.contains(r#""id":"overheat""#));
        assert!(json.contains("triggered_at"));
    }

    #[test]
    fn test_format_alarm_list() {
        let mut room = Room::new("test", 1.0);
        room.add_alarm("overheat", "temp", AlarmOp::Gt, 95.0, 30.0);
        room.add_alarm("low_pressure", "pressure", AlarmOp::Lt, 3.0, 60.0);

        let resp = format_alarm_list(&room);
        let json = resp.to_json_line();

        assert!(json.contains(r#""type":"alarm_list""#));
        assert!(json.contains(r#""id":"overheat""#));
        assert!(json.contains(r#""id":"low_pressure""#));
        assert!(json.contains("cooldown_sec"));
    }

    // ─── Command Builders ─────────────────────────────

    #[test]
    fn test_command_builders() {
        assert_eq!(cmd_tick(), "tick");
        assert_eq!(cmd_history(20), "history 20");
        assert_eq!(cmd_actuator("pump", 1.0), "actuator pump 1");
        assert_eq!(cmd_alarm_list(), "alarm list");
        assert_eq!(
            cmd_alarm_set("overheat", "temp", AlarmOp::Gt, 95.0, 30.0),
            "alarm set overheat temp > 95 30"
        );
        assert_eq!(cmd_subscribe(), "subscribe");
        assert_eq!(cmd_unsubscribe(), "unsubscribe");
        assert_eq!(cmd_help(), "help");
        assert_eq!(cmd_quit(), "quit");
    }

    // ─── Round-trip Tests ─────────────────────────────

    #[test]
    fn test_response_roundtrip() {
        let resp = format_tick(&TickSnapshot {
            timestamp: 1000.0,
            seq: 5,
            sensors: vec![("temp".into(), 42.0)],
            actuators: vec![],
        });
        let json = resp.to_json_line();
        let parsed = parse_response(&json);

        match parsed {
            Response::Tick { t, seq, data } => {
                assert!((t - 1000.0).abs() < f64::EPSILON);
                assert_eq!(seq, 5);
                assert_eq!(data.get("temp").and_then(|v| v.as_f64()), Some(42.0));
            }
            _ => panic!("expected Tick response"),
        }
    }

    #[test]
    fn test_command_roundtrip() {
        let cmds = [
            ("tick", Command::Tick),
            ("history 5", Command::History(5)),
            ("subscribe", Command::Subscribe),
            ("unsubscribe", Command::Unsubscribe),
            ("help", Command::Help),
            ("quit", Command::Quit),
        ];

        for (line, expected) in &cmds {
            let parsed = parse_command(line).unwrap();
            assert_eq!(&parsed, expected, "roundtrip failed for: {}", line);
        }
    }
}

