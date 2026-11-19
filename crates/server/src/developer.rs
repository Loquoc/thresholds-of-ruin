use crate::journal::{CreatureAdvancement, Position, RegionView, WizardItem, WizardOperation};
use crate::journal::{Direction, PurchaseReplacement};
use tor_protocol::{ActorId, EntryId};

fn catalog_value<T: serde::de::DeserializeOwned>(name: &str, usage: &str) -> Result<T, String> {
    if !tor_simulation::content_ids::valid_id(name) {
        return Err(usage.into());
    }
    serde_json::from_value(serde_json::Value::String(name.into())).map_err(|_| usage.into())
}

const HELP: &str = "Wizard: where; inspect tile <position>; item spawn/give; condition apply/clear; creature spawn; teleport. Try wizard help <item|condition|resource|creature|teleport|inspect|where|arena|world>.";
const ITEM_HELP: &str = "Usage: wizard item spawn <region x y z | here [offset +DX -DY +DZ]> <identity ID | appearance ID | recipe KEY> [other selectors].\nUsage: wizard item give <self|actor-id> <identity ID | appearance ID | recipe KEY> [other selectors].\nUsage: wizard item list [after <recipe-key>]; wizard item inspect <exact-item-id>.";
const CREATURE_HELP: &str = "Usage: wizard creature spawn <selector> <region x y z | here [offset +DX -DY +DZ]> [depth N]; wizard creature list.";
const TELEPORT_HELP: &str =
    "Usage: wizard teleport <actor> <region x y z | here [offset +DX -DY +DZ]>.";
const INSPECT_HELP: &str = "Usage: wizard inspect tile <region x y z | here [offset +DX -DY +DZ]>.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WizardQuery {
    Where,
    ItemList { after: Option<String> },
    ItemInspect { item: u64 },
    Tile { position: Position },
    Help { topic: Option<String> },
}

pub fn wizard_help(topic: Option<&str>) -> Result<&'static str, String> {
    Ok(match topic {
        None => HELP,
        Some("resource") => "Usage: wizard resource restore <self|actor-id> <stamina|focus|mana> <positive amount>.\nUsage: wizard resource fill <self|actor-id> <stamina|focus|mana>. No health/nutrition/set/drain in this command.",
        Some("item") => ITEM_HELP,
        Some("condition") => "Usage: wizard condition apply <self|actor-id> <condition-id> <active-ticks> [source <self|actor-id>].\nUsage: wizard condition clear <self|actor-id> <condition-id>.",
        Some("creature") => CREATURE_HELP,
        Some("teleport") => TELEPORT_HELP,
        Some("inspect" | "tile") => INSPECT_HELP,
        Some("where") => "Usage: wizard where. Shows the attached actor's location.",
        Some("arena") => "Usage: wizard arena pause | resume | step [actions].",
        Some("world") => {
            "Wizard world commands: chamber, room, door, connect, place, wall, actor, rewind."
        }
        Some(_) => return Err("Unknown wizard help topic. Try wizard help.".into()),
    })
}

pub(crate) fn is_wizard_query(text: &str) -> bool {
    let mut words = text.split_whitespace();
    matches!(
        (words.next(), words.next()),
        (Some("where" | "help" | "inspect"), _) | (Some("item"), Some("list" | "inspect"))
    )
}

/// Parse read-only text commands separately; none become journal operations.
pub fn parse_wizard_query(text: &str, origin: Position) -> Result<Option<WizardQuery>, String> {
    let words: Vec<_> = text.split_whitespace().collect();
    Ok(Some(match words.as_slice() {
        ["item", "list"] => WizardQuery::ItemList { after: None },
        ["item", "list", "after", recipe]
            if recipe.len() <= 80 && crate::wizard_creatures::valid_selector(recipe) =>
        {
            WizardQuery::ItemList {
                after: Some((*recipe).into()),
            }
        }
        ["item", "list", ..] => return Err("Usage: wizard item list [after <recipe-key>].".into()),
        ["item", "inspect", id] => WizardQuery::ItemInspect {
            item: id
                .parse::<u64>()
                .ok()
                .filter(|id| *id > 0 && *id < u64::MAX)
                .ok_or("Usage: wizard item inspect <exact-item-id> (from wizard inspect tile).")?,
        },
        ["item", "inspect", ..] => {
            return Err(
                "Usage: wizard item inspect <exact-item-id> (from wizard inspect tile).".into(),
            )
        }
        ["where"] => WizardQuery::Where,
        ["where", ..] => return Err(wizard_help(Some("where"))?.into()),
        ["inspect", "tile", args @ ..] => WizardQuery::Tile {
            position: crate::wizard_position::exact(args, Some(origin), INSPECT_HELP)?,
        },
        ["inspect", ..] => return Err(INSPECT_HELP.into()),
        ["help"] => WizardQuery::Help { topic: None },
        ["help", topic] => {
            wizard_help(Some(topic))?;
            WizardQuery::Help {
                topic: Some((*topic).into()),
            }
        }
        ["help", ..] => return Err("Usage: wizard help [topic].".into()),
        _ => return Ok(None),
    }))
}

