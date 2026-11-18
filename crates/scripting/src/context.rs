//! The `ctx` object handlers receive: scoped queries and effect requests.

use crate::values::{self, id};
use crate::{Meter, Program, RuntimeConfig, EFFECT_COST, QUERY_COST};
use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use std::cell::RefCell;
use tor_simulation::scripting::{
    script_random, ActorClass, Audience, Binding, BindingId, FilterKey, HookFact, HookKind,
    HookOutput, MessageSegment, MessageStyle, Owner, RunEnd, ScriptEffect, ScriptView, TimerId,
    Value, VarScope, MAX_EFFECTS_PER_CALL, MAX_MESSAGE_SEGMENTS,
};
use tor_simulation::{ItemId, ItemIdentity};

pub(crate) struct Call<'a> {
    pub view: ScriptView<'a>,
    pub facts: &'a [HookFact],
    pub program: &'a Program,
    pub config: &'a RuntimeConfig,
    pub meter: &'a Meter,
}

struct CallState {
    effects: Vec<ScriptEffect>,
    rng: u64,
    next_binding: u64,
    next_timer: u64,
    next_item: u64,
}

fn arg(args: &MultiValue, index: usize) -> LuaValue {
    // Index 0 is `ctx` itself for method calls.
    args.get(index + 1).cloned().unwrap_or(LuaValue::Nil)
}

fn budget_error() -> mlua::Error {
    mlua::Error::runtime("work budget exhausted")
}

fn owner(value: &LuaValue) -> mlua::Result<Option<Owner>> {
    match value {
        LuaValue::Nil => Ok(None),
        LuaValue::String(text) => {
            let text = text.to_str()?.to_owned();
            if text.starts_with("actor:") {
                Ok(Some(Owner::Actor(values::actor(value)?)))
            } else if text.starts_with("item:") {
                Ok(Some(Owner::Item(values::item(value)?)))
            } else if text.starts_with("region:") {
                Ok(Some(Owner::Region(values::region(value)?)))
            } else {
                Err(mlua::Error::runtime("owner must be an actor, item or region id"))
            }
        }
        _ => Err(mlua::Error::runtime("owner must be an actor, item or region id")),
    }
}

fn scope(value: &LuaValue) -> mlua::Result<VarScope> {
    Ok(match owner(value)? {
        None => VarScope::Scenario,
        Some(Owner::Actor(actor)) => VarScope::Actor(actor),
        Some(Owner::Item(item)) => VarScope::Item(item),
        Some(Owner::Region(region)) => VarScope::Region(region),
    })
}

fn to_lua(lua: &Lua, value: &Value) -> mlua::Result<LuaValue> {
    Ok(match value {
        Value::Bool(b) => LuaValue::Boolean(*b),
        Value::Int(n) => {
            if n.unsigned_abs() >= values::EXACT_INTEGER {
                return Err(mlua::Error::runtime("stored integer exceeds 2^53"));
            }
            LuaValue::Number(*n as f64)
        }
        Value::Text(text) => LuaValue::String(lua.create_string(text)?),
    })
}

fn from_lua(value: &LuaValue) -> mlua::Result<Value> {
    Ok(match value {
        LuaValue::Boolean(b) => Value::Bool(*b),
        LuaValue::Integer(_) | LuaValue::Number(_) => Value::Int(values::integer(value, "value")?),
        LuaValue::String(_) => Value::Text(values::text(value, "value")?),
        _ => return Err(mlua::Error::runtime("variables hold booleans, numbers or strings")),
    })
}

/// Split `{control:id}` placeholders out of message text.
fn segments(text: &str, controls: &std::collections::BTreeSet<String>) -> mlua::Result<Vec<MessageSegment>> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("{control:") {
        if start > 0 {
            out.push(MessageSegment::Text(rest[..start].to_owned()));
        }
        let after = &rest[start + "{control:".len()..];
        let end = after
            .find('}')
            .ok_or_else(|| mlua::Error::runtime("unterminated {control:...} placeholder"))?;
        let control = &after[..end];
        if !controls.contains(control) {
            return Err(mlua::Error::runtime(format!("unknown control {control}")));
        }
        out.push(MessageSegment::Control(control.to_owned()));
        rest = &after[end + 1..];
    }
    if !rest.is_empty() {
        out.push(MessageSegment::Text(rest.to_owned()));
    }
    if out.is_empty() || out.len() > MAX_MESSAGE_SEGMENTS {
        return Err(mlua::Error::runtime("a message needs 1 to 32 segments"));
    }
    Ok(out)
}

