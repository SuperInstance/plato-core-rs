//! Room lifecycle: tick loop, sensor reads, actuator writes, alarm checks,
//! and history snapshotting.

use crate::{Alarm, AlarmOp, Actuator, MAX_HISTORY, Sensor, TickSnapshot};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// A Room is the central object in PLATO. It groups sensors, actuators, alarms,
/// and a rolling history buffer of tick snapshots.
///
/// Rooms are ticked periodically (at `tick_hz`). Each tick:
/// 1. Reads all sensors
/// 2. Checks all alarms against current sensor values
/// 3. Records a snapshot in the history buffer
/// 4. Returns the snapshot (and any triggered alarms)
#[derive(Debug, Clone)]
pub struct Room {
    /// Human-readable room identifier (e.g. `"engine_room"`).
    pub name: String,
    /// Tick frequency in Hz (ticks per second). 0.2 = 1 tick every 5 seconds.
    pub tick_hz: f64,
    /// Named sensors.
    pub sensors: HashMap<String, Sensor>,
    /// Named actuators.
    pub actuators: HashMap<String, Actuator>,
    /// Alarm rules.
    pub alarms: Vec<Alarm>,
    /// Rolling history of tick snapshots (oldest first).
    pub history: Vec<TickSnapshot>,
    /// Monotonic tick counter.
    pub seq: u64,
}

impl Room {
    /// Create a new room with the given name and tick frequency.
    pub fn new(name: impl Into<String>, tick_hz: f64) -> Self {
        assert!(tick_hz > 0.0, "tick_hz must be positive, got {}", tick_hz);
        Self {
            name: name.into(),
            tick_hz,
            sensors: HashMap::new(),
            actuators: HashMap::new(),
            alarms: Vec::new(),
            history: Vec::new(),
            seq: 0,
        }
    }

    // ─── Sensor Management ───────────────────────────────

    /// Add a sensor with the given initial value.
    pub fn add_sensor(&mut self, name: impl Into<String>, value: f64) {
        let name = name.into();
        self.sensors.insert(name.clone(), Sensor::new(name, value));
    }

    /// Update a sensor's value. Returns `false` if the sensor doesn't exist.
    pub fn set_sensor(&mut self, name: &str, value: f64) -> bool {
        if let Some(s) = self.sensors.get_mut(name) {
            s.value = value;
            true
        } else {
            false
        }
    }

    /// Read a sensor's value. Returns `None` if not found.
    pub fn sensor_value(&self, name: &str) -> Option<f64> {
        self.sensors.get(name).map(|s| s.value)
    }

    /// Get sorted sensor names (for consistent JSON output).
    fn sensor_names_sorted(&self) -> Vec<String> {
        let mut names: Vec<String> = self.sensors.keys().cloned().collect();
        names.sort();
        names
    }

    // ─── Actuator Management ─────────────────────────────

    /// Add an actuator with the given initial value.
    pub fn add_actuator(&mut self, name: impl Into<String>, value: f64) {
        let name = name.into();
        self.actuators.insert(name.clone(), Actuator::new(name, value));
    }

    /// Set an actuator's value. Returns `false` if the actuator doesn't exist.
    pub fn set_actuator(&mut self, name: &str, value: f64) -> bool {
        if let Some(a) = self.actuators.get_mut(name) {
            a.value = value;
            true
        } else {
            false
        }
    }

    /// Read an actuator's value. Returns `None` if not found.
    pub fn actuator_value(&self, name: &str) -> Option<f64> {
        self.actuators.get(name).map(|a| a.value)
    }

    // ─── Alarm Management ────────────────────────────────

    /// Add an alarm rule to the room.
    pub fn add_alarm(
        &mut self,
        id: impl Into<String>,
        sensor: impl Into<String>,
        op: AlarmOp,
        threshold: f64,
        cooldown_sec: f64,
    ) {
        let id_str = id.into();
        // Reject duplicate alarm IDs
        if self.alarms.iter().any(|a| a.id == id_str) {
            return; // Silent no-op for duplicate — caller should remove first
        }
        if cooldown_sec < 0.0 {
            panic!("cooldown_sec must be non-negative, got {}", cooldown_sec);
        }
        self.alarms.push(Alarm::new(id_str, sensor, op, threshold, cooldown_sec));
    }

