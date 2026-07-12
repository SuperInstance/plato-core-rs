# plato-core-rs

Rust implementation of the **PLATO Core protocol** — Room, Sensor, Actuator, Alarm, history buffer, and wire protocol parsing/formatting.

Part of the [SuperInstance](https://github.com/SuperInstance) ecosystem.

## Overview

PLATO is a lightweight, text-based, line-delimited protocol for agent-to-room communication. It's designed so that:

1. A human can type commands in a terminal (`nc localhost 1234`)
2. An LLM can parse responses without special tooling
3. An ESP32 can generate responses in <1KB of code
4. A Rust binary can connect in a few lines

This crate provides the core types and wire protocol implementation for Rust.

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
plato-core = "0.1"
```

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
use plato_core::room::Room;

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
// Each produces a wire-protocol command string
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

## Running Tests

```bash
cargo test
```

## Related

- [plato-core (Python)](https://github.com/SuperInstance/plato-core) — Python implementation
- [PLATO Wire Protocol Spec](https://github.com/SuperInstance/AI-Writings/blob/main/PLATO_WIRE_PROTOCOL.md)

## License

MIT