fn array(lua: &Lua, items: impl IntoIterator<Item = String>) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    for (index, item) in items.into_iter().enumerate() {
        table.raw_set(index + 1, item)?;
    }
    Ok(table)
}

pub(crate) fn invoke(
    lua: &Lua,
    chunk: &Function,
    function: &str,
    call: Call<'_>,
) -> mlua::Result<HookOutput> {
    lua.gc_collect()?;
    let state = call.view.state();
    let calls = RefCell::new(CallState {
        effects: Vec::new(),
        rng: state.rng(),
        next_binding: state.next_binding().0,
        next_timer: state.next_timer().0,
        next_item: call.view.next_item().0,
    });
    let view = call.view;
    let meter = call.meter;
    let program = call.program;
    let config = call.config;
    let query = || {
        if meter.charge(QUERY_COST) {
            Ok(())
        } else {
            Err(budget_error())
        }
    };
    let effect = |effect: ScriptEffect| -> mlua::Result<()> {
        if !meter.charge(EFFECT_COST) {
            return Err(budget_error());
        }
        let mut calls = calls.borrow_mut();
        if calls.effects.len() >= MAX_EFFECTS_PER_CALL {
            return Err(mlua::Error::runtime("too many effects in one call"));
        }
        calls.effects.push(effect);
        Ok(())
    };
    lua.scope(|scope| {
        let ctx = lua.create_table()?;
        macro_rules! method {
            ($name:literal, |$lua:ident, $args:ident| $body:expr) => {
                ctx.set(
                    $name,
                    scope.create_function(|$lua, $args: MultiValue| $body)?,
                )?;
            };
        }

        // Queries.
        method!("tick", |lua, _a| {
            query()?;
            values::exact(lua, view.tick())
        });
        method!("anchor", |lua, a| {
            query()?;
            let name = values::text(&arg(&a, 0), "anchor")?;
            let at = config
                .anchors
                .get(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("unknown anchor {name}")))?;
            values::cell(lua, *at)
        });
        method!("location", |lua, a| {
            query()?;
            match view.actor_location(values::actor(&arg(&a, 0))?) {
                Some(at) => Ok(LuaValue::Table(values::cell(lua, at)?)),
                None => Ok(LuaValue::Nil),
            }
        });
        method!("alive", |_lua, a| {
            query()?;
            Ok(view.alive(values::actor(&arg(&a, 0))?))
        });
        method!("is_character", |_lua, a| {
            query()?;
            Ok(view.is_character(values::actor(&arg(&a, 0))?))
        });
        method!("characters", |lua, _a| {
            query()?;
            array(lua, view.characters().map(|actor| id("actor", actor.0)))
        });
        method!("actors_at", |lua, a| {
            query()?;
            let at = values::parse_cell(&arg(&a, 0))?;
            array(lua, view.actors_at(at).into_iter().map(|actor| id("actor", actor.0)))
        });
        method!("health", |lua, a| {
            query()?;
            match view.health(values::actor(&arg(&a, 0))?) {
                Some((current, max)) => Ok(MultiValue::from_vec(vec![
                    values::exact(lua, current)?,
                    values::exact(lua, max)?,
                ])),
                None => Ok(MultiValue::new()),
            }
        });
        method!("carried", |lua, a| {
            query()?;
            let actor = values::actor(&arg(&a, 0))?;
            array(lua, view.carried(actor).into_iter().map(|item| id("item", item.0)))
        });
        method!("items_at", |lua, a| {
            query()?;
            let at = values::parse_cell(&arg(&a, 0))?;
            array(lua, view.items_at(at).into_iter().map(|item| id("item", item.0)))
        });
        method!("item_identity", |_lua, a| {
            query()?;
            Ok(view
                .item_identity(values::item(&arg(&a, 0))?)
                .map(|identity| identity.as_str().to_owned()))
        });
        method!("door_open", |_lua, a| {
            query()?;
            Ok(view.door_open(values::parse_cell(&arg(&a, 0))?))
        });
        method!("var", |lua, a| {
            query()?;
            let name = values::text(&arg(&a, 0), "variable")?;
            match view.var(scope_arg(&a, 1)?, &name) {
                Some(value) => to_lua(lua, value),
                None => Ok(LuaValue::Nil),
            }
        });
        method!("timer_remaining", |lua, a| {
            query()?;
            match view.timer_remaining(values::timer(&arg(&a, 0))?) {
                Some(left) => values::exact(lua, left),
                None => Ok(LuaValue::Nil),
            }
        });
        method!("random", |_lua, a| {
            query()?;
            let bound = values::count(&arg(&a, 0), "random bound")?;
            let mut calls = calls.borrow_mut();
            let draw = script_random(&mut calls.rng, bound)
                .ok_or_else(|| mlua::Error::runtime("random bound must be at least 1"))?;
            Ok((draw + 1) as f64)
        });

        // Effects.
        method!("message", |_lua, a| {
            let text = values::text(&arg(&a, 0), "message")?;
            let options = arg(&a, 1);
            let (style, audience) = match &options {
                LuaValue::Nil => (MessageStyle::Tutorial, Audience::Characters),
                LuaValue::Table(options) => {
                    let style = match options.get::<LuaValue>("style")? {
                        LuaValue::Nil => MessageStyle::Tutorial,
                        value => match values::text(&value, "style")?.as_str() {
                            "tutorial" => MessageStyle::Tutorial,
                            "hint" => MessageStyle::Hint,
                            "narration" => MessageStyle::Narration,
                            other => {
                                return Err(mlua::Error::runtime(format!("unknown style {other}")))
                            }
                        },
                    };
                    let audience = match options.get::<LuaValue>("to")? {
                        LuaValue::Nil => Audience::Characters,
                        value => Audience::Actor(values::actor(&value)?),
                    };
                    (style, audience)
                }
                _ => return Err(mlua::Error::runtime("message options must be a table")),
            };
            effect(ScriptEffect::Message {
                audience,
                style,
                segments: segments(&text, &config.controls)?,
            })
        });
        method!("set_var", |_lua, a| {
            effect(ScriptEffect::SetVar {
                name: values::text(&arg(&a, 0), "variable")?,
                value: from_lua(&arg(&a, 1))?,
                scope: scope_arg(&a, 2)?,
            })
        });
        method!("clear_var", |_lua, a| {
            effect(ScriptEffect::ClearVar {
                name: values::text(&arg(&a, 0), "variable")?,
                scope: scope_arg(&a, 1)?,
            })
        });
        method!("start_timer", |_lua, a| {
            let name = values::text(&arg(&a, 0), "handler")?;
            let handler = program
                .handler_id(&name)
                .ok_or_else(|| mlua::Error::runtime(format!("unknown handler {name}")))?;
            let after = values::count(&arg(&a, 1), "timer delay")?;
            let (arg_value, timer_owner) = match arg(&a, 2) {
                LuaValue::Nil => (0, None),
                LuaValue::Table(options) => (
                    match options.get::<LuaValue>("arg")? {
                        LuaValue::Nil => 0,
                        value => values::integer(&value, "timer arg")?,
                    },
                    owner(&options.get::<LuaValue>("owner")?)?,
                ),
                _ => return Err(mlua::Error::runtime("timer options must be a table")),
            };
            let timer = {
                let mut calls = calls.borrow_mut();
                let timer = TimerId(calls.next_timer);
                calls.next_timer += 1;
                timer
            };
            effect(ScriptEffect::StartTimer {
                id: timer,
                handler,
                owner: timer_owner,
                after,
                arg: arg_value,
            })?;
            Ok(id("timer", timer.0))
        });
        method!("cancel_timer", |_lua, a| {
            effect(ScriptEffect::CancelTimer {
                id: values::timer(&arg(&a, 0))?,
            })
        });
        method!("bind", |_lua, a| {
            let LuaValue::Table(spec) = arg(&a, 0) else {
                return Err(mlua::Error::runtime("bind takes a table"));
            };
            let binding = parse_binding(&spec, program)?;
            let binding_id = {
                let mut calls = calls.borrow_mut();
                let next = BindingId(calls.next_binding);
                calls.next_binding += 1;
                next
            };
            effect(ScriptEffect::Bind {
                id: binding_id,
                binding,
            })?;
            Ok(id("binding", binding_id.0))
        });
        method!("unbind", |_lua, a| {
            effect(ScriptEffect::Unbind {
                id: values::binding(&arg(&a, 0))?,
            })
        });
        method!("set_door", |_lua, a| {
            let open = match arg(&a, 1) {
                LuaValue::Boolean(open) => open,
                _ => return Err(mlua::Error::runtime("set_door needs true or false")),
            };
            effect(ScriptEffect::SetDoor {
                at: values::parse_cell(&arg(&a, 0))?,
                open,
            })
        });
        method!("place_item", |_lua, a| {
            let identity: ItemIdentity = values::text(&arg(&a, 0), "item identity")?
                .parse()
                .map_err(|_| mlua::Error::runtime("invalid item identity"))?;
            let name = values::text(&arg(&a, 1), "item name")?;
            let at = values::parse_cell(&arg(&a, 2))?;
            let item = {
                let mut calls = calls.borrow_mut();
                let item = ItemId(calls.next_item);
                calls.next_item += 1;
                item
            };
            effect(ScriptEffect::PlaceItem {
                id: item,
                identity,
                name,
                at,
            })?;
            Ok(id("item", item.0))
        });
        method!("remove_item", |_lua, a| {
            effect(ScriptEffect::RemoveItem {
                item: values::item(&arg(&a, 0))?,
            })
        });
        method!("teleport", |_lua, a| {
            effect(ScriptEffect::Teleport {
                actor: values::actor(&arg(&a, 0))?,
                to: values::parse_cell(&arg(&a, 1))?,
            })
        });
        method!("win", |_lua, a| {
            let actor = match arg(&a, 0) {
                LuaValue::Nil => None,
                value => Some(values::actor(&value)?),
            };
            effect(ScriptEffect::EndRun {
                end: RunEnd::Victory,
                actor,
            })
        });
        method!("lose", |_lua, _a| {
            effect(ScriptEffect::EndRun {
                end: RunEnd::Defeat,
                actor: None,
            })
        });
        ctx.set_readonly(true);

        let facts = lua.create_table()?;
        for (index, fact) in call.facts.iter().enumerate() {
            facts.raw_set(index + 1, values::fact(lua, fact)?)?;
        }
        facts.set_readonly(true);
        chunk.set_environment(crate::fresh_env(lua)?)?;
        let module: Table = chunk.call(())?;
        let handler: Function = module.get(function)?;
        handler.call::<()>((ctx, facts))?;
        Ok(())
    })?;
    let calls = calls.into_inner();
    Ok(HookOutput {
        effects: calls.effects,
        rng: calls.rng,
    })
}