    /// Remove an alarm by ID. Returns `true` if removed.
    pub fn remove_alarm(&mut self, id: &str) -> bool {
        let before = self.alarms.len();
        self.alarms.retain(|a| a.id != id);
        self.alarms.len() < before
    }

    // ─── Tick Lifecycle ──────────────────────────────────

    /// Advance the room by one tick.
    ///
    /// This:
    /// 1. Increments the sequence counter
    /// 2. Records a snapshot of all sensors and actuators
    /// 3. Checks all alarms (respecting cooldown)
    /// 4. Returns the snapshot and a list of triggered alarm IDs
    pub fn tick(&mut self) -> TickSnapshot {
        self.seq += 1;
        let now = now_unix();

        let mut snapshot = TickSnapshot::new(now, self.seq);

        // Collect sensor readings (sorted for determinism)
        for name in self.sensor_names_sorted() {
            if let Some(s) = self.sensors.get(&name) {
                snapshot.sensors.push((name.clone(), s.value));
            }
        }

        // Collect actuator states (sorted)
        let mut actuator_names: Vec<String> = self.actuators.keys().cloned().collect();
        actuator_names.sort();
        for name in &actuator_names {
            if let Some(a) = self.actuators.get(name) {
                snapshot.actuators.push((name.clone(), a.value));
            }
        }

        // Store in history (with rolling buffer)
        self.history.push(snapshot.clone());
        if self.history.len() >= MAX_HISTORY {
            self.history.remove(0);
        }

        snapshot
    }

    /// Check all alarms against the latest tick snapshot.
    ///
    /// Returns a list of `(alarm_id, triggered)` for alarms that fired.
    /// Updates `last_triggered` on each alarm that fires.
    /// Respects cooldown: an alarm won't fire again until `cooldown_sec`
    /// seconds have passed since its last trigger.
    pub fn check_alarms(&mut self, snapshot: &TickSnapshot) -> Vec<(String, f64)> {
        let now = snapshot.timestamp;
        let mut triggered = Vec::new();

        for alarm in &mut self.alarms {
            // Find the sensor value for this alarm
            if let Some(val) = snapshot.sensor(&alarm.sensor) {
                if alarm.op.evaluate(val, alarm.threshold) {
                    // Check cooldown
                    let can_fire = match alarm.last_triggered {
                        None => true,
                        Some(last) => (now - last) >= alarm.cooldown_sec,
                    };

                    if can_fire {
                        alarm.last_triggered = Some(now);
                        triggered.push((alarm.id.clone(), now));
                    }
                }
            }
        }

        triggered
    }

    /// Convenience: tick and check alarms in one call.
    ///
    /// Returns `(snapshot, triggered_alarms)`.
    pub fn tick_and_check(&mut self) -> (TickSnapshot, Vec<(String, f64)>) {
        let snapshot = self.tick();
        let triggered = self.check_alarms(&snapshot);
        (snapshot, triggered)
    }

    // ─── History ─────────────────────────────────────────

    /// Get the last `n` ticks from history (oldest first).
    /// Returns fewer if not enough history exists.
    pub fn last_n_ticks(&self, n: usize) -> &[TickSnapshot] {
        let len = self.history.len();
        if len <= n {
            &self.history
        } else {
            &self.history[len - n..]
        }
    }

    /// Get the most recent tick, if any.
    pub fn latest_tick(&self) -> Option<&TickSnapshot> {
        self.history.last()
    }

    /// Clear all history.
    pub fn clear_history(&mut self) {
        self.history.clear();
    }

    // ─── Introspection ───────────────────────────────────

    /// List all sensor names (sorted).
    pub fn sensor_names(&self) -> Vec<String> {
        self.sensor_names_sorted()
    }

    /// List all actuator names (sorted).
    pub fn actuator_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.actuators.keys().cloned().collect();
        names.sort();
        names
    }
}

