//! Strict offline scene descriptions. No runtime or filesystem effects in validation.
use crate::creatures::{Equipment, Look, Move, Variant, Wardrobe};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use tor_client_3d_model::CreatureModel;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Script {
    pub version: u32,
    pub hz: u32,
    pub viewport: [u32; 2],
    pub actors: Vec<Actor>,
    #[serde(default)]
    pub ground_items: Vec<GroundProp>,
    #[serde(default)]
    pub ground_labels: bool,
    #[serde(default = "default_floor")]
    pub floor_color: [f32; 3],
    #[serde(default)]
    pub dungeon: bool,
    #[serde(default)]
    pub pile_picker: bool,
    pub cameras: Vec<CameraSpec>,
    pub events: Vec<Event>,
    #[serde(default)]
    pub captures: Vec<Capture>,
}
fn default_floor() -> [f32; 3] {
    [0.2, 0.21, 0.23]
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GroundProp {
    #[serde(default)]
    pub asset: Option<String>,
    #[serde(default)]
    pub disclosed_name: Option<String>,
    #[serde(default = "default_quantity")]
    pub quantity: u64,
    pub model: String,
    pub position: [f32; 3],
    pub yaw: f32,
}
fn default_quantity() -> u64 {
    1
}
pub(super) fn item_model(name: &str) -> Result<tor_client_3d_model::ItemModel, String> {
    use tor_client_3d_model::ItemModel::*;
    [
        Placeholder,
        Mace,
        Greatsword,
        ShortSword,
        LongSword,
        Dagger,
        Shortbow,
        Arrow,
        Spear,
        Dart,
        Shuriken,
        Fruit,
        Torch,
        Staff,
        LeatherArmor,
        StuddedLeather,
        RingMail,
        ScaleMail,
        ChainMail,
        SplintMail,
        BandedMail,
        PlateMail,
        Bandage,
        Ration,
        Seal,
        Coins,
        Tablet,
        Potion,
        Ring,
        Amulet,
        Scroll,
        Spellbook,
        Wand,
        Gem,
        Pick,
        Pouch,
    ]
    .into_iter()
    .find(|m| format!("{m:?}") == name)
    .ok_or_else(|| format!("unsupported ground model {name}"))
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Actor {
    #[serde(default)]
    pub held_item: Option<String>,
    #[serde(default)]
    pub held_asset: Option<String>,
    pub id: String,
    pub model: String,
    pub seed: u64,
    pub body: Variant,
    pub outfit: String,
    pub equipment: Vec<String>,
    pub position: [f32; 3],
    pub yaw: f32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CameraSpec {
    pub id: String,
    pub position: [f32; 3],
    pub target: [f32; 3],
    pub fov_degrees: f32,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Capture {
    pub tick: u32,
    pub name: String,
    pub camera: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Event {
    pub tick: u32,
    pub actor: String,
    pub operation: Operation,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Operation {
    Action { name: String },
    Velocity { value: [f32; 3] },
    Teleport { position: [f32; 3], yaw: f32 },
    Dress { outfit: String },
    Equipment { items: Vec<String> },
}

pub(super) fn model(name: &str) -> Result<CreatureModel, String> {
    use CreatureModel::*;
    // Exact recipe identities, never the showcase's fallback lookup.
    let models = [
        Placeholder,
        Humanoid,
        Human,
        Sentinel,
        RedSentinel,
        BlueSentinel,
        Mage,
        RedMage,
        Scout,
        Guardian,
        Hobgoblin,
        Zombie,
        Vampire,
        Yeti,
        Troll,
        Bird,
        Emu,
        Equine,
        Quagga,
        Unicorn,
        BlackUnicorn,
        Centaur,
        Serpent,
        Snake,
        Wisp,
        Ice,
        Orc,
        Leprechaun,
        Nymph,
        Medusa,
        Phantom,
        Wraith,
        Bat,
        Kestrel,
        Griffin,
        Dragon,
        Jabberwock,
        Aquator,
        Xeroc,
        Flytrap,
        Rattlesnake,
    ];
    models
        .into_iter()
        .find(|m| format!("{m:?}") == name)
        .ok_or_else(|| format!("unknown model {name}"))
}
pub(super) fn equipment(items: &[String]) -> Result<Equipment, String> {
    let mut result = Equipment::default();
    let mut seen = HashSet::new();
    for item in items {
        if !seen.insert(item) {
            return Err(format!("duplicate equipment {item}"));
        }
        result.select(item)?;
    }
    Ok(result)
}
pub(super) fn action(name: &str) -> Result<Option<Move>, String> {
    Ok(match name {
        "cancel" => None,
        "jump" => Some(Move::Jump),
        "strike" => Some(Move::Strike),
        "thrust" => Some(Move::Thrust),
        "heavy" => Some(Move::Heavy),
        "focus" => Some(Move::Focus),
        "gesture" => Some(Move::Gesture),
        _ => return Err(format!("unsupported action {name}")),
    })
}
impl Actor {
    pub fn look(&self) -> Result<Look, String> {
        let base = Look::preview_adjusted(model(&self.model)?, self.seed, |v| *v = self.body);
        let wardrobe = Wardrobe::parse(&self.outfit)
            .ok_or_else(|| format!("unknown outfit {}", self.outfit))?;
        let gear = equipment(&self.equipment)?;
        base.composed(wardrobe, gear)
            .map_err(|error| format!("{}: {error}", self.id))
    }
}
fn safe_name(s: &str) -> bool {
    let upper = s.to_ascii_uppercase();
    if matches!(upper.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (upper.len() == 4
            && (upper.starts_with("COM") || upper.starts_with("LPT"))
            && matches!(upper.as_bytes()[3], b'1'..=b'9'))
    {
        return false;
    }
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
}
fn vector(v: [f32; 3]) -> bool {
    v.into_iter().all(|x| x.is_finite() && x.abs() <= 1000.0)
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Timeline {
    Capture,
    Live,
}
impl Script {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        Self::parse_for(bytes, Timeline::Capture)
    }
    pub fn parse_live(bytes: &[u8]) -> Result<Self, String> {
        Self::parse_for(bytes, Timeline::Live)
    }
    fn parse_for(bytes: &[u8], timeline: Timeline) -> Result<Self, String> {
        if bytes.len() > 1_048_576 {
            return Err("scene script exceeds 1 MiB".into());
        }
        let result: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        result.validate(timeline)?;
        Ok(result)
    }
    fn validate(&self, timeline: Timeline) -> Result<(), String> {
        if timeline == Timeline::Live && self.floor_color != default_floor() {
            return Err("custom floor color is capture-only".into());
        }
        if timeline == Timeline::Live && (self.dungeon || self.pile_picker) {
            return Err("dungeon fixture is capture-only".into());
        }
        if self.version != 1 {
            return Err("unsupported scene script version".into());
        }
        crate::review_clock::ReviewStep::new(self.hz)?;
        if self.viewport.iter().any(|v| !(320..=2048).contains(v)) {
            return Err("viewport must be 320..2048".into());
        }
        if (self.actors.is_empty() && self.ground_items.is_empty())
            || self.actors.len() > 32
            || self.ground_items.len() > 128
            || self.cameras.is_empty()
            || self.cameras.len() > 16
            || self.events.len() > 4096
            || (timeline == Timeline::Capture && self.captures.is_empty())
            || self.captures.len() > 256
        {
            return Err("scene record count outside limits".into());
        }
        if self
            .floor_color
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("invalid floor color".into());
        }
        if timeline == Timeline::Live && (!self.ground_items.is_empty() || self.ground_labels) {
            return Err("ground items and labels are capture-only".into());
        }
        for item in &self.ground_items {
            if item
                .asset
                .as_deref()
                .is_some_and(|asset| !review_asset(asset))
            {
                return Err("invalid ground appearance asset".into());
            }
            if item.quantity == 0 {
                return Err("ground quantity must be positive".into());
            }
            if item.disclosed_name.as_ref().is_some_and(|name| {
                name.trim().is_empty()
                    || name.chars().count() > 120
                    || name.chars().any(char::is_control)
            }) {
                return Err("invalid disclosed ground name".into());
            }
            if (self.ground_labels || self.pile_picker) && item.disclosed_name.is_none() {
                return Err("ground labels require explicit disclosed names".into());
            }
            item_model(&item.model)?;
            if !vector(item.position) || !item.yaw.is_finite() || item.yaw.abs() > 360.0 {
                return Err("invalid ground item placement".into());
            }
        }
        if self.pile_picker {
            let mut cells = std::collections::BTreeMap::new();
            for item in &self.ground_items {
                *cells.entry(super::ground_cell(item.position)).or_insert(0) += 1;
            }
            if !cells.values().any(|count| *count > 4) {
                return Err("pile picker requires a disclosed pile".into());
            }
        }
        let mut actors = HashSet::new();
        for actor in &self.actors {
            if !safe_name(&actor.id) || !actors.insert(&actor.id) {
                return Err("invalid or duplicate actor id".into());
            }
            if !vector(actor.position) || !actor.yaw.is_finite() || actor.yaw.abs() > 360.0 {
                return Err("invalid actor placement".into());
            }
            let b = actor.body;
            if b.build > 2
                || b.bust > 4
                || b.bust_form > 8
                || b.bust_lean > 2
                || b.figure > 2
                || b.skin > 4
                || b.hair > 4
                || b.hair_tone > 5
                || b.shade > 4
                || b.pattern > 3
            {
                return Err("body variation outside supported bounds".into());
            }
            actor.look()?;
            if actor
                .held_asset
                .as_deref()
                .is_some_and(|asset| !review_asset(asset))
                || (actor.held_asset.is_some() && actor.held_item.is_none())
            {
                return Err("invalid held appearance asset".into());
            }
            if let Some(kind) = &actor.held_item {
                item_model(kind)?;
                if timeline == Timeline::Live || actor.model != "Human" {
                    return Err("held item review requires capture-only Human actor".into());
                }
            }
        }
        let mut cameras = HashSet::new();
        for c in &self.cameras {
            let direction = Vec3::from_array(c.target) - Vec3::from_array(c.position);
            if !safe_name(&c.id)
                || !cameras.insert(&c.id)
                || !vector(c.position)
                || !vector(c.target)
                || direction.length() < 0.01
                || direction.cross(Vec3::Y).length() < 0.01
                || !c.fov_degrees.is_finite()
                || !(10.0..=120.0).contains(&c.fov_degrees)
            {
                return Err("invalid camera".into());
            }
        }
        let end = match timeline {
            Timeline::Capture => {
                let mut names = HashSet::new();
                for c in &self.captures {
                    if !safe_name(&c.name)
                        // Capture IDs become filenames: enforce portable uniqueness while
                        // retaining the authored spelling in manifests and output paths.
                        || !names.insert(c.name.to_ascii_lowercase())
                        || !cameras.contains(&c.camera)
                        || c.tick > self.hz * 120
                    {
                        return Err("invalid capture name, camera or tick".into());
                    }
                }
                self.captures.iter().map(|c| c.tick).max().unwrap()
            }
            Timeline::Live => self.hz * 600,
        };
        let mut resolved = self.actors.clone();
        let mut previous = 0;
        for event in &self.events {
            if event.tick < previous || event.tick > end {
                return Err(match timeline {
                    Timeline::Capture => "events must be ordered and precede final capture",
                    Timeline::Live => "live events must be ordered and within 600 seconds",
                }
                .into());
            }
            previous = event.tick;
            let a = resolved
                .iter_mut()
                .find(|a| a.id == event.actor)
                .ok_or("unknown event actor")?;
            match &event.operation {
                Operation::Action { name } => {
                    action(name)?;
                    if event.tick == 0 {
                        return Err("actions start at tick 1 or later".into());
                    }
                }
                Operation::Velocity { value } => {
                    if !vector(*value) || Vec3::from_array(*value).length() / self.hz as f32 > 0.25
                    {
                        return Err("invalid velocity".into());
                    }
                }
                Operation::Teleport { position, yaw } => {
                    if !vector(*position) || !yaw.is_finite() || yaw.abs() > 360.0 {
                        return Err("invalid teleport".into());
                    }
                }
                Operation::Dress { outfit } => {
                    a.outfit = outfit.clone();
                    a.look()?;
                }
                Operation::Equipment { items } => {
                    a.equipment = items.clone();
                    a.look()?;
                }
            }
        }
        if timeline == Timeline::Capture && self.captures.windows(2).any(|w| w[0].tick > w[1].tick)
        {
            return Err("captures must be ordered by tick".into());
        }
        Ok(())
    }
}

fn review_asset(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 80
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}
