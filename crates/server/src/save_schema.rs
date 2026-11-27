//! Save-owned schemas and explicit mappings from backend values.
//! Action DTOs own numeric targets and stored variants independently of transport
//! and simulation enums. Remaining shared identity/metadata values use remote
//! derives so wire serializers never define the persisted representation.
use serde::{Deserialize, Serialize};
use tor_protocol as wire;
#[path = "save_advancement.rs"]
mod advancement;
#[path = "save_attachments.rs"]
pub(crate) mod attachments;

#[path = "save_scenario.rs"]
pub(crate) mod scenario;

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::ActorId", transparent)]
pub(crate) struct ActorId(pub u64);

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Direction {
    North,
    East,
    South,
    West,
    NorthEast,
    SouthEast,
    SouthWest,
    NorthWest,
    Up,
    Down,
}

pub(crate) mod optional_direction {
    use super::*;
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &Option<crate::actions::Direction>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.map(Direction::from).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<crate::actions::Direction>, D::Error> {
        Option::<Direction>::deserialize(deserializer).map(|value| value.map(Into::into))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "crate::journal::IntentionChange",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum IntentionChange {
    Suspended,
    Resumed,
    Cancelled,
    Escaped {
        #[serde(with = "Direction")]
        direction: crate::actions::Direction,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Action {
    Escape {
        hold: tor_simulation::HoldId,
    },
    #[serde(deserialize_with = "advancement::read_growth_source")]
    ChooseGrowthSource {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::growth_source")]
        source: tor_simulation::earned_growth::GrowthSource,
    },
    #[serde(deserialize_with = "advancement::read_intellect_expertise")]
    ChooseIntellectExpertise {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::intellect_expertise")]
        choice: tor_simulation::player_advancement::IntellectExpertiseOperation,
    },
    #[serde(deserialize_with = "advancement::read_purchase")]
    Purchase {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::purchase")]
        purchase: tor_simulation::player_advancement::PurchaseOperation,
    },
    #[serde(deserialize_with = "advancement::read_respec")]
    Respec {
        #[serde(with = "advancement::basis")]
        basis: Box<tor_simulation::player_advancement::AdvancementBasis>,
        #[serde(with = "advancement::replacement")]
        replacement: tor_simulation::player_respec::ReplacementOperation,
        supplies: SupplySelection,
    },
    UseAbility {
        #[serde(with = "ability_id")]
        id: tor_simulation::content_ids::AbilityId,
        invocation: AbilityInvocation,
        supplies: SupplySelection,
    },
    Equip {
        item: u64,
        slot: u16,
    },
    Unequip {
        item: u64,
    },
    Ignite {
        item: u64,
    },
    Extinguish {
        item: u64,
    },
    Drink {
        item: u64,
    },
    Read {
        item: u64,
    },
    Zap {
        item: u64,
        aim: DeviceAim,
    },
    Identify {
        choice: tor_simulation::IntentionId,
        #[serde(deserialize_with = "required_option")]
        item: Option<u64>,
    },
    Eat {
        item: u64,
    },
    ExamineTrap {
        trap: u64,
    },
    SetDoor {
        door: u64,
        open: bool,
    },
    Move {
        direction: Direction,
    },
    Take {
        item: u64,
        quantity: Option<u64>,
    },
    TouchApparentObject {
        subject: u64,
        #[serde(deserialize_with = "required_option")]
        quantity: Option<u64>,
    },
    Drop {
        item: u64,
        quantity: Option<u64>,
    },
    Wait,
}

fn required_option<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

/// Save-owned spelling: later simulation or transport serde changes cannot
/// rewrite the historical identity domain attached to an accepted preparation.
pub(crate) mod actor_reference_kind {
    use serde::{Deserialize, Serialize};
    use tor_simulation::perception::ActorReferenceKind;

    pub(crate) fn serialize<S: serde::Serializer>(
        kind: &ActorReferenceKind,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match kind {
            ActorReferenceKind::Creature => "creature",
            ActorReferenceKind::Contact => "contact",
        }
        .serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<ActorReferenceKind, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "creature" => Ok(ActorReferenceKind::Creature),
            "contact" => Ok(ActorReferenceKind::Contact),
            _ => Err(serde::de::Error::custom(
                "invalid saved actor reference kind",
            )),
        }
    }
}

/// A saved content reference, independent of frontend display data.
pub(crate) mod ability_id {
    use serde::{Deserialize, Serialize};
    pub(crate) fn serialize<S: serde::Serializer>(
        id: &tor_simulation::content_ids::AbilityId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        id.as_str().serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<tor_simulation::content_ids::AbilityId, D::Error> {
        tor_simulation::content_ids::AbilityId::deserialize(deserializer)
    }
}

impl Direction {
    pub(crate) fn serialize<S: serde::Serializer>(
        direction: &crate::actions::Direction,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Serialize::serialize(&Self::from(*direction), serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<crate::actions::Direction, D::Error> {
        <Self as Deserialize>::deserialize(deserializer).map(Into::into)
    }
}
impl From<crate::actions::Direction> for Direction {
    fn from(direction: crate::actions::Direction) -> Self {
        match direction {
            crate::actions::Direction::North => Self::North,
            crate::actions::Direction::East => Self::East,
            crate::actions::Direction::South => Self::South,
            crate::actions::Direction::West => Self::West,
            crate::actions::Direction::NorthEast => Self::NorthEast,
            crate::actions::Direction::SouthEast => Self::SouthEast,
            crate::actions::Direction::SouthWest => Self::SouthWest,
            crate::actions::Direction::NorthWest => Self::NorthWest,
            crate::actions::Direction::Up => Self::Up,
            crate::actions::Direction::Down => Self::Down,
        }
    }
}
impl From<Direction> for crate::actions::Direction {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::North => Self::North,
            Direction::East => Self::East,
            Direction::South => Self::South,
            Direction::West => Self::West,
            Direction::NorthEast => Self::NorthEast,
            Direction::SouthEast => Self::SouthEast,
            Direction::SouthWest => Self::SouthWest,
            Direction::NorthWest => Self::NorthWest,
            Direction::Up => Self::Up,
            Direction::Down => Self::Down,
        }
    }
}

/// Save-owned named records do not inherit serde's positional tuple alias.
pub(crate) fn named_map<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct Named<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Named<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a named stored record")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Named(std::marker::PhantomData))
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SupplySelection {
    Carried,
    Explicit { items: Vec<SelectedSupply> },
}
impl<'de> Deserialize<'de> for SupplySelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Carried {},
            Explicit {
                #[serde(deserialize_with = "read_supply_items")]
                items: Vec<SelectedSupply>,
            },
        }
        Ok(match Fields::deserialize(deserializer)? {
            Fields::Carried {} => Self::Carried,
            Fields::Explicit { items } => Self::Explicit { items },
        })
    }
}
#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SelectedSupply {
    cost_ordinal: u8,
    item: u64,
}
impl<'de> Deserialize<'de> for SelectedSupply {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            cost_ordinal: u8,
            item: u64,
        }
        let value: Fields = named_map(deserializer)?;
        Ok(Self {
            cost_ordinal: value.cost_ordinal,
            item: value.item,
        })
    }
}
fn read_supply_items<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<SelectedSupply>, D::Error> {
    struct Rows;
    impl<'de> serde::de::Visitor<'de> for Rows {
        type Value = Vec<SelectedSupply>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("1..32 named supply rows")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut access: A,
        ) -> Result<Self::Value, A::Error> {
            use serde::de::Error;
            if access.size_hint().is_some_and(|n| n > 32) {
                return Err(A::Error::custom("too many supply rows"));
            }
            let mut rows = Vec::new();
            while rows.len() < 32 {
                let Some(row) = access.next_element::<SelectedSupply>()? else {
                    return if rows.is_empty() {
                        Err(A::Error::custom("empty supply rows"))
                    } else {
                        Ok(rows)
                    };
                };
                if row.cost_ordinal >= 8
                    || row.item == 0
                    || row.item == u64::MAX
                    || rows.last().is_some_and(|prior: &SelectedSupply| {
                        (prior.cost_ordinal, prior.item) >= (row.cost_ordinal, row.item)
                    })
                    || rows.iter().any(|prior| prior.item == row.item)
                {
                    return Err(A::Error::custom("invalid supply row"));
                }
                rows.push(row);
            }
            if rows.is_empty() || access.next_element::<serde::de::IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("invalid supply row count"));
            }
            Ok(rows)
        }
    }
    deserializer.deserialize_seq(Rows)
}
impl From<&crate::actions::SupplySelection> for SupplySelection {
    fn from(value: &crate::actions::SupplySelection) -> Self {
        match value {
            crate::actions::SupplySelection::Carried => Self::Carried,
            crate::actions::SupplySelection::Explicit { items } => Self::Explicit {
                items: items
                    .iter()
                    .map(|row| SelectedSupply {
                        cost_ordinal: row.cost_ordinal,
                        item: row.item,
                    })
                    .collect(),
            },
        }
    }
}
impl From<SupplySelection> for crate::actions::SupplySelection {
    fn from(value: SupplySelection) -> Self {
        match value {
            SupplySelection::Carried => Self::Carried,
            SupplySelection::Explicit { items } => Self::Explicit {
                items: items
                    .into_iter()
                    .map(|row| crate::actions::SelectedSupply {
                        cost_ordinal: row.cost_ordinal,
                        item: row.item,
                    })
                    .collect(),
            },
        }
    }
}

