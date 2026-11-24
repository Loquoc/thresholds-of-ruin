//! Bounded player-facing level names. A level is a presentation grouping, not
//! a room, streaming scope, activation boundary, or new piece of saved state.

use std::{borrow::Cow, collections::BTreeMap};

use serde::{Deserialize, Serialize};

use crate::procedural_world::{FamilyCatalog, WorldFamily};
use crate::scenario_package::RegionIndex;

const DEPTH: &str = "{depth}";

/// Named authored levels, memberships by canonical decimal region ID, and
/// display templates for procedural families. IDs are never sent to clients.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LevelLabels {
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "crate::scenario_package::spatial_authoring::unique_map"
    )]
    pub names: BTreeMap<String, String>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "crate::scenario_package::spatial_authoring::unique_map"
    )]
    pub regions: BTreeMap<String, String>,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "crate::scenario_package::spatial_authoring::unique_map"
    )]
    pub procedural: BTreeMap<String, String>,
}

fn depth_label(template: &str, depth: u32) -> Option<String> {
    let (prefix, suffix) = template.split_once(DEPTH)?;
    if prefix.contains(['{', '}']) || suffix.contains(['{', '}']) {
        return None;
    }
    let rendered = format!("{prefix}{depth}{suffix}");
    tor_protocol::valid_level_name(&rendered).then_some(rendered)
}

impl LevelLabels {
    pub(crate) fn valid(
        &self,
        index: &RegionIndex,
        families: &BTreeMap<String, WorldFamily>,
    ) -> bool {
        let catalog = FamilyCatalog::new(families, 0);
        self.names.len() <= self.regions.len()
            && self
                .names
                .values()
                .all(|name| tor_protocol::valid_level_name(name))
            && self.regions.iter().all(|(region, name)| {
                self.names.contains_key(name)
                    && region.parse::<u64>().is_ok_and(|id| {
                        id > 0
                            && region == &id.to_string()
                            && index
                                .regions
                                .binary_search_by_key(&id, |row| row.id)
                                .is_ok()
                            && catalog.find(id).is_none()
                    })
            })
            && self.procedural.iter().all(|(family, template)| {
                families.contains_key(family)
                    && depth_label(template, 1).is_some()
                    && depth_label(template, u32::MAX).is_some()
            })
    }

    pub(crate) fn at(
        &self,
        region: u64,
        families: &BTreeMap<String, WorldFamily>,
        seed: u64,
    ) -> Option<Cow<'_, str>> {
        if !self.regions.is_empty() {
            if let Some(id) = self.regions.get(&region.to_string()) {
                return self.names.get(id).map(|name| Cow::Borrowed(name.as_str()));
            }
        }
        if self.procedural.is_empty() {
            return None;
        }
        let (family, world) = FamilyCatalog::new(families, seed).find(region)?;
        let (depth, _) = world.address(region)?;
        depth_label(self.procedural.get(family)?, depth).map(Cow::Owned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scenario_package::IndexedRegion;

    fn index(ids: &[u64]) -> RegionIndex {
        let rows: Vec<_> =
            ids.iter()
                .map(|id| {
                    serde_json::from_value::<IndexedRegion>(serde_json::json!({
                "id": id, "file": "", "hash": "", "compiled_hash": "", "name": "", "zone": null,
                "size": [1, 1, 1], "chamber": false, "anchors": {}, "portals": [],
                "actors": [], "items": [], "doors": [], "traps": []
            })).unwrap()
                })
                .collect();
        RegionIndex { regions: rows }
    }

    #[test]
    fn authored_memberships_share_a_label_without_relabeling_unassigned_rooms() {
        let labels: LevelLabels = toml::from_str(
            r#"
            [names]
            old-mine = "The Old Mine"
            keep = "The Keep"
            [regions]
            1 = "old-mine"
            2 = "old-mine"
            3 = "keep"
        "#,
        )
        .unwrap();
        let families = BTreeMap::new();
        assert!(labels.valid(&index(&[1, 2, 3, 4]), &families));
        assert!(!labels.valid(&index(&[1, 3, 4]), &families));
        assert_eq!(labels.at(1, &families, 42).as_deref(), Some("The Old Mine"));
        assert_eq!(labels.at(2, &families, 42).as_deref(), Some("The Old Mine"));
        assert_eq!(labels.at(3, &families, 42).as_deref(), Some("The Keep"));
        assert_eq!(labels.at(4, &families, 42), None);
    }

    #[test]
    fn rogue_label_matches_every_slot_on_each_depth() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios/rogue-exploration");
        let package = crate::scenario_package::read_package(&root).unwrap();
        let labels = package.manifest.levels.unwrap();
        let families = package.manifest.procedural_worlds;
        let family = &families["dungeon"];
        assert!(labels.valid(&RegionIndex::default(), &families));
        for depth in [1, 2, 26] {
            let expected = format!("Dungeon Level {depth}");
            for member in family.group("dungeon", 42, depth).unwrap().members {
                assert_eq!(
                    labels.at(member, &families, 42).as_deref(),
                    Some(expected.as_str())
                );
            }
        }
    }

    #[test]
    fn depth_templates_are_bounded_and_have_exactly_one_placeholder() {
        for invalid in [
            "Level",
            "{depth}/{depth}",
            "{other} {depth}",
            " {depth}",
            "{depth}\n",
        ] {
            assert_eq!(depth_label(invalid, 2), None, "{invalid:?}");
        }
        assert_eq!(
            depth_label("Dungeon Level {depth}", 2).as_deref(),
            Some("Dungeon Level 2")
        );
        assert_eq!(
            depth_label(&format!("{}{{depth}}", "x".repeat(71)), u32::MAX),
            None
        );
    }
}