fn scope_arg(args: &MultiValue, index: usize) -> mlua::Result<VarScope> {
    scope(&arg(args, index))
}

fn parse_binding(spec: &Table, program: &Program) -> mlua::Result<Binding> {
    let on = values::text(&spec.get::<LuaValue>("on")?, "on")?;
    let kind = HookKind::from_id(&on)
        .filter(|kind| *kind != HookKind::Timer)
        .ok_or_else(|| mlua::Error::runtime(format!("unknown hook {on}")))?;
    let name = values::text(&spec.get::<LuaValue>("handler")?, "handler")?;
    let handler = program
        .handler_id(&name)
        .ok_or_else(|| mlua::Error::runtime(format!("unknown handler {name}")))?;
    let mut keys = Vec::new();
    if let value @ LuaValue::Table(_) = spec.get::<LuaValue>("cell")? {
        keys.push(FilterKey::Cell(values::parse_cell(&value)?));
    }
    for (field, parse) in [
        ("region", (|v: &LuaValue| values::region(v).map(FilterKey::Region)) as fn(&LuaValue) -> mlua::Result<FilterKey>),
        ("actor", |v| values::actor(v).map(FilterKey::Actor)),
        ("item", |v| values::item(v).map(FilterKey::Item)),
    ] {
        let value = spec.get::<LuaValue>(field)?;
        if !value.is_nil() {
            keys.push(parse(&value)?);
        }
    }
    if let value @ LuaValue::String(_) = spec.get::<LuaValue>("identity")? {
        keys.push(FilterKey::ItemIdentity(
            values::text(&value, "identity")?
                .parse()
                .map_err(|_| mlua::Error::runtime("invalid item identity"))?,
        ));
    }
    if keys.len() > 1 {
        return Err(mlua::Error::runtime("a binding takes at most one filter"));
    }
    let key = keys.pop().unwrap_or(FilterKey::Any);
    if !kind.accepts(&key) {
        return Err(mlua::Error::runtime(format!("{on} cannot be filtered that way")));
    }
    let actors = match spec.get::<LuaValue>("actors")? {
        LuaValue::Nil => ActorClass::Character,
        value => match values::text(&value, "actors")?.as_str() {
            "character" => ActorClass::Character,
            "creature" => ActorClass::Creature,
            "any" => ActorClass::Any,
            other => return Err(mlua::Error::runtime(format!("unknown actor class {other}"))),
        },
    };
    let once = match spec.get::<LuaValue>("once")? {
        LuaValue::Nil => false,
        LuaValue::Boolean(once) => once,
        _ => return Err(mlua::Error::runtime("once must be a boolean")),
    };
    Ok(Binding {
        kind,
        key,
        actors,
        handler,
        owner: owner(&spec.get::<LuaValue>("owner")?)?,
        once,
    })
}
