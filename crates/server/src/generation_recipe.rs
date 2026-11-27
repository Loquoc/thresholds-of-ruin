//! Deterministic recipe composition over a bounded group of region slots.
//! Preparation owns temporary geometry only; published terrain is represented
//! by the ordinary simulation region records.
use crate::{scenario_package::RegionDef, Failure};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationGroup {
    pub depth: u32,
    pub recipe: String,
    /// Columns and rows of equally sized region slots.
    #[serde(default = "default_grid")]
    pub grid: [u32; 2],
    /// Region IDs in row-major slot order.
    pub members: Vec<u64>,
}

pub(crate) const MAX_GRID_SIDE: u32 = 8;
pub const GRID_ROOMS_VERSION: u32 = 1;

fn default_grid() -> [u32; 2] {
    [3, 3]
}

/// Authored parameters for the bounded grid-room generator. Stage order and
/// stream identities are implementation details of this versioned algorithm.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GridRoomsPreset {
    pub version: u32,
    pub room_width: [u32; 2],
    pub room_height: [u32; 2],
    pub extra_connections: [u32; 2],
    pub room_omissions: Option<RoomOmissions>,
    pub maze_rooms: Option<MazeRooms>,
    pub lighting: Option<RoomLighting>,
    pub secret_entrances: Option<SecretEntrances>,
    pub hidden_traps: Option<HiddenTraps>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoomOmissions {
    pub count: [u32; 2],
}

impl<'de> Deserialize<'de> for RoomOmissions {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            count: [u32; 2],
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            count: fields.count,
        })
    }
}

/// Optional bounded maze trial among eligible dark rooms. Absence retains the
/// existing recipe and all existing geometry and feature streams.
#[derive(Clone, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MazeRooms {
    pub one_in: u32,
}