impl From<&crate::actions::Action> for Action {
    fn from(action: &crate::actions::Action) -> Self {
        use crate::actions::Action as Backend;
        match action {
            Backend::Escape { hold } => Self::Escape { hold: *hold },
            Backend::ChooseGrowthSource { basis, source } => Self::ChooseGrowthSource {
                basis: basis.clone(),
                source: source.clone(),
            },
            Backend::ChooseIntellectExpertise { basis, choice } => Self::ChooseIntellectExpertise {
                basis: basis.clone(),
                choice: choice.clone(),
            },
            Backend::Purchase { basis, purchase } => Self::Purchase {
                basis: basis.clone(),
                purchase: purchase.clone(),
            },
            Backend::Respec {
                basis,
                replacement,
                supplies,
            } => Self::Respec {
                basis: basis.clone(),
                replacement: replacement.clone(),
                supplies: SupplySelection::from(supplies),
            },
            Backend::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id: id.clone(),
                invocation: (*invocation).into(),
                supplies: SupplySelection::from(supplies),
            },
            Backend::ExamineTrap { trap } => Self::ExamineTrap { trap: *trap },
            Backend::SetDoor { door, open } => Self::SetDoor {
                door: *door,
                open: *open,
            },
            Backend::Move { direction } => Self::Move {
                direction: (*direction).into(),
            },
            Backend::Take { item, quantity } => Self::Take {
                item: *item,
                quantity: *quantity,
            },
            Backend::TouchApparentObject { subject, quantity } => Self::TouchApparentObject {
                subject: subject.0,
                quantity: *quantity,
            },
            Backend::Drop { item, quantity } => Self::Drop {
                item: *item,
                quantity: *quantity,
            },
            Backend::Equip { item, slot } => Self::Equip {
                item: *item,
                slot: *slot,
            },
            Backend::Unequip { item } => Self::Unequip { item: *item },
            Backend::Ignite { item } => Self::Ignite { item: *item },
            Backend::Extinguish { item } => Self::Extinguish { item: *item },
            Backend::Drink { item } => Self::Drink { item: *item },
            Backend::Read { item } => Self::Read { item: *item },
            Backend::Zap { item, aim } => Self::Zap {
                item: *item,
                aim: (*aim).into(),
            },
            Backend::Identify { choice, item } => Self::Identify {
                choice: *choice,
                item: *item,
            },
            Backend::Eat { item } => Self::Eat { item: *item },
            Backend::Wait => Self::Wait,
        }
    }
}
impl From<Action> for crate::actions::Action {
    fn from(action: Action) -> Self {
        match action {
            Action::Escape { hold } => Self::Escape { hold },
            Action::ChooseGrowthSource { basis, source } => {
                Self::ChooseGrowthSource { basis, source }
            }
            Action::ChooseIntellectExpertise { basis, choice } => {
                Self::ChooseIntellectExpertise { basis, choice }
            }
            Action::Purchase { basis, purchase } => Self::Purchase { basis, purchase },
            Action::Respec {
                basis,
                replacement,
                supplies,
            } => Self::Respec {
                basis,
                replacement,
                supplies: supplies.into(),
            },
            Action::UseAbility {
                id,
                invocation,
                supplies,
            } => Self::UseAbility {
                id,
                invocation: invocation.into(),
                supplies: supplies.into(),
            },
            Action::ExamineTrap { trap } => Self::ExamineTrap { trap },
            Action::SetDoor { door, open } => Self::SetDoor { door, open },
            Action::Move { direction } => Self::Move {
                direction: direction.into(),
            },
            Action::Take { item, quantity } => Self::Take { item, quantity },
            Action::TouchApparentObject { subject, quantity } => Self::TouchApparentObject {
                subject: tor_simulation::ActorId(subject),
                quantity,
            },
            Action::Drop { item, quantity } => Self::Drop { item, quantity },
            Action::Equip { item, slot } => Self::Equip { item, slot },
            Action::Unequip { item } => Self::Unequip { item },
            Action::Ignite { item } => Self::Ignite { item },
            Action::Extinguish { item } => Self::Extinguish { item },
            Action::Drink { item } => Self::Drink { item },
            Action::Read { item } => Self::Read { item },
            Action::Zap { item, aim } => Self::Zap {
                item,
                aim: aim.into(),
            },
            Action::Identify { choice, item } => Self::Identify { choice, item },
            Action::Eat { item } => Self::Eat { item },
            Action::Wait => Self::Wait,
        }
    }
}
impl Action {
    pub(crate) fn serialize<S: serde::Serializer>(
        action: &crate::actions::Action,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        Serialize::serialize(&Self::from(action), serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<crate::actions::Action, D::Error> {
        <Self as Deserialize>::deserialize(deserializer).map(Into::into)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "wire::Anchor",
    tag = "type",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Anchor {
    State {
        #[serde(with = "snapshot_id")]
        snapshot: wire::SnapshotId,
    },
    Entry {
        #[serde(with = "EntryId")]
        id: wire::EntryId,
    },
}

/// Snapshot capabilities keep their current stored string shape and validation.
/// These constraints belong to the save reader, independently of wire serde or
/// the protocol's future identity-validation policy.
pub(crate) mod snapshot_id {
    use super::*;

    pub(crate) fn serialize<S: serde::Serializer>(
        value: &wire::SnapshotId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.0.serialize(serializer)
    }

    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<wire::SnapshotId, D::Error> {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() || value.len() > 128 || value.chars().any(char::is_control) {
            return Err(serde::de::Error::custom("invalid snapshot identity"));
        }
        Ok(wire::SnapshotId(value))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::EntryId", transparent)]
pub(crate) struct EntryId(pub String);

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::BranchId", transparent)]
pub(crate) struct BranchId(pub String);

#[derive(Serialize, Deserialize)]
#[serde(
    remote = "wire::Author",
    tag = "type",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub(crate) enum Author {
    User { user: String },
    Frontend { user: String, component: String },
    Backend { component: String },
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::Audience", rename_all = "snake_case")]
pub(crate) enum Audience {
    Private,
    Actor,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::AnnotationCategory", rename_all = "snake_case")]
pub(crate) enum AnnotationCategory {
    Note,
    Bookmark,
    Explanation,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "wire::ClientSource", rename_all = "snake_case")]
pub(crate) enum ClientSource {
    User,
    Frontend,
}

/// Optional retained-boundary identities retain their stored string/null shape.
pub(crate) mod optional_entry {
    use super::*;
    #[derive(Serialize)]
    #[serde(transparent)]
    struct Borrowed<'a>(#[serde(with = "EntryId")] &'a wire::EntryId);
    #[derive(Deserialize)]
    #[serde(transparent)]
    struct Owned(#[serde(with = "EntryId")] wire::EntryId);
    pub(crate) fn serialize<S: serde::Serializer>(
        value: &Option<wire::EntryId>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        value.as_ref().map(Borrowed).serialize(serializer)
    }
    pub(crate) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<wire::EntryId>, D::Error> {
        Option::<Owned>::deserialize(deserializer).map(|value| value.map(|value| value.0))
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AbilityInvocation {
    aim: AbilityAim,
    weapon: WeaponSelection,
}
impl<'de> Deserialize<'de> for AbilityInvocation {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            aim: AbilityAim,
            weapon: WeaponSelection,
        }
        let Fields { aim, weapon } = named_map(deserializer)?;
        if matches!(
            aim,
            AbilityAim::DirectionalProjectile { .. } | AbilityAim::CellProjectile { .. }
        ) && !matches!(weapon, WeaponSelection::Default)
        {
            return Err(serde::de::Error::custom(
                "projectile delivery is the sole weapon selector",
            ));
        }
        Ok(Self { aim, weapon })
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum WeaponSelection {
    Default,
    Equipped { item: u64, socket: u16 },
}
impl<'de> Deserialize<'de> for WeaponSelection {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Default {},
            Equipped { item: u64, socket: u16 },
        }
        Ok(match named_map(deserializer)? {
            Fields::Default {} => Self::Default,
            Fields::Equipped { item, socket } => {
                if usize::from(socket) >= tor_simulation::limits::EQUIPMENT_SOCKETS
                    || item == 0
                    || item == u64::MAX
                {
                    return Err(serde::de::Error::custom(
                        "invalid equipped weapon selection",
                    ));
                }
                Self::Equipped { item, socket }
            }
        })
    }
}
impl From<crate::actions::AbilityInvocation> for AbilityInvocation {
    fn from(value: crate::actions::AbilityInvocation) -> Self {
        Self {
            aim: value.aim.into(),
            weapon: match value.weapon {
                crate::actions::WeaponSelection::Default => WeaponSelection::Default,
                crate::actions::WeaponSelection::Equipped { item, socket } => {
                    WeaponSelection::Equipped { item, socket }
                }
            },
        }
    }
}
impl From<AbilityInvocation> for crate::actions::AbilityInvocation {
    fn from(value: AbilityInvocation) -> Self {
        Self {
            aim: value.aim.into(),
            weapon: match value.weapon {
                WeaponSelection::Default => crate::actions::WeaponSelection::Default,
                WeaponSelection::Equipped { item, socket } => {
                    crate::actions::WeaponSelection::Equipped { item, socket }
                }
            },
        }
    }
}
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum AbilityAim {
    ActorTarget {
        target: u64,
    },
    ContactTarget {
        target: u64,
    },
    SelfStep {
        direction: Direction,
    },
    CellRay {
        #[serde(deserialize_with = "read_cell_offset")]
        offset: [i32; 3],
    },
    CellProjectile {
        #[serde(deserialize_with = "read_cell_offset")]
        offset: [i32; 3],
        delivery: ProjectileDeliverySelection,
    },
    DirectionalRay {
        direction: Direction,
    },
    DirectionalProjectile {
        direction: Direction,
        delivery: ProjectileDeliverySelection,
    },
}
impl<'de> Deserialize<'de> for AbilityAim {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Unit-enum deserialization alone also accepts an externally tagged
        // object. Selected directions are the documented string token only.
        fn direction_token<'de, D: serde::Deserializer<'de>>(
            deserializer: D,
        ) -> Result<Direction, D::Error> {
            let token = String::deserialize(deserializer)?;
            <Direction as Deserialize>::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(token),
            )
        }
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            ActorTarget {
                target: u64,
            },
            ContactTarget {
                target: u64,
            },
            SelfStep {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
            CellRay {
                #[serde(deserialize_with = "read_cell_offset")]
                offset: [i32; 3],
            },
            CellProjectile {
                #[serde(deserialize_with = "read_cell_offset")]
                offset: [i32; 3],
                delivery: ProjectileDeliverySelection,
            },
            DirectionalRay {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
            },
            DirectionalProjectile {
                #[serde(deserialize_with = "direction_token")]
                direction: Direction,
                delivery: ProjectileDeliverySelection,
            },
        }
        Ok(match named_map(deserializer)? {
            Fields::ActorTarget { target } => Self::ActorTarget { target },
            Fields::ContactTarget { target } => Self::ContactTarget { target },
            Fields::SelfStep { direction } => Self::SelfStep { direction },
            Fields::DirectionalRay { direction } => Self::DirectionalRay { direction },
            Fields::CellRay { offset } => Self::CellRay { offset },
            Fields::CellProjectile { offset, delivery } => {
                Self::CellProjectile { offset, delivery }
            }
            Fields::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction,
                delivery,
            },
        })
    }
}
impl From<crate::actions::AbilityAim> for AbilityAim {
    fn from(value: crate::actions::AbilityAim) -> Self {
        match value {
            crate::actions::AbilityAim::ActorTarget { target } => {
                Self::ActorTarget { target: target.0 }
            }
            crate::actions::AbilityAim::ContactTarget { target } => {
                Self::ContactTarget { target: target.0 }
            }
            crate::actions::AbilityAim::CellRay { offset } => Self::CellRay { offset },
            crate::actions::AbilityAim::CellProjectile { offset, delivery } => {
                Self::CellProjectile {
                    offset,
                    delivery: delivery.into(),
                }
            }
            crate::actions::AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction: direction.into(),
                delivery: delivery.into(),
            },
            crate::actions::AbilityAim::SelfStep { direction } => Self::SelfStep {
                direction: direction.into(),
            },
            crate::actions::AbilityAim::DirectionalRay { direction } => Self::DirectionalRay {
                direction: direction.into(),
            },
        }
    }
}
impl From<AbilityAim> for crate::actions::AbilityAim {
    fn from(value: AbilityAim) -> Self {
        match value {
            AbilityAim::ActorTarget { target } => Self::ActorTarget {
                target: tor_simulation::ActorId(target),
            },
            AbilityAim::ContactTarget { target } => Self::ContactTarget {
                target: tor_simulation::ActorId(target),
            },
            AbilityAim::CellRay { offset } => Self::CellRay { offset },
            AbilityAim::CellProjectile { offset, delivery } => Self::CellProjectile {
                offset,
                delivery: delivery.into(),
            },
            AbilityAim::DirectionalProjectile {
                direction,
                delivery,
            } => Self::DirectionalProjectile {
                direction: direction.into(),
                delivery: delivery.into(),
            },
            AbilityAim::SelfStep { direction } => Self::SelfStep {
                direction: direction.into(),
            },
            AbilityAim::DirectionalRay { direction } => Self::DirectionalRay {
                direction: direction.into(),
            },
        }
    }
}

fn valid_projectile_item(item: u64) -> bool {
    item != 0 && item != u64::MAX
}
fn valid_projectile_socket(socket: u16) -> bool {
    usize::from(socket) < tor_simulation::limits::EQUIPMENT_SOCKETS
}

/// The selected equipped launcher, with no implicit primary-weapon fallback.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct EquippedProjectileWeapon {
    pub item: u64,
    pub socket: u16,
}
impl EquippedProjectileWeapon {
    pub(crate) fn valid(self) -> bool {
        valid_projectile_item(self.item) && valid_projectile_socket(self.socket)
    }
}
impl<'de> Deserialize<'de> for EquippedProjectileWeapon {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            item: u64,
            socket: u16,
        }
        let fields: Fields = named_map(d)?;
        let result = Self {
            item: fields.item,
            socket: fields.socket,
        };
        if !result.valid() {
            return Err(serde::de::Error::custom(
                "invalid projectile weapon selector",
            ));
        }
        Ok(result)
    }
}

