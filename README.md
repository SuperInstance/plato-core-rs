# 🏛️ PLATO Core (Rust)

![Crates.io](https://img.shields.io/crates/v/plato-core)
![Rust](https://img.shields.io/badge/rust-stable-orange)
![Tests](https://img.shields.io/badge/tests-40%2B-brightgreen)
![License](https://img.shields.io/badge/License-MIT-yellow)

**Rust implementation of the PLATO Core protocol** — Room, Sensor, Actuator, Alarm, history buffer, and wire protocol parsing/formatting.

PLATO is a lightweight, text-based, line-delimited protocol for agent-to-room communication. It's designed so that a human can type commands in a terminal, an LLM can parse responses without special tooling, and an ESP32 can generate responses in under 1KB of code.

---

## Philosophy

Part of [Working Animal Architecture](https://github.com/SuperInstance/AI-Writings), where **γ + η = C** (genome + nurture = capability). A PLATO Room is the **pasture** — the bounded space where working animals operate. Sensors are the fence wires; actuators are the gates; alarms are the barking dogs. The protocol is simple enough that anything from an LLM to a microcontroller can be a working animal in the room.

> *Type a command. Get a JSON line. That's the whole protocol.*

## Why PLATO?

1. A **human** can type commands in a terminal (`nc localhost 1234`)
2. An **LLM** can parse responses without special tooling
3. An **ESP32** can generate responses in <1KB of code
4. A **Rust binary** can connect in a few lines

## Installation

```bash
cargo add plato-core
```

Or in `Cargo.toml`:

```toml
[dependencies]
plato-core = "0.1"
```

## Quick Start

### Room with Sensors, Actuators, and Alarms

```rust
use plato_core::{Room, AlarmOp};

let mut room = Room::new("engine_room", 0.2); // 0.2 Hz = 1 tick / 5s

// Register sensors
room.add_sensor("coolant_temp_c", 90.0);
room.add_sensor("oil_pressure_psi", 45.0);
room.add_sensor("rpm", 1800.0);

// Register actuators
room.add_actuator("radiator_fan", 0.0);
room.add_actuator("oil_pump", 1.0);

// Register alarms
room.add_alarm("overheat", "coolant_temp_c", AlarmOp::Gt, 95.0, 30.0);
room.add_alarm("low_oil", "oil_pressure_psi", AlarmOp::Lt, 20.0, 60.0);

// Tick the room — read sensors, check alarms, snapshot history
let (snapshot, triggered) = room.tick_and_check();
println!("Tick {}: {} sensors, {} alarms fired",
    snapshot.seq, snapshot.sensors.len(), triggered.len());
```

### Wire Protocol — Parsing Commands

```rust
use plato_core::wire::{parse_command, Command};

match parse_command("actuator bilge_pump 1") {
    Ok(Command::Actuator { name, value }) => {
        println!("Set {} to {}", name, value);
    }
    Ok(cmd) => println!("Other command: {:?}", cmd),
    Err(e) => println!("Error: {}", e),
}
```

### Wire Protocol — Formatting Responses

```rust
use plato_core::wire::{format_welcome, format_tick, format_error};

let room = Room::new("engine_room", 0.2);

// Welcome message (sent on connect)
let welcome = format_welcome(&room);
println!("{}", welcome.to_json_line());
// {"type":"welcome","room_id":"engine_room","tick_hz":0.2,"sensors":[],"format":"json"}

// Error response
let err = format_error("actuator 'buzzer' not found");
println!("{}", err.to_json_line());
// {"type":"error","message":"actuator 'buzzer' not found"}
```

### Command Builders (Agent-Side)

```rust
use plato_core::wire::*;
use plato_core::AlarmOp;

let cmds = vec![
    cmd_tick(),
    cmd_history(20),
    cmd_actuator("bilge_pump", 1.0),
    cmd_alarm_list(),
    cmd_alarm_set("overheat", "coolant_temp_c", AlarmOp::Gt, 95.0, 30.0),
    cmd_subscribe(),
    cmd_unsubscribe(),
    cmd_help(),
    cmd_quit(),
];
// Each produces a wire-protocol command string ready to send
```

## Core Types

| Type | Description |
|------|-------------|
| `Room` | Central object grouping sensors, actuators, alarms, and history |
| `Sensor` | Read-only data source (temperature, RPM, etc.) |
| `Actuator` | Writable output (pump, fan, valve) |
| `Alarm` | Rule: triggers when `sensor OP threshold` is true |
| `AlarmOp` | Comparison operator (`Lt`, `Gt`, `Eq`, `Ne`, `Le`, `Ge`) |
| `TickSnapshot` | Point-in-time capture of all sensor and actuator values |

### `AlarmOp` Operators

```rust
use plato_core::AlarmOp;

AlarmOp::Lt  // less than
AlarmOp::Gt  // greater than
AlarmOp::Eq  // equal
AlarmOp::Ne  // not equal
AlarmOp::Le  // less or equal
AlarmOp::Ge  // greater or equal
```

## Wire Protocol

Commands are plain-text lines; responses are single-line JSON objects.

### Commands (Agent → Room)

| Command | Description |
|---------|-------------|
| `tick` | Get current sensor data |
| `history [N]` | Get last N ticks (default 10) |
| `actuator <name> <value>` | Set an actuator |
| `alarm list` | List all alarms |
| `alarm set <id> <sensor> <op> <threshold> <cooldown>` | Add an alarm rule |
| `subscribe` | Receive streaming ticks |
| `unsubscribe` | Stop streaming ticks |
| `help` | List commands |
| `quit` | Disconnect |

### Response Types (Room → Agent)

| Type | Triggered By |
|------|-------------|
| `welcome` | Connection established |
| `tick` | `tick` command or streaming tick |
| `history` | `history N` command |
| `ack` | `actuator` or `alarm set` command |
| `alarm_list` | `alarm list` command |
| `alarm` | Spontaneous alarm notification |
| `subscribed` | `subscribe` command |
| `unsubscribed` | `unsubscribe` command |
| `help` | `help` command |
| `bye` | `quit` command |
| `error` | Any failure |

Full spec: [PLATO_WIRE_PROTOCOL.md](https://github.com/SuperInstance/AI-Writings/blob/main/PLATO_WIRE_PROTOCOL.md)

## API Reference

### `room` Module

| Method | Description |
|--------|-------------|
| `Room::new(id, tick_hz)` | Create a room with given ID and tick frequency |
| `room.add_sensor(name, value)` | Register a sensor |
| `room.add_actuator(name, value)` | Register an actuator |
| `room.add_alarm(id, sensor, op, threshold, cooldown)` | Register an alarm rule |
| `room.tick()` | Execute one tick, return snapshot |
| `room.tick_and_check()` | Execute tick and return triggered alarms |
| `room.set_actuator(name, value)` | Update an actuator value |
| `room.history(n)` | Get last N tick snapshots |
| `room.alarms()` | List all alarms |

### `wire` Module

| Function | Description |
|----------|-------------|
| `parse_command(line)` | Parse a text command into a `Command` enum |
| `format_welcome(room)` | Create welcome response for new connections |
| `format_tick(snapshot)` | Format a tick snapshot as JSON response |
| `format_history(snapshots)` | Format history as JSON response |
| `format_error(msg)` | Format an error response |
| `format_ack(...)` | Format an acknowledgement response |
| `cmd_tick()` / `cmd_history(n)` / etc. | Build command strings for agents |

## Architecture

```
┌──────────────────────── Room ────────────────────────┐
│                                                       │
│  Sensors (read-only)        Actuators (writable)      │
│  ├── coolant_temp_c: 90     ├── radiator_fan: 0.0     │
│  ├── oil_pressure_psi: 45   └── oil_pump: 1.0         │
│  └── rpm: 1800                                        │
│                                                       │
│  Alarms (rule-based)            History (ring buffer) │
│  ├── overheat: temp > 95°C     ├── tick #1: {...}     │
│  └── low_oil: pressure < 20    ├── tick #2: {...}     │
│                                └── ... (up to 10,000) │
│                                                       │
│  Wire Protocol (JSON over TCP)                        │
│  ├── parse_command() ← text in                        │
│  └── format_*()      → JSON out                       │
└───────────────────────────────────────────────────────┘
```

### Crate Layout

```
src/
├── lib.rs    # Core types: Sensor, Actuator, AlarmOp, Room, TickSnapshot
└── wire/
    └── mod.rs  # Protocol: parse_command, format_*, Command, Response, cmd_*
```

## Testing

```bash
# Run all tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Test wire protocol parsing
cargo test wire::

# Test room behavior
cargo test room::
```

## Cross-Implementation

| Aspect | Python | Rust |
|--------|--------|------|
| Package | `pip install plato-core` | `cargo add plato-core` |
| Repo | [plato-core](https://github.com/SuperInstance/plato-core) | [plato-core-rs](https://github.com/SuperInstance/plato-core-rs) (this) |
| Wire protocol | ✅ Compatible | ✅ Compatible |
| Dependencies | stdlib + serde | serde + serde_json |

All implementations share the same PLATO wire protocol specification. An agent written in Python can connect to a room written in Rust, and vice versa.

### Related PLATO Implementations
- **Python** — [plato-core](https://github.com/SuperInstance/plato-core)
- **Rust Runtime Kernel** — [plato-runtime-kernel](https://github.com/SuperInstance/plato-runtime-kernel) (spatial model: tensor grid, batons, assertion traps)
- **Rust Security Audit Room** — [plato-room-security-audit](https://github.com/SuperInstance/plato-room-security-audit-rs)

## Ecosystem

### PLATO Rooms
- [plato-room-security-audit-rs](https://github.com/SuperInstance/plato-room-security-audit-rs) — Automated security auditing as a PLATO room

### FLUX Policy Layer
- [conservation-enforcer-rs](https://github.com/SuperInstance/conservation-enforcer-rs) — Conservation-law enforcement
- [flux-registry-rs](https://github.com/SuperInstance/flux-registry-rs) — Policy registry CLI
- [flux-policy-tester-rs](https://github.com/SuperInstance/flux-policy-tester-rs) — Policy testing framework

### Theory
- [AI-Writings](https://github.com/SuperInstance/AI-Writings) — Paradigm essays and protocol specs

## License

MIT
