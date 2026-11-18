# Scenario scripting

A scenario package can carry [Luau](https://luau.org) scripts that react to
what happens in play. Scripts are the scenario's own authority: they can show
players messages, open doors, place and remove items, move actors and end the
run. The [tutorial package](../../scenarios/tutorial) is a complete example.

Scripts are optional. A package without a `[scripting]` table behaves exactly
as before and pays one bit test per possible hook site (see
[Performance](#performance)).

## Declaring scripts and hooks

```toml
[scripting]
scripts = ["scripts/lessons.luau"]

[scripting.variables]
lessons = { scope = "scenario", type = "int", default = 0 }

[[scripting.hooks]]
on = "moved"
anchor = "1/door"
handler = "lessons.at_door"
once = true
```

- `scripts` lists package-relative `.luau` files. Each file's stem is its
  module name. A module returns a table of functions, and each function is a
  handler named `module.function`. Module and function names are lowercase
  letters, digits and underscores.
- `variables` declares every value a script may keep, with its `scope`
  (`scenario`, `region`, `actor` or `item`) and `type` (`bool`, `int` or
  `text`). Only scenario variables take a `default`.
- `hooks` are the initial bindings. Each names one kind (`on`), one handler and
  at most one filter. A filter is one of `anchor` (a `region/name` anchor),
  `region`, `actor`, `item` or `identity` (an item identity). `actors` chooses
  whose facts are heard: `character` (the default), `creature` or `any`.
  `once = true` retires the binding after the first invocation that hears it
  succeeds.

The validator compiles every script, runs each module's top level once under
a small work budget, and checks that every handler exists, every filter fits
its kind and every anchor resolves. Script bytes join the certificate's files,
so an edited script makes the certificate stale.

### Hook kinds

| Kind | Fact fields | Filters |
| --- | --- | --- |
| `run_started` | none | none |
| `moved` | `actor`, `from`, `to` | `anchor`, `region`, `actor` |
| `door_changed` | `actor`, `at`, `open` | `anchor`, `region`, `actor` |
| `item_taken`, `item_dropped` | `actor`, `item`, `identity`, `quantity`, `at` | any |
| `waited` | `actor`, `at` | `anchor`, `region`, `actor` |
| `attack_resolved` | `actor`, `target` | `actor` |
| `died` | `actor`, `at` | `anchor`, `region`, `actor` |
| `region_first_visit` | `actor`, `region` | `anchor`, `region`, `actor` |
| `timer` | `timer`, `arg` | not bindable; a timer calls its own handler |

Every fact also carries `kind` and `tick`. Cells are read-only tables
`{ region = "region:1", x = 3, y = 1, z = 0 }`. Identities are strings such
as `actor:12`, so they keep all 64 bits and compare with `==`.

## Handlers

```lua
local lessons = {}

function lessons.at_door(ctx, facts)
    ctx:message("Open it with {control:open_door}.", { to = facts[1].actor })
end

return lessons
```

A handler receives a context and an array of every fact its bindings heard
at that boundary, in order. Handlers run when the simulation settles a tick
boundary, never in the middle of an action, and never once per tick: a
handler runs only when one of its bindings heard something or its timer is
due.

### Queries

`ctx:tick()`, `ctx:anchor(name)`, `ctx:location(actor)`, `ctx:alive(actor)`,
`ctx:is_character(actor)`, `ctx:characters()`, `ctx:actors_at(cell)`,
`ctx:health(actor)`, `ctx:carried(actor)`, `ctx:items_at(cell)`,
`ctx:item_identity(item)`, `ctx:door_open(cell)`, `ctx:var(name, scope?)`,
`ctx:timer_remaining(timer)` and `ctx:random(n)` (a uniform draw from 1 to
`n` from the scenario's own seeded stream).

Queries read the state at the start of the call. Effects take place after the
handler returns.

### Effects

| Call | Effect |
| --- | --- |
| `ctx:message(text, { style?, to? })` | A user message. `style` is `tutorial` (default), `hint` or `narration`; `to` is an actor, otherwise every character |
| `ctx:set_var(name, value, scope?)`, `ctx:clear_var(name, scope?)` | Persistent typed variables |
| `ctx:start_timer(handler, after, { arg?, owner? })`, `ctx:cancel_timer(timer)` | Call `handler` after `after` ticks |
| `ctx:bind{ on, handler, ...filter, actors?, once?, owner? }`, `ctx:unbind(binding)` | Add or remove a binding while playing |
| `ctx:set_door(cell, open)` | Open or close a door, with no actor needed |
| `ctx:place_item(identity, name, cell)`, `ctx:remove_item(item)` | Items on the ground |
| `ctx:teleport(actor, cell)` | Move an actor |
| `ctx:win(actor?)`, `ctx:lose()` | End an adventure run |

`{control:<id>}` in a message names a control by meaning. Each client shows
its own key or command for it. The ids are `move`, `wait`, `search`,
`pick_up`, `inventory`, `equip`, `drink`, `read`, `zap`, `throw`, `travel`,
`look`, `open_door`, `rest`, `save` and `help`.

Effects are not actions: reach, timing and costs do not apply. Each one is
checked only against world invariants. The target must exist, a destination
must be passable and unoccupied, a variable must match its declared type, and
the stated limits hold.

An owned binding or timer (`owner = actor:…`, `item:…` or `region:…`) goes
away with its owner. A timer owned by a frozen region keeps its remaining time
and resumes when the region thaws; it never catches up.

## Rejection

A root invocation is one handler call for facts from play, together with every
call its effects caused. Effects can cause further facts, which are dispatched
again up to a provisional depth of 4. A root invocation is rejected as a
whole, and the game continues as if it had not run, when any of these happen:

- it exhausts its work budget (counted at Luau calls and loop back-edges plus
  a fixed charge per query and effect);
- it exceeds the 16 MiB memory limit or the cascade depth;
- it raises an error;
- it proposes an effect that breaks an invariant.

A rejection is an authoring defect, not a game event. It is recorded with the
handler and reason in the save's journal and never reaches players. Other
root invocations at the same boundary are unaffected.

## Determinism and the sandbox

- Only `string`, `table`, `math` (without `random`), `bit32`, `utf8` and
  `buffer` are available. There is no I/O, clock, `require`, `load`, `debug`
  or garbage-collector access, and globals are read-only.
- Every call instantiates its module afresh, so module-level locals cannot
  carry state between calls. Use variables or a timer's `arg`.
- Script state (variables, bindings, timers and the script random stream) is
  part of the game. Saves, rewind and replay keep it. Replay calls the same
  handlers and requires identical calls, work, effects and rejections.
- Runtimes are never saved. A restored game gets a fresh runtime built from
  the pinned script text.

Wizard operations do not fire hooks.

## Performance

- Every hook site tests one bit of a per-kind mask before doing any work. A
  fact is built only for a kind some binding listens to.
- A subscribed fact is matched through a per-kind index keyed by its filter
  (anchor cell, region, actor, item, identity), so the cost does not depend
  on how many bindings or regions exist.
- Facts are batched per handler and boundary, and timers are scheduled like
  creature events. An idle script costs nothing.
- Work counters `hook_sites_checked`, `hook_facts_noted` and
  `hook_handler_calls` make this measurable.