/// Throw an equipped item or a carried stack using the selected socket.
/// The socket remains explicit for either source mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ThrownSourceSelection {
    Equipped { item: u64, socket: u16 },
    Carried { item: u64, socket: u16 },
}
impl ThrownSourceSelection {
    pub(crate) fn valid(self) -> bool {
        let (Self::Equipped { item, socket } | Self::Carried { item, socket }) = self;
        valid_projectile_item(item) && valid_projectile_socket(socket)
    }
}
impl<'de> Deserialize<'de> for ThrownSourceSelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Equipped { item: u64, socket: u16 },
            Carried { item: u64, socket: u16 },
        }
        let result = match named_map(d)? {
            Fields::Equipped { item, socket } => Self::Equipped { item, socket },
            Fields::Carried { item, socket } => Self::Carried { item, socket },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom("invalid thrown source selector"));
        }
        Ok(result)
    }
}

/// A closed delivery choice for exactly one projectile. Launch always names
/// both the equipped launcher and the carried ammunition stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ProjectileDeliverySelection {
    Throw {
        source: ThrownSourceSelection,
    },
    Launch {
        launcher: EquippedProjectileWeapon,
        ammunition: u64,
    },
}
impl ProjectileDeliverySelection {
    pub(crate) fn valid(self) -> bool {
        match self {
            Self::Throw { source } => source.valid(),
            Self::Launch {
                launcher,
                ammunition,
            } => {
                launcher.valid() && valid_projectile_item(ammunition) && launcher.item != ammunition
            }
        }
    }
}
impl<'de> Deserialize<'de> for ProjectileDeliverySelection {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Throw {
                source: ThrownSourceSelection,
            },
            Launch {
                launcher: EquippedProjectileWeapon,
                ammunition: u64,
            },
        }
        let result = match named_map(d)? {
            Fields::Throw { source } => Self::Throw { source },
            Fields::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher,
                ammunition,
            },
        };
        if !result.valid() {
            return Err(serde::de::Error::custom(
                "invalid projectile delivery selector",
            ));
        }
        Ok(result)
    }
}