pub fn parse_wizard(text: &str) -> Result<WizardOperation, String> {
    parse_wizard_with_origin(text, None)
}

pub fn parse_wizard_at(text: &str, origin: Position) -> Result<WizardOperation, String> {
    parse_wizard_with_origin(text, Some(origin))
}

fn parse_wizard_with_origin(
    text: &str,
    origin: Option<Position>,
) -> Result<WizardOperation, String> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str(text).map_err(|_| "Invalid developer command JSON".into());
    }
    let words: Vec<_> = text.split_whitespace().collect();
    if words.first() == Some(&"resource") {
        return crate::wizard_resources::parse(&words);
    }
    if words.first() == Some(&"condition") {
        return crate::wizard_conditions::parse(&words);
    }
    let usage = match words.first().copied() {
        Some("item") => ITEM_HELP,
        Some("creature") => CREATURE_HELP,
        Some("teleport") => TELEPORT_HELP,
        Some("arena") => wizard_help(Some("arena"))?,
        _ => HELP,
    };
    let owner = |text: &str| -> Result<u64, String> {
        text.parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| usage.into())
    };
    let purchase_index = |text: &str| -> Result<u16, String> {
        text.parse::<u16>()
            .ok()
            .filter(|value| (1..=256).contains(value))
            .ok_or_else(|| usage.into())
    };
    let position = |v: &[&str]| crate::wizard_position::exact(v, origin, usage);
    let operation = match words.as_slice() {
        ["creature", "spawn", selector, args @ ..] => {
            let (position, tail) = crate::wizard_position::parse(args, origin, usage)?;
            let depth = match tail {
                [] => None,
                ["depth", value] => {
                    Some(value.parse::<u32>().ok().filter(|v| *v > 0).ok_or(usage)?)
                }
                _ => return Err(usage.into()),
            };
            if !crate::wizard_creatures::valid_selector(selector) {
                return Err(usage.into());
            }
            WizardOperation::SpawnCreature {
                selector: (*selector).into(),
                position,
                depth,
            }
        }
        ["creature", "add-hd", actor, source] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::AddHitDie {
                source: catalog_value(source, usage)?,
            },
        },
        ["creature", "train", actor, ordinal, skill] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::Train {
                owner: owner(ordinal)?,
                skill: crate::creature_authoring::skill(skill)
                    .map(|_| (*skill).to_owned())
                    .map_err(|_| usage)?,
            },
        },
        ["creature", "attribute", actor, ordinal, attribute] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::IncreaseAttribute {
                owner: owner(ordinal)?,
                attribute: crate::creature_authoring::attribute(attribute)
                    .map(|_| (*attribute).to_owned())
                    .map_err(|_| usage)?,
            },
        },
        ["creature", "expertise", actor, ordinal, expertise] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::SelectExpertise {
                owner: owner(ordinal)?,
                expertise: catalog_value(expertise, usage)?,
            },
        },
        ["creature", "talent", actor, ordinal, talent] => WizardOperation::AdvanceCreature {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            advancement: CreatureAdvancement::SelectTalent {
                owner: owner(ordinal)?,
                talent: catalog_value(talent, usage)?,
            },
        },
        ["creature", "replace", actor, ordinal, purchase, "refund"] => {
            WizardOperation::AdvanceCreature {
                actor: ActorId(actor.parse().map_err(|_| usage)?),
                advancement: CreatureAdvancement::ReplacePurchase {
                    owner: owner(ordinal)?,
                    purchase: purchase_index(purchase)?,
                    replacement: PurchaseReplacement::Refund {},
                },
            }
        }
        ["creature", "replace", actor, ordinal, purchase, pool, kind, value] => {
            WizardOperation::AdvanceCreature {
                actor: ActorId(actor.parse().map_err(|_| usage)?),
                advancement: CreatureAdvancement::ReplacePurchase {
                    owner: owner(ordinal)?,
                    purchase: purchase_index(purchase)?,
                    replacement: match *kind {
                        "skill" => PurchaseReplacement::SkillRank {
                            pool: catalog_value(pool, usage)?,
                            skill: crate::creature_authoring::skill(value)
                                .map(|_| (*value).to_owned())
                                .map_err(|_| usage)?,
                        },
                        "expertise" => PurchaseReplacement::Expertise {
                            pool: catalog_value(pool, usage)?,
                            expertise: catalog_value(value, usage)?,
                        },
                        "talent" => PurchaseReplacement::Talent {
                            pool: catalog_value(pool, usage)?,
                            talent: catalog_value(value, usage)?,
                        },
                        _ => return Err(usage.into()),
                    },
                },
            }
        }
        ["creature", "template", actor, template, mode] => WizardOperation::SetCreatureTemplate {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            template: (*template).into(),
            enabled: match *mode {
                "on" => true,
                "off" => false,
                _ => return Err(usage.into()),
            },
        },
        ["creature", "remove-hd", actor] => WizardOperation::RemoveCreatureHitDie {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
        },
        ["arena", "pause"] => WizardOperation::ArenaControl {
            paused: true,
            advance: 0,
        },
        ["arena", "resume"] => WizardOperation::ArenaControl {
            paused: false,
            advance: 0,
        },
        ["arena", "step"] => WizardOperation::ArenaControl {
            paused: true,
            advance: 1,
        },
        ["arena", "step", count] => {
            let advance = count.parse::<u64>().map_err(|_| usage)?;
            if !(1..=10000).contains(&advance) {
                return Err(usage.into());
            }
            WizardOperation::ArenaControl {
                paused: true,
                advance,
            }
        }
        ["gravity", region, x, y, z] => WizardOperation::SetGravity {
            region: region.parse().map_err(|_| usage)?,
            vector: [
                x.parse().map_err(|_| usage)?,
                y.parse().map_err(|_| usage)?,
                z.parse().map_err(|_| usage)?,
            ],
        },
        ["cell-gravity", r, x, y, z, gx, gy, gz] => WizardOperation::SetCellGravity {
            position: position(&[r, x, y, z])?,
            vector: [
                gx.parse().map_err(|_| usage)?,
                gy.parse().map_err(|_| usage)?,
                gz.parse().map_err(|_| usage)?,
            ],
        },
        ["velocity", actor, x, y, z] => WizardOperation::SetVelocity {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            velocity: [
                x.parse().map_err(|_| usage)?,
                y.parse().map_err(|_| usage)?,
                z.parse().map_err(|_| usage)?,
            ],
        },
        ["door", r, x, y, z, state, height @ ..] if height.len() <= 1 => {
            WizardOperation::PlaceDoor {
                position: position(&[r, x, y, z])?,
                open: match *state {
                    "open" => true,
                    "closed" => false,
                    _ => return Err(usage.into()),
                },
                height: match height {
                    [] => 1,
                    [cells] => cells.parse().map_err(|_| usage)?,
                    _ => unreachable!("at most one height"),
                },
            }
        }
        ["join", r, x, y, z, facing, r2, x2, y2, z2, turns, width, height] => {
            WizardOperation::ConnectArea {
                from: position(&[r, x, y, z])?,
                to: position(&[r2, x2, y2, z2])?,
                direction: match *facing {
                    "north" => Direction::North,
                    "east" => Direction::East,
                    "south" => Direction::South,
                    "west" => Direction::West,
                    "up" => Direction::Up,
                    "down" => Direction::Down,
                    _ => return Err(usage.into()),
                },
                quarter_turns: turns.parse().map_err(|_| usage)?,
                width: width.parse().map_err(|_| usage)?,
                height: height.parse().map_err(|_| usage)?,
            }
        }
        [kind @ ("room" | "chamber"), id, width, depth, height, name @ ..] if !name.is_empty() => {
            let region = RegionView {
                id: id.parse().map_err(|_| usage)?,
                name: name.join(" "),
                width: width.parse().map_err(|_| usage)?,
                depth: depth.parse().map_err(|_| usage)?,
                height: height.parse().map_err(|_| usage)?,
            };
            if *kind == "chamber" {
                WizardOperation::PlaceChamber { region }
            } else {
                WizardOperation::PlaceRoom { region }
            }
        }
        ["connect", r, x, y, z, facing, r2, x2, y2, z2, turns] => WizardOperation::Connect {
            from: position(&[r, x, y, z])?,
            direction: match *facing {
                "north" => Direction::North,
                "east" => Direction::East,
                "south" => Direction::South,
                "west" => Direction::West,
                "up" => Direction::Up,
                "down" => Direction::Down,
                _ => return Err(usage.into()),
            },
            to: position(&[r2, x2, y2, z2])?,
            quarter_turns: turns.parse().map_err(|_| usage)?,
        },
        ["place", r, x, y, z, value] => WizardOperation::SetPlaceHint {
            position: position(&[r, x, y, z])?,
            present: match *value {
                "on" => true,
                "off" => false,
                _ => return Err(usage.into()),
            },
        },
        ["wall", r, x, y, z, value] => WizardOperation::SetWall {
            position: position(&[r, x, y, z])?,
            wall: match *value {
                "closed" => true,
                "open" => false,
                _ => return Err(usage.into()),
            },
        },
        ["identify", actor, item] => WizardOperation::IdentifyItem {
            actor: tor_protocol::ActorId(actor.parse().map_err(|_| usage)?),
            item: item.parse().map_err(|_| usage)?,
        },
        ["item", "spawn", args @ ..] => {
            let (position, selectors) = crate::wizard_position::parse(args, origin, usage)?;
            let crate::wizard_items::Selectors {
                identity,
                appearance,
                recipe,
            } = crate::wizard_items::Selectors::parse(selectors).ok_or(usage)?;
            WizardOperation::SpawnItem {
                position,
                identity,
                appearance,
                recipe,
            }
        }
        ["item", "give", actor, selectors @ ..] => {
            crate::wizard_items::parse_give(actor, selectors)?
        }
        ["item", kind, args @ ..] => WizardOperation::PlaceItem {
            kind: match *kind {
                "token" => WizardItem::Token,
                "tablet" => WizardItem::Tablet,
                _ => return Err(usage.into()),
            },
            position: position(args)?,
        },
        ["actor", ticks, args @ ..] => WizardOperation::SpawnActor {
            turn_ticks: ticks.parse().map_err(|_| usage)?,
            position: position(args)?,
        },
        ["teleport", actor, args @ ..] => WizardOperation::Teleport {
            actor: ActorId(actor.parse().map_err(|_| usage)?),
            position: position(args)?,
        },
        ["rewind", target] => WizardOperation::Rewind {
            target: (*target != "initial").then(|| EntryId((*target).into())),
        },
        _ => return Err(usage.into()),
    };
    Ok(operation)
}