impl<'de> Deserialize<'de> for MazeRooms {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            one_in: u32,
        }
        let fields: Fields = crate::creature_authoring::named_map(deserializer)?;
        Ok(Self {
            one_in: fields.one_in,
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomLighting {
    pub darkness_roll: u32,
    pub darkness_start: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecretEntrances {
    pub chance_per_mille: u32,
    pub maximum: u32,
    pub dc: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HiddenTraps {
    pub count: [u32; 2],
    pub dc: i32,
    pub rust_slot: String,
    pub rust_amount: u32,
    pub sleep_condition: String,
    pub sleep_duration: u64,
    #[serde(default)]
    pub sleep_exposure: Option<String>,
    #[serde(default)]
    pub hazards: Option<HazardTraps>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HazardTraps {
    pub bear_condition: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bear_duration: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bear_escape_dc: Option<i32>,
    pub arrow_ammunition: String,
    pub attack_bonus: i32,
    pub poison_exposure: String,
    pub avoidance_dc: i32,
}
impl HazardTraps {
    pub(crate) fn bear_trap_kind(&self) -> Result<crate::scenario_package::TrapKind, Failure> {
        use crate::scenario_package::TrapKind;
        let condition = self.bear_condition.clone();
        match (self.bear_duration, self.bear_escape_dc) {
            (Some(duration), None) => Ok(TrapKind::BearTrap {
                condition,
                duration,
            }),
            (None, Some(escape_dc)) => Ok(TrapKind::BearTrapEscape {
                condition,
                escape_dc,
            }),
            _ => Err(fail(
                "A bear trap requires exactly one of bear_duration or bear_escape_dc",
            )),
        }
    }

    fn valid(&self) -> bool {
        tor_world::Concealment {
            dc: self.avoidance_dc,
        }
        .valid()
            && self
                .bear_trap_kind()
                .is_ok_and(|kind| kind.compiled().valid())
            && tor_world::TrapKind::Arrow {
                attack_bonus: self.attack_bonus,
                ammunition: self.arrow_ammunition.clone(),
            }
            .valid()
            && tor_world::TrapKind::PoisonDart {
                attack_bonus: self.attack_bonus,
                exposure: self.poison_exposure.clone(),
            }
            .valid()
    }
}

impl GridRoomsPreset {
    /// Lower authoring parameters into a checked, deterministic execution plan.
    pub fn to_recipe(&self) -> Result<Recipe, Failure> {
        if self.version != GRID_ROOMS_VERSION {
            return Err(fail("Unsupported grid_rooms preset algorithm version"));
        }
        let mut stages = vec![
            ("stone", Operation::StoneFill),
            ("slots", Operation::GridPartition),
            (
                "rooms",
                Operation::Rooms {
                    width: self.room_width,
                    height: self.room_height,
                },
            ),
            (
                "connections",
                Operation::ConnectedGraph {
                    extra: self.extra_connections,
                },
            ),
            ("corridors", Operation::Corridors),
            ("stairs", Operation::Stairs),
        ];
        if let Some(config) = &self.room_omissions {
            stages.push((
                "room_omissions",
                Operation::RoomOmissions {
                    config: config.clone(),
                },
            ));
        }
        if let Some(config) = &self.maze_rooms {
            if self.lighting.is_none() {
                return Err(fail("Maze rooms require an explicit lighting policy"));
            }
            stages.push((
                "maze_rooms",
                Operation::MazeRooms {
                    config: config.clone(),
                },
            ));
        }
        if let Some(lighting) = &self.lighting {
            stages.push((
                "lighting",
                Operation::RoomLighting {
                    darkness_roll: lighting.darkness_roll,
                    darkness_start: lighting.darkness_start,
                },
            ));
        }
        if let Some(config) = &self.secret_entrances {
            stages.push((
                "secret_entrances",
                Operation::SecretEntrances {
                    config: config.clone(),
                },
            ));
        }
        if let Some(config) = &self.hidden_traps {
            stages.push((
                "hidden_traps",
                Operation::HiddenTraps {
                    config: config.clone(),
                },
            ));
        }
        let recipe = Recipe {
            version: if self.maze_rooms.is_some() {
                5
            } else if self.room_omissions.is_some() {
                4
            } else if self.secret_entrances.is_some() || self.hidden_traps.is_some() {
                3
            } else if self.lighting.is_some() {
                2
            } else {
                1
            },
            stages: stages
                .into_iter()
                .map(|(id, operation)| Stage {
                    id: id.into(),
                    version: 1,
                    operation,
                })
                .collect(),
        };
        check(&recipe)?;
        Ok(recipe)
    }
}

/// Compiled execution plan; source authors use the grid_rooms preset.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub version: u32,
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    pub id: String,
    pub version: u32,
    pub operation: Operation,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Operation {
    StoneFill,
    GridPartition,
    Rooms {
        width: [u32; 2],
        height: [u32; 2],
    },
    ConnectedGraph {
        extra: [u32; 2],
    },
    Corridors,
    Stairs,
    RoomOmissions {
        config: RoomOmissions,
    },
    MazeRooms {
        config: MazeRooms,
    },
    RoomLighting {
        darkness_roll: u32,
        darkness_start: u32,
    },
    SecretEntrances {
        config: SecretEntrances,
    },
    HiddenTraps {
        config: HiddenTraps,
    },
}

fn fail(message: impl AsRef<str>) -> Failure {
    Failure::new(tor_protocol::ErrorCode::InvalidAction, message.as_ref())
}

pub(crate) fn check_group(group: &GenerationGroup) -> Result<(), Failure> {
    let [columns, rows] = group.grid;
    if !(1..=MAX_GRID_SIDE).contains(&columns) || !(1..=MAX_GRID_SIDE).contains(&rows) {
        return Err(fail(
            "Generation grid dimensions must each be between 1 and 8",
        ));
    }
    let count = columns
        .checked_mul(rows)
        .ok_or_else(|| fail("Generation grid member count overflows"))? as usize;
    if group.members.len() != count || group.members.iter().collect::<BTreeSet<_>>().len() != count
    {
        return Err(fail(format!(
            "Generation grid requires exactly {count} distinct region members"
        )));
    }
    Ok(())
}

pub(crate) fn check(recipe: &Recipe) -> Result<(), Failure> {
    let has_omissions = recipe
        .stages
        .get(6)
        .is_some_and(|stage| matches!(stage.operation, Operation::RoomOmissions { .. }));
    let maze_position = 6 + usize::from(has_omissions);
    let lighting_position = if recipe.version == 5 {
        maze_position + 1
    } else if recipe.version == 4 {
        7
    } else {
        6
    };
    if !((recipe.version == 1 && recipe.stages.len() == 6)
        || (recipe.version == 2 && recipe.stages.len() == 7)
        || (recipe.version == 3 && (7..=9).contains(&recipe.stages.len()))
        || (recipe.version == 4
            && (7..=10).contains(&recipe.stages.len())
            && matches!(recipe.stages[6].operation, Operation::RoomOmissions { .. }))
        || (recipe.version == 5
            && (8..=11).contains(&recipe.stages.len())
            && recipe
                .stages
                .get(maze_position)
                .is_some_and(|stage| matches!(stage.operation, Operation::MazeRooms { .. }))
            && recipe
                .stages
                .get(lighting_position)
                .is_some_and(|stage| matches!(stage.operation, Operation::RoomLighting { .. }))))
    {
        return Err(fail(
            "Recipe requires ordered geometry, optional omissions, optional dark mazes with lighting, secret entrances and traps",
        ));
    }
    let mut names = BTreeSet::new();
    let mut extra_order = 0;
    for (position, stage) in recipe.stages.iter().enumerate() {
        if stage.version != 1
            || stage.id.is_empty()
            || stage.id.len() > 80
            || !stage
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !names.insert(&stage.id)
        {
            return Err(fail("Invalid or duplicate recipe stage identity/version"));
        }
        let valid = match (&stage.operation, position) {
            (Operation::StoneFill, 0)
            | (Operation::GridPartition, 1)
            | (Operation::Corridors, 4)
            | (Operation::Stairs, 5) => true,
            (Operation::RoomOmissions { config }, 6) if (4..=5).contains(&recipe.version) => {
                config.count[0] <= config.count[1]
                    && config.count[1] < MAX_GRID_SIDE * MAX_GRID_SIDE
            }
            (Operation::MazeRooms { config }, n) if recipe.version == 5 && n == maze_position => {
                (1..=10_000).contains(&config.one_in)
            }
            (Operation::RoomLighting { darkness_roll, .. }, n) if n == lighting_position => {
                extra_order = 1;
                (1..=10_000).contains(darkness_roll)
            }
            (Operation::SecretEntrances { config }, n)
                if n >= 6 && (3..=5).contains(&recipe.version) && extra_order < 2 =>
            {
                extra_order = 2;
                config.chance_per_mille <= 1000
                    && config.maximum <= 64
                    && tor_world::Concealment { dc: config.dc }.valid()
            }
            (Operation::HiddenTraps { config }, n)
                if n >= 6 && (3..=5).contains(&recipe.version) && extra_order < 3 =>
            {
                extra_order = 3;
                config.count[0] <= config.count[1]
                    && config.count[1] <= 64
                    && config.hazards.as_ref().is_none_or(HazardTraps::valid)
                    && tor_world::Concealment { dc: config.dc }.valid()
                    && tor_world::TrapKind::Rust {
                        slot: config.rust_slot.clone(),
                        amount: config.rust_amount,
                    }
                    .valid()
                    && tor_world::TrapKind::SleepingGas {
                        condition: config.sleep_condition.clone(),
                        duration: config.sleep_duration,
                        exposure: config.sleep_exposure.clone(),
                    }
                    .valid()
            }
            (Operation::Rooms { width, height }, 2) => {
                width[0] >= 3
                    && width[0] <= width[1]
                    && width[1] <= 30
                    && height[0] >= 3
                    && height[0] <= height[1]
                    && height[1] <= 30
            }
            (Operation::ConnectedGraph { extra }, 3) => extra[0] <= extra[1] && extra[1] <= 4,
            _ => false,
        };
        if !valid {
            return Err(fail(format!(
                "Stage {}: invalid operation, parameters or order",
                stage.id
            )));
        }
    }
    Ok(())
}

/// Validate recipe parameters against one equal-sized slot without carving.
/// Procedural families use this while checking their compact descriptors.
pub(crate) fn check_layout(recipe: &Recipe, size: [i32; 3]) -> Result<(), Failure> {
    check(recipe)?;
    if !(5..=32).contains(&size[0]) || !(5..=32).contains(&size[1]) || size[2] != 2 {
        return Err(fail(
            "Grid slots need bounds of 5 to 32 cells and two cells of clearance",
        ));
    }
    let Operation::Rooms { width, height } = recipe.stages[2].operation else {
        unreachable!("checked recipe room stage")
    };
    if width[1] > (size[0] - 2) as u32 || height[1] > (size[1] - 2) as u32 {
        return Err(fail(
            "Room ranges exceed bounded slots with two-cell clearance",
        ));
    }
    Ok(())
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut n = self.0;
        n = (n ^ (n >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        n = (n ^ (n >> 27)).wrapping_mul(0x94d049bb133111eb);
        n ^ (n >> 31)
    }
    fn range(&mut self, low: i32, high: i32) -> i32 {
        low + (self.next() % (high - low + 1) as u64) as i32
    }
}

fn stream(
    seed: u64,
    identity: &str,
    group: &GenerationGroup,
    _recipe: &Recipe,
    stage: &Stage,
    key: &str,
) -> Result<Rng, Failure> {
    let bytes = serde_json::to_vec(&(
        "tor-group-recipe-v1",
        seed,
        identity,
        group.depth,
        // Geometry stages retain their version-1 stream when lighting is appended.
        1u32,
        stage,
        key,
    ))
    .map_err(|e| fail(e.to_string()))?;
    let hash = Sha256::digest(bytes);
    Ok(Rng(u64::from_le_bytes(
        hash[..8].try_into().expect("digest length"),
    )))
}

struct RecipeRun<'a> {
    identity: &'a str,
    group: &'a GenerationGroup,
    recipe: &'a Recipe,
    seed: u64,
}

struct Room {
    cells: Vec<(i32, i32)>,
    center: (i32, i32),
}

impl RecipeRun<'_> {
    fn rng(&self, stage: usize, key: &str) -> Result<Rng, Failure> {
        stream(
            self.seed,
            self.identity,
            self.group,
            self.recipe,
            &self.recipe.stages[stage],
            key,
        )
    }

    fn present_rooms(&self, definitions: &BTreeMap<u64, RegionDef>) -> Result<Vec<bool>, Failure> {
        let mut present = vec![true; self.group.members.len()];
        let Some((index, config)) =
            self.recipe
                .stages
                .iter()
                .enumerate()
                .find_map(|(index, stage)| {
                    if let Operation::RoomOmissions { config } = &stage.operation {
                        Some((index, config))
                    } else {
                        None
                    }
                })
        else {
            return Ok(present);
        };
        let mut eligible: Vec<_> = self
            .group
            .members
            .iter()
            .enumerate()
            .filter_map(|(slot, id)| {
                let def = &definitions[id];
                let protected = def
                    .generate
                    .as_ref()
                    .is_some_and(|g| !g.stair_anchors.is_empty())
                    || def.anchors.keys().any(|name| {
                        !def.generate
                            .as_ref()
                            .is_some_and(|g| g.boundary_anchors.contains(name))
                    });
                (!protected).then_some(slot)
            })
            .collect();
        // Keep one real room even when no authored endpoint protects a slot.
        let capacity = eligible.len().min(present.len() - 1) as u32;
        if config.count[0] > capacity {
            return Err(fail(
                "Room omission minimum exceeds unprotected slot capacity",
            ));
        }
        let mut rng = self.rng(index, "selection")?;
        let count =
            rng.range(config.count[0] as i32, config.count[1].min(capacity) as i32) as usize;
        // Bounded partial shuffle, without replacement or rejection sampling.
        for i in 0..count {
            let j = rng.range(i as i32, eligible.len() as i32 - 1) as usize;
            eligible.swap(i, j);
            present[eligible[i]] = false;
        }
        Ok(present)
    }

    fn room(&self, def: &RegionDef) -> Result<Room, Failure> {
        let Operation::Rooms { width, height } = self.recipe.stages[2].operation else {
            unreachable!()
        };
        let size = def.size;
        let mut rng = self.rng(2, &format!("{}/{size:?}", def.id))?;
        let w = rng.range(width[0] as i32, width[1] as i32);
        let h = rng.range(height[0] as i32, height[1] as i32);
        let boundary = def.generate.as_ref().map(|g| &g.boundary_anchors);
        let mut x_low = 1;
        let mut x_high = size[0] - w - 1;
        let mut y_low = 1;
        let mut y_high = size[1] - h - 1;
        for (_, at) in def
            .anchors
            .iter()
            .filter(|(name, _)| !boundary.is_some_and(|b| b.contains(name)))
        {
            if at[2] != 0
                || !(1..size[0] - 1).contains(&at[0])
                || !(1..size[1] - 1).contains(&at[1])
            {
                return Err(fail(format!(
                    "Region {}: Room entries must lie inside the slot margin on the floor plane",
                    def.id
                )));
            }
            x_low = x_low.max(at[0] - w + 1);
            x_high = x_high.min(at[0]);
            y_low = y_low.max(at[1] - h + 1);
            y_high = y_high.min(at[1]);
        }
        if x_low > x_high || y_low > y_high {
            return Err(fail(
                "Room cannot contain authored entries within its margin",
            ));
        }
        let x = rng.range(x_low, x_high);
        let y = rng.range(y_low, y_high);
        Ok(Room {
            cells: (y..y + h)
                .flat_map(|y| (x..x + w).map(move |x| (x, y)))
                .collect(),
            center: (x + w / 2, y + h / 2),
        })
    }

    fn stairs(&self, def: &mut RegionDef, cells: &[(i32, i32)]) -> Result<(), Failure> {
        let names = def
            .generate
            .as_ref()
            .map_or_else(Vec::new, |g| g.stair_anchors.clone());
        let mut occupied: BTreeSet<_> = def.anchors.values().copied().collect();
        for name in names {
            let candidates: Vec<_> = cells
                .iter()
                .map(|&(x, y)| [x, y, 0])
                .filter(|at| !occupied.contains(at))
                .collect();
            if candidates.is_empty() {
                return Err(fail("Insufficient room space for stair endpoints"));
            }
            let mut rng = self.rng(5, &format!("{}/{name}", def.id))?;
            let at = candidates[rng.range(0, candidates.len() as i32 - 1) as usize];
            occupied.insert(at);
            def.anchors.insert(name, at);
        }
        Ok(())
    }
}

fn validate_carving(open: &BTreeSet<(i32, i32)>, size: [i32; 3]) -> Result<(), Failure> {
    let start = open
        .first()
        .copied()
        .ok_or_else(|| fail("Floor has no carved cells"))?;
    if size[0] < 3
        || size[1] < 3
        || size[2] < 2
        || open
            .iter()
            .any(|&(x, y)| x <= 0 || y <= 0 || x >= size[0] - 1 || y >= size[1] - 1)
    {
        return Err(fail(
            "Carved floor must preserve its exterior and body clearance",
        ));
    }
    let mut visited = BTreeSet::from([start]);
    let mut pending = vec![start];
    while let Some((x, y)) = pending.pop() {
        for next in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
            if open.contains(&next) && visited.insert(next) {
                pending.push(next);
            }
        }
    }
    if visited.len() != open.len() {
        return Err(fail("Carved floor is disconnected"));
    }
    Ok(())
}

/// Prepare region definitions from a recipe without publishing simulation state.
/// Geometry and stable room interiors are produced together, before any gameplay
/// population. The room metadata is retained by prepared activation scopes.
pub struct GeneratedGroup {
    pub regions: BTreeMap<u64, RegionDef>,
    pub rooms: Vec<tor_simulation::floor_activation::PreparedRoomGroup>,
    pub(crate) spatial: GeneratedSpatialProvenance,
}

/// Ephemeral labels from the SAME completed generation pass as regions/rooms.
/// No saved graph, RNG replay, population change or public authored format.
#[derive(Default)]
pub(crate) struct GeneratedSpatialProvenance {
    pub ordinary_room_ids: BTreeSet<u32>,
    pub doorway_owners: BTreeMap<tor_world::Location, u32>,
    pub closure_faces: BTreeSet<tor_world::Location>,
}

fn retain_spatial_provenance(
    group: &GenerationGroup,
    size: [i32; 3],
    present: &[bool],
    maze_slots: &[bool],
    room_cells: &[Vec<(i32, i32)>],
    open: &BTreeSet<(i32, i32)>,
) -> Result<GeneratedSpatialProvenance, Failure> {
    if present.len() != group.members.len()
        || maze_slots.len() != present.len()
        || room_cells.len() != present.len()
    {
        return Err(fail("Spatial generation labels must match actual slots"));
    }
    let columns = group.grid[0] as usize;
    let mut result = GeneratedSpatialProvenance::default();
    let mut ordinary = BTreeMap::new();
    for (slot, cells) in room_cells.iter().enumerate() {
        let room = u32::try_from(slot)
            .ok()
            .and_then(|n| n.checked_add(1))
            .ok_or_else(|| fail("Room identity overflow"))?;
        if !present[slot] || maze_slots[slot] {
            continue;
        }
        result.ordinary_room_ids.insert(room);
        let ox = (slot % columns) as i32 * size[0];
        let oy = (slot / columns) as i32 * size[1];
        for &(x, y) in cells {
            let p = (x + ox, y + oy);
            if !open.contains(&p) || ordinary.insert(p, room).is_some() {
                return Err(fail("Conflicting final ordinary-room cells"));
            }
        }
    }
    // ALL ordinary open mouths, independently of reserved anchors, width,
    // closure maximum/chance/eligibility. This consumes no RNG and does not
    // change the existing secret candidate set or candidate iteration order.
    let mut owners = BTreeMap::new();
    for (&(x, y), &room) in &ordinary {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let mouth = (x + dx, y + dy);
            if !open.contains(&mouth) || ordinary.contains_key(&mouth) {
                continue;
            }
            if owners
                .insert(mouth, room)
                .is_some_and(|prior| prior != room)
            {
                return Err(fail("Generated doorway has competing room owners"));
            }
        }
    }
    for ((x, y), room) in owners {
        if x < 0 || y < 0 {
            return Err(fail("Generated doorway outside group"));
        }
        let slot = (y / size[1]) as usize * columns + (x / size[0]) as usize;
        let region = *group
            .members
            .get(slot)
            .ok_or_else(|| fail("Generated doorway outside group"))?;
        for z in 0..2 {
            result.doorway_owners.insert(
                tor_world::Location {
                    region: tor_world::RegionId(region),
                    position: tor_world::Position {
                        x: x % size[0],
                        y: y % size[1],
                        z,
                    },
                },
                room,
            );
        }
    }
    Ok(result)
}

#[cfg(test)]
pub fn materialize(
    identity: &str,
    group: &GenerationGroup,
    recipe: &Recipe,
    definitions: Vec<RegionDef>,
    seed: u64,
) -> Result<GeneratedGroup, Failure> {
    materialize_with_feature_bases(
        identity,
        group,
        recipe,
        definitions,
        seed,
        FeatureBases { doors: 1, traps: 1 },
    )
}

pub fn materialize_with_feature_bases(
    identity: &str,
    group: &GenerationGroup,
    recipe: &Recipe,
    definitions: Vec<RegionDef>,
    seed: u64,
    feature_bases: FeatureBases,
) -> Result<GeneratedGroup, Failure> {
    check(recipe)?;
    check_group(group)?;
    let count = group.members.len();
    if definitions.len() != count {
        return Err(fail(
            "Generation group must have one definition for every member",
        ));
    }
    let mut definitions: BTreeMap<_, _> = definitions.into_iter().map(|d| (d.id, d)).collect();
    if definitions.len() != count || group.members.iter().any(|id| !definitions.contains_key(id)) {
        return Err(fail(
            "Generation group definitions must exactly match its distinct members",
        ));
    }
    let size = definitions[&group.members[0]].size;
    if definitions.values().any(|d| d.size != size) {
        return Err(fail("Grid members need equal bounds"));
    }
    check_layout(recipe, size)?;
    let columns = group.grid[0] as usize;
    let rows = group.grid[1] as usize;
    let width = size[0]
        .checked_mul(group.grid[0] as i32)
        .ok_or_else(|| fail("Generation grid width overflows"))?;
    let height = size[1]
        .checked_mul(group.grid[1] as i32)
        .ok_or_else(|| fail("Generation grid height overflows"))?;
    let run = RecipeRun {
        identity,
        group,
        recipe,
        seed,
    };
    let mut open = BTreeSet::new();
    let present = run.present_rooms(&definitions)?;
    let mut centers = Vec::new();
    let mut room_cells = Vec::new();
    for (slot, id) in group.members.iter().enumerate() {
        let def = &definitions[id];
        let room = run.room(def)?;
        let offset = (
            (slot % columns) as i32 * size[0],
            (slot / columns) as i32 * size[1],
        );
        let cells: Vec<_> = room
            .cells
            .iter()
            .map(|&(x, y)| (x + offset.0, y + offset.1))
            .collect();
        if present[slot] {
            open.extend(cells.iter().copied());
            room_cells.push(room.cells);
        } else {
            room_cells.push(Vec::new());
        }
        centers.push((room.center.0 + offset.0, room.center.1 + offset.1));
    }
    let mut edges = Vec::new();
    for slot in 0..count {
        if slot % columns + 1 < columns {
            edges.push((slot, slot + 1));
        }
        if slot / columns + 1 < rows {
            edges.push((slot, slot + columns));
        }
    }
    let mut rng = stream(seed, identity, group, recipe, &recipe.stages[3], "graph")?;
    for i in (1..edges.len()).rev() {
        let j = rng.range(0, i as i32) as usize;
        edges.swap(i, j);
    }
    let mut labels: Vec<_> = (0..count).collect();
    let mut connected = Vec::new();
    let mut unused = Vec::new();
    for (a, b) in edges {
        if labels[a] == labels[b] {
            unused.push((a, b));
            continue;
        }
        let old = labels[b];
        let new = labels[a];
        for label in &mut labels {
            if *label == old {
                *label = new;
            }
        }
        connected.push((a, b));
    }
    let Operation::ConnectedGraph { extra } = recipe.stages[3].operation else {
        unreachable!()
    };
    connected.extend(
        unused
            .into_iter()
            .take(rng.range(extra[0] as i32, extra[1] as i32) as usize),
    );
    let mut rng = stream(
        seed,
        identity,
        group,
        recipe,
        &recipe.stages[4],
        "corridors",
    )?;
    let mut corridor_cells = BTreeSet::new();
    for (a, b) in connected {
        let (mut x, mut y) = centers[a];
        let (tx, ty) = centers[b];
        let horizontal_first = rng.next() & 1 == 0;
        corridor_cells.insert((x, y));
        for horizontal in [horizontal_first, !horizontal_first] {
            while if horizontal { x != tx } else { y != ty } {
                if horizontal {
                    x += (tx - x).signum();
                } else {
                    y += (ty - y).signum();
                }
                corridor_cells.insert((x, y));
            }
        }
    }
    open.extend(corridor_cells.iter().copied());
    let mut dark_rooms = vec![false; count];
    validate_carving(&open, [width, height, size[2]])?;
    for (slot, id) in group.members.iter().enumerate() {
        let def = definitions.get_mut(id).expect("checked member");
        let offset = (
            (slot % columns) as i32 * size[0],
            (slot / columns) as i32 * size[1],
        );
        for z in 0..size[2] {
            for y in 0..size[1] {
                for x in 0..size[0] {
                    if z >= 2 || !open.contains(&(x + offset.0, y + offset.1)) {
                        def.walls.push([x, y, z]);
                    }
                }
            }
        }
        run.stairs(def, &room_cells[slot])?;
        if let Some(stage) = recipe
            .stages
            .iter()
            .find(|stage| matches!(stage.operation, Operation::RoomLighting { .. }))
        {
            let mut rng = stream(seed, identity, group, recipe, stage, &format!("room/{id}"))?;
            def.lit = false;
            def.lighting.clear();
            let Operation::RoomLighting {
                darkness_roll,
                darkness_start,
            } = stage.operation
            else {
                unreachable!()
            };
            let lit = (rng.range(0, darkness_roll as i32 - 1) as u32)
                >= group.depth.saturating_sub(darkness_start);
            dark_rooms[slot] = !lit;
            if lit {
                let mut illuminated = BTreeSet::new();
                for &(x, y) in &room_cells[slot] {
                    for z in 0..2 {
                        illuminated.insert([x, y, z]);
                        for [dx, dy, dz] in [
                            [-1, 0, 0],
                            [1, 0, 0],
                            [0, -1, 0],
                            [0, 1, 0],
                            [0, 0, -1],
                            [0, 0, 1],
                        ] {
                            let at = [x + dx, y + dy, z + dz];
                            if at[2] < 0
                                || at[2] >= 2
                                || !open.contains(&(at[0] + offset.0, at[1] + offset.1))
                            {
                                illuminated.insert(at);
                            }
                        }
                    }
                }
                def.lighting.extend(
                    illuminated
                        .into_iter()
                        .map(|at| crate::scenario_package::CellLight { at, lit: true }),
                );
            }
        }
        let walls: BTreeSet<_> = def.walls.iter().copied().collect();
        for name in def.anchors.keys().filter(|name| {
            !def.generate
                .as_ref()
                .is_some_and(|g| g.boundary_anchors.contains(name))
        }) {
            let [x, y, z] = def.anchors[name];
            if z != 0 || walls.contains(&[x, y, 0]) || walls.contains(&[x, y, 1]) {
                return Err(fail(format!(
                    "Region {}: anchor {name} lacks body clearance",
                    def.id
                )));
            }
        }
    }
    let maze_slots = maze::carve(
        &run,
        &mut open,
        &corridor_cells,
        &mut room_cells,
        &dark_rooms,
        &mut definitions,
        size,
    )?;
    let mut spatial =
        retain_spatial_provenance(group, size, &present, &maze_slots, &room_cells, &open)?;
    place_concealed_features(
        &run,
        &open,
        &room_cells,
        &mut definitions,
        size,
        feature_bases,
        &maze_slots,
    )?;
    // Retain the already-placed physical closures, without repeating decisions.
    // A maze/passage closure without ordinary-room ownership stays a closure,
    // but does not fabricate a Room illumination owner.
    for (region, def) in &definitions {
        for door in &def.doors {
            for dz in 0..i32::from(door.height) {
                let at = tor_world::Location {
                    region: tor_world::RegionId(*region),
                    position: tor_world::Position {
                        x: door.at[0],
                        y: door.at[1],
                        z: door.at[2]
                            .checked_add(dz)
                            .ok_or_else(|| fail("Door face coordinate overflow"))?,
                    },
                };
                if spatial.doorway_owners.contains_key(&at) {
                    spatial.closure_faces.insert(at);
                }
            }
        }
    }
    // Opening every discoverable closure restores exactly this connected carving;
    // features never erase a corridor, stair or room from the required route graph.
    validate_carving(&open, [width, height, size[2]])?;
    let rooms = group
        .members
        .iter()
        .enumerate()
        .filter(|(slot, _)| present[*slot])
        .map(|(slot, id)| {
            let mut cells: Vec<_> = room_cells[slot]
                .iter()
                .map(|&(x, y)| tor_world::Location {
                    region: tor_world::RegionId(*id),
                    position: tor_world::Position { x, y, z: 0 },
                })
                .collect();
            cells.sort();
            tor_simulation::floor_activation::PreparedRoomGroup {
                id: slot as u32 + 1,
                cells,
            }
        })
        .collect();
    Ok(GeneratedGroup {
        regions: definitions,
        rooms,
        spatial,
    })
}

#[cfg(test)]
#[path = "generation_recipe_tests.rs"]
mod tests;

#[path = "generation_features.rs"]
mod features;
use features::place_concealed_features;
pub use features::{FeatureBases, FEATURE_IDENTITY_STRIDE};

#[path = "generation_maze.rs"]
mod maze;