impl From<crate::actions::ProjectileDeliverySelection> for ProjectileDeliverySelection {
    fn from(value: crate::actions::ProjectileDeliverySelection) -> Self {
        match value {
            crate::actions::ProjectileDeliverySelection::Throw { source } => Self::Throw {
                source: match source {
                    crate::actions::ThrownSourceSelection::Equipped { item, socket } => {
                        ThrownSourceSelection::Equipped { item, socket }
                    }
                    crate::actions::ThrownSourceSelection::Carried { item, socket } => {
                        ThrownSourceSelection::Carried { item, socket }
                    }
                },
            },
            crate::actions::ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher: EquippedProjectileWeapon {
                    item: launcher.item,
                    socket: launcher.socket,
                },
                ammunition,
            },
        }
    }
}

impl From<ProjectileDeliverySelection> for crate::actions::ProjectileDeliverySelection {
    fn from(value: ProjectileDeliverySelection) -> Self {
        match value {
            ProjectileDeliverySelection::Throw { source } => Self::Throw {
                source: match source {
                    ThrownSourceSelection::Equipped { item, socket } => {
                        crate::actions::ThrownSourceSelection::Equipped { item, socket }
                    }
                    ThrownSourceSelection::Carried { item, socket } => {
                        crate::actions::ThrownSourceSelection::Carried { item, socket }
                    }
                },
            },
            ProjectileDeliverySelection::Launch {
                launcher,
                ammunition,
            } => Self::Launch {
                launcher: crate::actions::EquippedProjectileWeapon {
                    item: launcher.item,
                    socket: launcher.socket,
                },
                ammunition,
            },
        }
    }
}

