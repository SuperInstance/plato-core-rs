//! # PLATO Core
//!
//! Rust implementation of the PLATO Core protocol — Room, Sensor, Actuator,
//! Alarm, history buffer, and wire protocol parsing/formatting.
//!
//! ## Quick Start
//!
//! ```rust
//! use plato_core::{Room, AlarmOp};
//!
//! let mut room = Room::new("engine_room", 0.2);
//! room.add_sensor("coolant_temp_c", 90.0);
//! room.add_actuator("radiator_fan", 0.0);
//! room.add_alarm("overheat", "coolant_temp_c", AlarmOp::Gt, 95.0, 30.0);
//!
//! let snapshot = room.tick();
//! assert_eq!(snapshot.seq, 1);
//! ```
//!
//! See [`wire`] for protocol-level parsing and formatting.

pub mod room;
pub mod wire;

// Re-export core types at crate root for convenience
pub use room::Room;
pub use wire::{
    parse_command, format_ack, format_alarm_list, format_alarm_notification, format_bye,
    format_error, format_help, format_history, format_subscribed, format_tick,
    format_unsubscribed, format_welcome, AlarmEntry, Command, Response,
};


/// Default TCP port for PLATO wire protocol.
pub const DEFAULT_PORT: u16 = 1234;

/// Protocol version string.
pub const PROTOCOL_VERSION: &str = "0.1";

/// Maximum history buffer size (ticks retained).
pub const MAX_HISTORY: usize = 10_000;

// ─── Core Types ───────────────────────────────────────────────────

/// A sensor reading. Sensors are read-only data sources (temperature, RPM, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct Sensor {
    /// Human-readable sensor name (e.g. `"coolant_temp_c"`).
    pub name: String,
    /// Current sensor value.
    pub value: f64,
}

impl Sensor {
    /// Create a new sensor with the given name and initial value.
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self { name: name.into(), value }
    }
}

/// An actuator. Actuators are writable outputs (pumps, fans, valves, etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct Actuator {
    /// Human-readable actuator name (e.g. `"bilge_pump"`).
    pub name: String,
    /// Current actuator value (0.0 = off, 1.0 = fully on, or a percentage).
    pub value: f64,
}

impl Actuator {
    /// Create a new actuator with the given name and initial value.
    pub fn new(name: impl Into<String>, value: f64) -> Self {
        Self { name: name.into(), value }
    }
}

/// Comparison operator used by alarm conditions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlarmOp {
    /// Less than `<`
    Lt,
    /// Greater than `>`
    Gt,
    /// Equal to `==`
    Eq,
    /// Not equal to `!=`
    Ne,
    /// Less than or equal `<=`
    Le,
    /// Greater than or equal `>=`
    Ge,
}

impl AlarmOp {
    /// Evaluate the operator against two f64 values.
    pub fn evaluate(&self, sensor: f64, threshold: f64) -> bool {
        match self {
            AlarmOp::Lt => sensor < threshold,
            AlarmOp::Gt => sensor > threshold,
            AlarmOp::Eq => (sensor - threshold).abs() < f64::EPSILON,
            AlarmOp::Ne => (sensor - threshold).abs() >= f64::EPSILON,
            AlarmOp::Le => sensor <= threshold,
            AlarmOp::Ge => sensor >= threshold,
        }
    }

    /// Parse an operator from a string token like `">"`, `"<="`, etc.
    pub fn from_str(op: &str) -> Option<Self> {
        match op.trim() {
            "<" => Some(AlarmOp::Lt),
            ">" => Some(AlarmOp::Gt),
            "==" => Some(AlarmOp::Eq),
            "!=" => Some(AlarmOp::Ne),
            "<=" => Some(AlarmOp::Le),
            ">=" => Some(AlarmOp::Ge),
            _ => None,
        }
    }

    /// Render the operator back to its string form.
    pub fn as_str(&self) -> &'static str {
        match self {
            AlarmOp::Lt => "<",
            AlarmOp::Gt => ">",
            AlarmOp::Eq => "==",
            AlarmOp::Ne => "!=",
            AlarmOp::Le => "<=",
            AlarmOp::Ge => ">=",
        }
    }
}

/// An alarm rule: triggers when `sensor OP threshold` is true.
#[derive(Debug, Clone, PartialEq)]
pub struct Alarm {
    /// Unique alarm identifier (e.g. `"overheat"`).
    pub id: String,
    /// Name of the sensor this alarm watches.
    pub sensor: String,
    /// Comparison operator.
    pub op: AlarmOp,
    /// Threshold value for triggering.
    pub threshold: f64,
    /// Minimum seconds between triggers (prevents alarm spam).
    pub cooldown_sec: f64,
    /// Timestamp of the last trigger, or `None` if never triggered.
    pub last_triggered: Option<f64>,
}

impl Alarm {
    /// Create a new alarm rule.
    pub fn new(
        id: impl Into<String>,
        sensor: impl Into<String>,
        op: AlarmOp,
        threshold: f64,
        cooldown_sec: f64,
    ) -> Self {
        Self {
            id: id.into(),
            sensor: sensor.into(),
            op,
            threshold,
            cooldown_sec,
            last_triggered: None,
        }
    }

    /// Build the condition string (e.g. `"coolant_temp_c > 95"`).
    pub fn condition_string(&self) -> String {
        format!("{} {} {}", self.sensor, self.op.as_str(), self.threshold)
    }

    /// Render the alarm as a human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "[{}] {} (cooldown {}s, {})",
            self.id,
            self.condition_string(),
            self.cooldown_sec,
            match self.last_triggered {
                Some(t) => format!("last triggered at {}", t),
                None => "idle".to_string(),
            }
        )
    }
}

/// A snapshot of all sensor and actuator values at a single tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TickSnapshot {
    /// Unix timestamp (seconds since epoch).
    pub timestamp: f64,
    /// Monotonic tick sequence number.
    pub seq: u64,
    /// Sensor readings as `(name, value)` pairs.
    pub sensors: Vec<(String, f64)>,
    /// Actuator states as `(name, value)` pairs.
    pub actuators: Vec<(String, f64)>,
}

impl TickSnapshot {
    /// Create a new snapshot for the given timestamp and sequence number.
    pub fn new(timestamp: f64, seq: u64) -> Self {
        Self {
            timestamp,
            seq,
            sensors: Vec::new(),
            actuators: Vec::new(),
        }
    }

    /// Look up a sensor value by name.
    pub fn sensor(&self, name: &str) -> Option<f64> {
        self.sensors.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }

    /// Look up an actuator value by name.
    pub fn actuator(&self, name: &str) -> Option<f64> {
        self.actuators.iter().find(|(n, _)| n == name).map(|(_, v)| *v)
    }
}