/// Get the current Unix timestamp in seconds (f64).
pub fn now_unix() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_creation() {
        let room = Room::new("engine", 0.2);
        assert_eq!(room.name, "engine");
        assert!((room.tick_hz - 0.2).abs() < f64::EPSILON);
        assert!(room.sensors.is_empty());
        assert!(room.actuators.is_empty());
        assert!(room.alarms.is_empty());
        assert_eq!(room.seq, 0);
    }

    #[test]
    fn test_sensor_management() {
        let mut room = Room::new("test", 1.0);
        room.add_sensor("temp", 50.0);

        assert_eq!(room.sensor_value("temp"), Some(50.0));
        assert!(room.set_sensor("temp", 75.0));
        assert_eq!(room.sensor_value("temp"), Some(75.0));
        assert!(!room.set_sensor("nonexistent", 1.0));
        assert_eq!(room.sensor_value("nonexistent"), None);
    }

    #[test]
    fn test_actuator_management() {
        let mut room = Room::new("test", 1.0);
        room.add_actuator("pump", 0.0);

        assert_eq!(room.actuator_value("pump"), Some(0.0));
        assert!(room.set_actuator("pump", 1.0));
        assert_eq!(room.actuator_value("pump"), Some(1.0));
        assert!(!room.set_actuator("nope", 1.0));
    }

    #[test]
    fn test_tick_increments_seq() {
        let mut room = Room::new("test", 1.0);
        room.add_sensor("temp", 90.0);

        let s1 = room.tick();
        assert_eq!(s1.seq, 1);
        assert_eq!(s1.sensor("temp"), Some(90.0));

        let s2 = room.tick();
        assert_eq!(s2.seq, 2);
    }

    #[test]
    fn test_history_buffer() {
        let mut room = Room::new("test", 1.0);
        room.add_sensor("x", 1.0);

        for _ in 0..5 {
            room.tick();
        }

        assert_eq!(room.history.len(), 5);
        let last3 = room.last_n_ticks(3);
        assert_eq!(last3.len(), 3);
        assert_eq!(last3[0].seq, 3);
        assert_eq!(last3[2].seq, 5);
    }

    #[test]
    fn test_alarm_trigger() {
        let mut room = Room::new("test", 1.0);
        room.add_sensor("temp", 90.0);
        room.add_alarm("overheat", "temp", AlarmOp::Gt, 95.0, 30.0);

        // Temp is 90, below threshold — no alarm
        let snap = room.tick();
        let triggered = room.check_alarms(&snap);
        assert!(triggered.is_empty());

        // Raise temp above threshold
        room.set_sensor("temp", 96.0);
        let snap = room.tick();
        let triggered = room.check_alarms(&snap);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].0, "overheat");
    }

    #[test]
    fn test_alarm_cooldown() {
        let mut room = Room::new("test", 1.0);
        room.add_sensor("temp", 100.0);
        room.add_alarm("overheat", "temp", AlarmOp::Gt, 95.0, 30.0);

        // First trigger
        let snap = room.tick();
        let t1 = room.check_alarms(&snap);
        assert_eq!(t1.len(), 1);

        // Immediate second tick — should not fire (cooldown)
        let snap = room.tick();
        let t2 = room.check_alarms(&snap);
        assert!(t2.is_empty());
    }

    #[test]
    fn test_tick_and_check() {
        let mut room = Room::new("test", 1.0);
        room.add_sensor("pressure", 5.0);
        room.add_alarm("low_pressure", "pressure", AlarmOp::Lt, 3.0, 60.0);

        // Normal — no alarm
        let (_snap, triggered) = room.tick_and_check();
        assert!(triggered.is_empty());

        // Drop pressure
        room.set_sensor("pressure", 2.0);
        let (_snap, triggered) = room.tick_and_check();
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].0, "low_pressure");
    }

    #[test]
    fn test_alarm_removal() {
        let mut room = Room::new("test", 1.0);
        room.add_alarm("a1", "temp", AlarmOp::Gt, 50.0, 10.0);
        room.add_alarm("a2", "temp", AlarmOp::Lt, 10.0, 10.0);

        assert_eq!(room.alarms.len(), 2);
        assert!(room.remove_alarm("a1"));
        assert_eq!(room.alarms.len(), 1);
        assert!(!room.remove_alarm("a1"));
    }

    #[test]
    fn test_snapshot_lookups() {
        let mut snap = TickSnapshot::new(1000.0, 5);
        snap.sensors.push(("temp".into(), 90.0));
        snap.actuators.push(("fan".into(), 1.0));

        assert_eq!(snap.sensor("temp"), Some(90.0));
        assert_eq!(snap.sensor("nonexistent"), None);
        assert_eq!(snap.actuator("fan"), Some(1.0));
        assert_eq!(snap.actuator("nonexistent"), None);
    }
}