#[cfg(test)]
#[path = "save_schema_tests.rs"]
mod tests;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum DeviceAim {
    Direction {
        direction: Direction,
    },
    Cell {
        #[serde(deserialize_with = "read_cell_offset")]
        offset: [i32; 3],
    },
    SelfTarget {},
}
impl<'de> Deserialize<'de> for DeviceAim {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
        enum Fields {
            Direction {
                direction: Direction,
            },
            Cell {
                #[serde(deserialize_with = "read_cell_offset")]
                offset: [i32; 3],
            },
            SelfTarget {},
        }
        Ok(match named_map(d)? {
            Fields::Direction { direction } => Self::Direction { direction },
            Fields::SelfTarget {} => Self::SelfTarget {},
            Fields::Cell { offset } => Self::Cell { offset },
        })
    }
}
impl From<crate::actions::DeviceAim> for DeviceAim {
    fn from(value: crate::actions::DeviceAim) -> Self {
        match value {
            crate::actions::DeviceAim::Direction { direction } => Self::Direction {
                direction: direction.into(),
            },
            crate::actions::DeviceAim::SelfTarget {} => Self::SelfTarget {},
            crate::actions::DeviceAim::Cell { offset } => Self::Cell { offset },
        }
    }
}
impl From<DeviceAim> for crate::actions::DeviceAim {
    fn from(value: DeviceAim) -> Self {
        match value {
            DeviceAim::Direction { direction } => Self::Direction {
                direction: direction.into(),
            },
            DeviceAim::SelfTarget {} => Self::SelfTarget {},
            DeviceAim::Cell { offset } => Self::Cell { offset },
        }
    }
}

fn read_cell_offset<'de, D: serde::Deserializer<'de>>(d: D) -> Result<[i32; 3], D::Error> {
    let offset = <[i32; 3] as Deserialize>::deserialize(d)?;
    if offset == [0, 0, 0] {
        return Err(serde::de::Error::custom(
            "cell aim must have a nonzero offset",
        ));
    }
    Ok(offset)
}
