//! Conversions between simulation values and Luau values.

use mlua::{Lua, Table, Value as LuaValue};
use tor_simulation::scripting::{
    BindingId, FactSubject, FailureReason, HookFact, TimerId, MAX_TEXT_BYTES,
};
use tor_simulation::{ActorId, ItemId};
use tor_world::{Location, Position, RegionId};

/// The largest integer a Luau number holds exactly.
pub const EXACT_INTEGER: u64 = 1 << 53;

pub fn id(kind: &str, raw: u64) -> String {
    format!("{kind}:{raw}")
}

pub fn parse_id(kind: &str, value: &LuaValue) -> mlua::Result<u64> {
    let text = match value {
        LuaValue::String(text) => text.to_str()?.to_owned(),
        _ => return Err(mlua::Error::runtime(format!("expected a {kind} id"))),
    };
    text.strip_prefix(kind)
        .and_then(|rest| rest.strip_prefix(':'))
        .and_then(|digits| {
            (!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))
                .then(|| digits.parse::<u64>().ok())
                .flatten()
        })
        .ok_or_else(|| mlua::Error::runtime(format!("expected a {kind} id, got {text}")))
}

pub fn actor(value: &LuaValue) -> mlua::Result<ActorId> {
    parse_id("actor", value).map(ActorId)
}
pub fn item(value: &LuaValue) -> mlua::Result<ItemId> {
    parse_id("item", value).map(ItemId)
}
pub fn binding(value: &LuaValue) -> mlua::Result<BindingId> {
    parse_id("binding", value).map(BindingId)
}
pub fn timer(value: &LuaValue) -> mlua::Result<TimerId> {
    parse_id("timer", value).map(TimerId)
}
pub fn region(value: &LuaValue) -> mlua::Result<RegionId> {
    parse_id("region", value).map(RegionId)
}

/// A non-negative integral number below 2^53.
pub fn count(value: &LuaValue, what: &str) -> mlua::Result<u64> {
    let number = match value {
        LuaValue::Integer(n) => *n as f64,
        LuaValue::Number(n) => *n,
        _ => return Err(mlua::Error::runtime(format!("{what} must be a number"))),
    };
    if number.fract() != 0.0 || number < 0.0 || number >= EXACT_INTEGER as f64 {
        return Err(mlua::Error::runtime(format!(
            "{what} must be a whole number from 0 to 2^53"
        )));
    }
    Ok(number as u64)
}

/// A signed integral number within ±2^53.
pub fn integer(value: &LuaValue, what: &str) -> mlua::Result<i64> {
    let number = match value {
        LuaValue::Integer(n) => *n as f64,
        LuaValue::Number(n) => *n,
        _ => return Err(mlua::Error::runtime(format!("{what} must be a number"))),
    };
    if number.fract() != 0.0 || number.abs() >= EXACT_INTEGER as f64 {
        return Err(mlua::Error::runtime(format!(
            "{what} must be a whole number within 2^53"
        )));
    }
    Ok(number as i64)
}

pub fn exact(lua: &Lua, value: u64) -> mlua::Result<LuaValue> {
    if value >= EXACT_INTEGER {
        return Err(mlua::Error::runtime("value exceeds 2^53"));
    }
    let _ = lua;
    Ok(LuaValue::Number(value as f64))
}

pub fn cell(lua: &Lua, at: Location) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("region", id("region", at.region.0))?;
    table.set("x", at.position.x)?;
    table.set("y", at.position.y)?;
    table.set("z", at.position.z)?;
    table.set_readonly(true);
    Ok(table)
}

pub fn parse_cell(value: &LuaValue) -> mlua::Result<Location> {
    let LuaValue::Table(table) = value else {
        return Err(mlua::Error::runtime("expected a cell"));
    };
    let coordinate = |name: &str| -> mlua::Result<i32> {
        let value: LuaValue = table.get(name)?;
        let n = integer(&value, name)?;
        i32::try_from(n).map_err(|_| mlua::Error::runtime("cell coordinate out of range"))
    };
    Ok(Location {
        region: region(&table.get::<LuaValue>("region")?)?,
        position: Position {
            x: coordinate("x")?,
            y: coordinate("y")?,
            z: coordinate("z")?,
        },
    })
}

pub fn text(value: &LuaValue, what: &str) -> mlua::Result<String> {
    match value {
        LuaValue::String(text) => {
            let text = text.to_str()?.to_owned();
            if text.len() > MAX_TEXT_BYTES {
                return Err(mlua::Error::runtime(format!(
                    "{what} exceeds {MAX_TEXT_BYTES} bytes"
                )));
            }
            Ok(text)
        }
        _ => Err(mlua::Error::runtime(format!("{what} must be a string"))),
    }
}

pub fn fact(lua: &Lua, fact: &HookFact) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("kind", fact.kind.id())?;
    table.set("tick", exact(lua, fact.tick)?)?;
    if let Some(actor) = fact.actor {
        table.set("actor", id("actor", actor.0))?;
    }
    match &fact.subject {
        FactSubject::None => {}
        FactSubject::Moved { from, to } => {
            table.set("from", cell(lua, *from)?)?;
            table.set("to", cell(lua, *to)?)?;
        }
        FactSubject::Door { at, open } => {
            table.set("at", cell(lua, *at)?)?;
            table.set("open", *open)?;
        }
        FactSubject::Item {
            item,
            identity,
            quantity,
            at,
        } => {
            table.set("item", id("item", item.0))?;
            table.set("identity", identity.as_str())?;
            table.set("quantity", exact(lua, *quantity)?)?;
            table.set("at", cell(lua, *at)?)?;
        }
        FactSubject::Waited { at } => table.set("at", cell(lua, *at)?)?,
        FactSubject::Attack { target } => table.set("target", id("actor", target.0))?,
        FactSubject::Death { at } => {
            if let Some(at) = at {
                table.set("at", cell(lua, *at)?)?;
            }
        }
        FactSubject::Region { region } => table.set("region", id("region", region.0))?,
        FactSubject::Escaped { at } => table.set("at", cell(lua, *at)?)?,
        FactSubject::PlaneShifted {
            from,
            to,
            from_plane,
            to_plane,
        } => {
            table.set("from", cell(lua, *from)?)?;
            table.set("to", cell(lua, *to)?)?;
            table.set("from_plane", from_plane.as_str())?;
            table.set("plane", to_plane.as_str())?;
        }
        FactSubject::PlaneShiftBlocked { at, plane } => {
            table.set("at", cell(lua, *at)?)?;
            table.set("plane", plane.as_str())?;
        }
        FactSubject::Timer { timer, arg } => {
            table.set("timer", id("timer", timer.0))?;
            table.set("arg", *arg as f64)?;
        }
    }
    table.set_readonly(true);
    Ok(table)
}

/// Bound the runtime's deterministic message for journals.
pub fn failure(error: mlua::Error) -> FailureReason {
    if matches!(error, mlua::Error::MemoryError(_)) {
        return FailureReason::Memory;
    }
    let mut message = match &error {
        mlua::Error::CallbackError { cause, .. } => root_message(cause),
        other => other.to_string(),
    };
    if let Some(stack) = message.find("\nstack traceback:") {
        message.truncate(stack);
    }
    if message.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    FailureReason::Error(message)
}

fn root_message(error: &mlua::Error) -> String {
    match error {
        mlua::Error::CallbackError { cause, .. } => root_message(cause),
        other => other.to_string(),
    }
}