#[cfg(test)]
mod arena_tests {
    use super::*;
    #[test]
    fn arena_controls_parse_bounded_steps_and_explicit_pause_resume() {
        for (text, paused, advance) in [
            ("arena pause", true, 0),
            ("arena resume", false, 0),
            ("arena step", true, 1),
            ("arena step 3", true, 3),
        ] {
            assert_eq!(
                parse_wizard(text).unwrap(),
                WizardOperation::ArenaControl { paused, advance }
            );
        }
        for text in [
            "arena",
            "arena step 0",
            "arena step -1",
            "arena step 10001",
            "arena step 18446744073709551616",
            "arena pause 1",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
    }
}

#[cfg(test)]
mod creature_tests {
    use super::*;
    #[test]
    fn removal_names_one_actor_and_rejects_forged_or_invalid_arguments() {
        assert_eq!(
            parse_wizard("creature remove-hd 42").unwrap(),
            WizardOperation::RemoveCreatureHitDie { actor: ActorId(42) }
        );
        for text in [
            "creature remove-hd",
            "creature remove-hd -1",
            "creature remove-hd 1 2",
            "creature remove-hd 18446744073709551616",
            "creature remove-hd nope",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(
            parse_wizard(r#"{"type":"remove_creature_hit_die","actor":1,"hit_dice":99}"#).is_err()
        );
    }
}

#[cfg(test)]
mod template_tests {
    use super::*;
    #[test]
    fn templates_parse_explicit_modes_and_reject_forged_definitions() {
        for (mode, enabled) in [("on", true), ("off", false)] {
            assert_eq!(
                parse_wizard(&format!("creature template 7 arcane {mode}")).unwrap(),
                WizardOperation::SetCreatureTemplate {
                    actor: ActorId(7),
                    template: "arcane".into(),
                    enabled
                }
            );
        }
        for text in [
            "creature template",
            "creature template 1 arcane",
            "creature template -1 arcane on",
            "creature template 1 arcane toggle",
            "creature template 1 arcane on extra",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard(r#"{"type":"set_creature_template","actor":1,"template":"arcane","enabled":true,"grants":[]}"#).is_err());
        assert!(parse_wizard(r#"{"type":"set_creature_template","actor":1,"template":{"kind":"undead"},"enabled":true}"#).is_err());
    }
}

#[cfg(test)]
mod advancement_tests {
    use super::*;
    #[test]
    fn purchase_replacement_uses_strict_one_based_locations_and_typed_choices() {
        for text in [
            "creature replace 1 2 1 refund",
            "creature replace 1 2 1 advancement skill athletics",
            "creature replace 1 2 1 advancement talent hardiness",
        ] {
            let operation = parse_wizard(text).unwrap();
            let restored: WizardOperation =
                serde_json::from_str(&serde_json::to_string(&operation).unwrap()).unwrap();
            assert_eq!(operation, restored);
        }
        for text in [
            "creature replace 1 0 1 refund",
            "creature replace 1 2 0 refund",
            "creature replace 1 2 257 refund",
            "creature replace 1 2 1 refund extra",
            "creature replace 1 2 1 bad! skill athletics",
            "creature replace 1 2 1 advancement skill unknown",
            "creature replace 1 2 1 advancement spell magic_bolt",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard(r#"{"type":"advance_creature","actor":1,"advancement":{"type":"replace_purchase","owner":2,"purchase":1,"replacement":{"kind":"refund","extra":true}}}"#).is_err());
    }
    #[test]
    fn advancement_uses_explicit_one_based_owners_and_named_catalog_values() {
        // Content IDs parse independently of the loaded catalog; resolution checks existence.
        for text in [
            "creature add-hd 1 racial",
            "creature add-hd 1 warrior",
            "creature add-hd 1 mage",
            "creature add-hd 1 bard",
            "creature talent 1 4 unknown",
            "creature expertise 1 4 fixture.knife",
            "creature replace 1 2 1 advancement expertise fixture.knife",
            "creature train 1 4 athletics",
            "creature attribute 1 4 strength",
            "creature talent 1 4 hardiness",
        ] {
            let operation = parse_wizard(text).unwrap();
            assert!(matches!(operation, WizardOperation::AdvanceCreature { .. }));
            let restored: WizardOperation =
                serde_json::from_str(&serde_json::to_string(&operation).unwrap()).unwrap();
            assert_eq!(operation, restored);
        }
        for text in [
            "creature add-hd 1 bad!",
            "creature add-hd -1 warrior",
            "creature add-hd 1 warrior extra",
            "creature train 1 0 athletics",
            "creature train 1 18446744073709551616 athletics",
            "creature train 1 -1 athletics",
            "creature train 1 4 unknown",
            "creature attribute 1 4 unknown",
            "creature talent 1 4 bad!",
            "creature expertise 1 4 bad!",
            "creature expertise 1 0 fixture.knife",
            "creature train 1 athletics",
            "creature attribute 1 strength",
            "creature talent 1 hardiness",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard(r#"{"type":"advance_creature","actor":1,"advancement":{"type":"add_hit_die","source":"warrior","health_seed":42}}"#).is_err());
    }
}

#[cfg(test)]
mod wide_scalar_tests {
    use super::*;
    #[test]
    fn wide_scalar_wizard_hd_owner_is_full_width_and_purchase_is_bounded() {
        assert!(matches!(
            parse_wizard("creature train 1 18446744073709551615 athletics").unwrap(),
            WizardOperation::AdvanceCreature {
                advancement: CreatureAdvancement::Train {
                    owner: u64::MAX,
                    ..
                },
                ..
            }
        ));
        assert!(parse_wizard("creature replace 1 65537 256 refund").is_ok());
        for command in [
            "creature train 1 0 athletics",
            "creature train 1 18446744073709551616 athletics",
            "creature replace 1 65537 257 refund",
        ] {
            assert!(parse_wizard(command).is_err());
        }
    }
}

#[cfg(test)]
mod named_creature_tests {
    use super::*;

    #[test]
    fn named_creature_spawn_rejects_bad_depth_coordinates_and_extra_arguments() {
        for text in [
            "creature spawn",
            "creature spawn rat 1 2 1 0 depth 0",
            "creature spawn rat 1 2 1 0 depth 4294967296",
            "creature spawn rat 1 x 1 0",
            "creature spawn rat 1 2 1 0 extra",
            "creature spawn rat 1 2 1 0 9",
        ] {
            assert!(parse_wizard(text).is_err(), "{text}");
        }
        assert!(parse_wizard("creature spawn rat 1 2 1 0").is_ok());
    }

    #[test]
    fn named_creature_spawn_preserves_an_explicit_authored_recipe_and_depth() {
        let operation =
            parse_wizard("creature spawn population:baseline/foes/rat 1 2 1 0 depth 27")
                .expect("named authored creatures must be spawnable");
        let encoded = serde_json::to_value(&operation).unwrap();
        assert_eq!(encoded["type"], "spawn_creature");
        assert_eq!(encoded["selector"], "population:baseline/foes/rat");
        assert_eq!(encoded["depth"], 27);
        assert_eq!(encoded["position"]["region"], 1);
        assert_eq!(encoded["position"]["x"], 2);
        assert_eq!(
            serde_json::from_value::<WizardOperation>(encoded).unwrap(),
            operation
        );
    }
}

#[cfg(test)]
mod item_spawn_tests {
    use super::*;

    #[test]
    fn authored_item_selectors_parse_without_weakening_legacy_commands() {
        for tail in [
            "recipe rogue.generated_mace",
            "recipe rogue.mace identity item.weapon.mace appearance item.weapon.mace",
            "identity item.potion.hallucination",
            "appearance item.potion.red",
            "identity item.potion.hallucination appearance item.potion.red",
            "appearance item.potion.red identity item.potion.hallucination",
        ] {
            let operation = parse_wizard(&format!("item spawn 1 2 3 0 {tail}")).unwrap();
            let value = serde_json::to_value(&operation).unwrap();
            assert_eq!(value["type"], "spawn_item");
            assert_eq!(value["position"]["x"], 2);
            assert_eq!(
                serde_json::from_value::<WizardOperation>(value).unwrap(),
                operation
            );
        }
        for tail in [
            "",
            "identity",
            "identity potion",
            "appearance red",
            "identity item.potion.hallucination identity item.potion.healing",
            "appearance item.potion.red junk",
            "quantity 2",
            "recipe rogue.mace recipe rogue.generated_mace",
        ] {
            assert!(
                parse_wizard(&format!("item spawn 1 2 3 0 {tail}")).is_err(),
                "{tail}"
            );
        }
        assert!(parse_wizard("item token 1 2 3 0").is_ok());
        assert!(parse_wizard("item tablet 1 2 3 0").is_ok());
    }
}

#[cfg(test)]
mod context_tests {
    use super::*;
    const ORIGIN: Position = Position {
        region: 77,
        x: 8,
        y: -3,
        z: 2,
    };

    #[test]
    fn wizard_here_resolves_to_same_journal_operation_as_absolute() {
        for (relative, absolute) in [
            (
                "item spawn here recipe rogue.mace",
                "item spawn 77 8 -3 2 recipe rogue.mace",
            ),
            (
                "item spawn here offset +2 -5 +0 identity item.potion.healing",
                "item spawn 77 10 -8 2 identity item.potion.healing",
            ),
            (
                "creature spawn rat here offset -2 +0 +1 depth 9",
                "creature spawn rat 77 6 -3 3 depth 9",
            ),
            ("teleport 9 here offset +0 +2 -1", "teleport 9 77 8 -1 1"),
        ] {
            let resolved = parse_wizard_at(relative, ORIGIN).unwrap();
            assert_eq!(resolved, parse_wizard(absolute).unwrap());
            let json = serde_json::to_string(&resolved).unwrap();
            assert!(!json.contains("here"));
            assert_eq!(resolved, parse_wizard(&json).unwrap());
            assert!(parse_wizard(relative).is_err());
        }
    }

    #[test]
    fn wizard_relative_offsets_require_signs_shape_and_checked_coordinates() {
        for bad in [
            "item spawn here offset 1 -2 +3 recipe rogue.mace",
            "item spawn here offset +1 -2 recipe rogue.mace",
            "creature spawn rat here offset +1 --2 +3",
            "teleport 9 here offset +2147483647 +0 +0",
            "teleport 9 here offset +0 -2147483648 +0",
            "teleport 9 here offset +1 +0 +0 88",
        ] {
            let error = parse_wizard_at(bad, ORIGIN).unwrap_err();
            assert!(error.len() < 220, "{error}");
            assert!(!error.contains("Wizard commands:"), "{error}");
        }
        let error = parse_wizard_at("item spawn here", ORIGIN).unwrap_err();
        assert!(error.contains("item spawn") && error.contains("recipe"));
    }

    #[test]
    fn wizard_help_fits_private_report_lines() {
        for topic in [
            None,
            Some("item"),
            Some("creature"),
            Some("teleport"),
            Some("inspect"),
            Some("tile"),
            Some("where"),
            Some("arena"),
            Some("world"),
        ] {
            assert!(wizard_help(topic)
                .unwrap()
                .lines()
                .all(|line| !line.is_empty()
                    && line.len() <= 240
                    && !line.chars().any(char::is_control)));
        }
    }

    #[test]
    fn wizard_read_queries_are_distinct_from_mutating_operations() {
        assert_eq!(
            parse_wizard_query("where", ORIGIN).unwrap(),
            Some(WizardQuery::Where)
        );
        assert_eq!(
            parse_wizard_query("inspect tile here", ORIGIN).unwrap(),
            Some(WizardQuery::Tile { position: ORIGIN })
        );
        assert_eq!(
            parse_wizard_query("inspect tile here offset +2 +1 -1", ORIGIN).unwrap(),
            parse_wizard_query("inspect tile 77 10 -2 1", ORIGIN).unwrap()
        );
        assert_eq!(
            parse_wizard_query("help item", ORIGIN).unwrap(),
            Some(WizardQuery::Help {
                topic: Some("item".into())
            })
        );
        assert_eq!(
            parse_wizard_query("item spawn here recipe rogue.mace", ORIGIN).unwrap(),
            None
        );
        assert!(parse_wizard_query("where unexpected", ORIGIN).is_err());
        assert!(parse_wizard_query("inspect tile here offset +1 +1", ORIGIN).is_err());
        assert!(parse_wizard_query("help unknown", ORIGIN).is_err());
        assert!(wizard_help(Some("item")).unwrap().contains("recipe"));
    }
}

#[cfg(test)]
mod item_query_tests {
    use super::*;
    #[test]
    fn private_item_queries_are_distinct_and_bounded() {
        let at = Position {
            region: 1,
            x: 2,
            y: 2,
            z: 0,
        };
        assert_eq!(
            parse_wizard_query("item list", at).unwrap(),
            Some(WizardQuery::ItemList { after: None })
        );
        assert_eq!(
            parse_wizard_query("item inspect 40", at).unwrap(),
            Some(WizardQuery::ItemInspect { item: 40 })
        );
        for bad in [
            "item inspect 0",
            "item inspect -1",
            "item inspect 18446744073709551615",
            "item inspect",
            "item list after",
            "item list extra",
        ] {
            assert!(parse_wizard_query(bad, at).is_err(), "{bad}");
        }
        assert!(is_wizard_query("item list"));
        assert!(is_wizard_query("item inspect 40"));
        assert!(!is_wizard_query("item give self recipe rogue.mace"));
        assert!(parse_wizard("item inspect 40").is_err());
    }
}
