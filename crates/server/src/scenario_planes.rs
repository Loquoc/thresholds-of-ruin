//! Coexisting planes in scenario packages.
//!
//! A package declares planes in its manifest:
//!
//! ```toml
//! [planes.ethereal]
//! mysterious = "a grey, sourceless mist in which walls are only suggestions"
//! revealed = "the Border Ethereal"
//! lore_dc = 14
//! gravity = [0, 0, 0]
//! movement_modes = [{ mode = "ether_glide", check = { skill = "discipline", dc = 15 } }]
//! regions = [40]                         # authored regions on this plane
//! overlaps = [{ from = 2, min = [0, 0, 0], max = [5, 4, 1], to = 40, origin = [0, 0, 0] }]
//! derive = [{ from = 3, id = 1003 }]     # built from material region 3
//! ```
//!
//! A derived region is built from its source when first needed: the same
//! interior size and anchors, empty terrain, the plane's gravity and light,
//! the source's portals whose destinations are also derived on this plane, and
//! an identity overlap over `volume` (the whole interior by default). Once
//! built it is an ordinary, independent region. Region files are unchanged.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tor_simulation::planes::{ChannelSpec, GlideRule, PlaneSpec};
use tor_simulation::Game;
use tor_world::{Overlap, PlaneId, Position, RegionId};

use super::{fail, require, Package, RegionDef};
use crate::Failure;

/// Most regions in one overlap group, as a package capacity limit.
pub const MAX_OVERLAP_GROUP: usize = 16;
/// The movement modes a plane can grant.
const ETHER_GLIDE: &str = "ether_glide";

fn default_lit() -> bool {
    true
}
fn unit_scale() -> u8 {
    1
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlaneDecl {
    /// What an observer that does not know the plane is told.
    pub mysterious: String,
    /// What an observer that knows the plane is told.
    pub revealed: String,
    /// The Lore difficulty to recognize the plane on arrival.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lore_dc: Option<i32>,
    /// Gravity of derived regions on this plane; authored regions keep their own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gravity: Option<[i32; 3]>,
    /// Whether derived regions are lit.
    #[serde(default = "default_lit")]
    pub lit: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub movement_modes: Vec<MovementModeDecl>,
    /// What observers on this plane perceive of other planes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<ChannelDecl>,
    /// Species whose members on this plane strike across its overlaps at
    /// what its channels show them. Strikes only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_plane_reach: Vec<String>,
    /// Authored regions on this plane.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub regions: Vec<u64>,
    /// Authored overlaps from another plane's region to one of this plane's.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub overlaps: Vec<OverlapDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derive: Vec<DeriveDecl>,
    /// Procedural families whose every region is derived onto this plane. A
    /// family's block reserves one shadow span per deriving plane.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derive_families: Vec<String>,
}

/// A one-way perception channel. `perception = "mirror"` is the only mode: an
/// ordinary eye pass on plane `to` from the counterpart of the observer's eye.
/// `light = "fixed"` is the only light rule: the other plane's lighting is
/// ignored within `range`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelDecl {
    pub to: String,
    pub range: u8,
    #[serde(default = "mirror")]
    pub perception: String,
    #[serde(default = "fixed")]
    pub light: String,
}
fn mirror() -> String {
    "mirror".into()
}
fn fixed() -> String {
    "fixed".into()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MovementModeDecl {
    pub mode: String,
    pub check: CheckDecl,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckDecl {
    pub skill: tor_simulation::attributes::Skill,
    pub dc: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlapDecl {
    pub from: u64,
    pub min: [i32; 3],
    pub max: [i32; 3],
    pub to: u64,
    pub origin: [i32; 3],
    #[serde(default)]
    pub rotation: u8,
    #[serde(default = "unit_scale")]
    pub scale: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeriveDecl {
    pub from: u64,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<VolumeDecl>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VolumeDecl {
    pub min: [i32; 3],
    pub max: [i32; 3],
}

fn position([x, y, z]: [i32; 3]) -> Position {
    Position { x, y, z }
}

fn description(text: &str) -> bool {
    !text.is_empty() && text.len() <= 240 && !text.chars().any(char::is_control)
}

impl Package {
    /// Plane identities, in name order from 1. The material plane is 0.
    pub(crate) fn plane_ids(&self) -> BTreeMap<&str, PlaneId> {
        self.manifest
            .planes
            .keys()
            .enumerate()
            .map(|(index, name)| (name.as_str(), PlaneId(index as u16 + 1)))
            .collect()
    }

    /// The plane and declaration a derived region comes from.
    pub(crate) fn derived_region(&self, region: u64) -> Option<(&str, &DeriveDecl)> {
        self.manifest.planes.iter().find_map(|(name, decl)| {
            decl.derive
                .iter()
                .find(|derive| derive.id == region)
                .map(|derive| (name.as_str(), derive))
        })
    }

    /// A derived region's definition, built from its source's.
    pub(crate) fn derived_definition(
        &self,
        source: &RegionDef,
        plane: &str,
        derive: &DeriveDecl,
    ) -> Result<RegionDef, Failure> {
        let decl = &self.manifest.planes[plane];
        let derived: BTreeMap<u64, u64> = decl
            .derive
            .iter()
            .map(|other| (other.from, other.id))
            .collect();
        let portals = source
            .portals
            .iter()
            .filter_map(|portal| {
                let (region, anchor) = portal.to.split_once('/')?;
                let target = derived.get(&region.parse::<u64>().ok()?)?;
                let mut copy = portal.clone();
                copy.to = format!("{target}/{anchor}");
                Some(copy)
            })
            .collect();
        Ok(RegionDef {
            lit: decl.lit,
            lighting: Vec::new(),
            id: derive.id,
            name: source.name.clone(),
            size: source.size,
            chamber: false,
            zone: source.zone.clone(),
            gravity: decl.gravity.or(source.gravity),
            gravity_overrides: Vec::new(),
            anchors: source.anchors.clone(),
            walls: Vec::new(),
            openings: Vec::new(),
            places: Vec::new(),
            portals,
            doors: Vec::new(),
            traps: Vec::new(),
            trapdoor_destination: None,
            items: Vec::new(),
            actors: Vec::new(),
            generate: None,
        })
    }

    /// The overlap a derived region has with its source.
    fn derived_overlap(&self, seed: u64, derive: &DeriveDecl) -> Result<Overlap, Failure> {
        let size = self.structural_metadata(seed, derive.from)?.size;
        let volume = derive.volume.unwrap_or(VolumeDecl {
            min: [0, 0, 0],
            max: [size[0] - 1, size[1] - 1, size[2] - 1],
        });
        Ok(Overlap {
            from: RegionId(derive.from),
            min: position(volume.min),
            max: position(volume.max),
            to: RegionId(derive.id),
            origin: position(volume.min),
            rotation: 0,
            scale: 1,
        })
    }

    /// Every overlap the package declares, authored and derived.
    pub(crate) fn package_overlaps(&self, seed: u64) -> Result<Vec<Overlap>, Failure> {
        let mut overlaps = Vec::new();
        for decl in self.manifest.planes.values() {
            for overlap in &decl.overlaps {
                overlaps.push(Overlap {
                    from: RegionId(overlap.from),
                    min: position(overlap.min),
                    max: position(overlap.max),
                    to: RegionId(overlap.to),
                    origin: position(overlap.origin),
                    rotation: overlap.rotation,
                    scale: overlap.scale,
                });
            }
            for derive in &decl.derive {
                overlaps.push(self.derived_overlap(seed, derive)?);
            }
        }
        Ok(overlaps)
    }

    /// Region membership of every non-material plane.
    fn plane_regions(&self) -> BTreeMap<u64, PlaneId> {
        let ids = self.plane_ids();
        let mut regions = BTreeMap::new();
        for (name, decl) in &self.manifest.planes {
            let plane = ids[name.as_str()];
            for region in decl
                .regions
                .iter()
                .copied()
                .chain(decl.derive.iter().map(|d| d.id))
            {
                regions.insert(region, plane);
            }
        }
        regions
    }

    /// Validate the manifest's planes against the package's regions.
    pub(crate) fn check_planes(&self) -> Result<(), Failure> {
        if self.manifest.planes.is_empty() {
            return Ok(());
        }
        require(
            self.manifest.planes.len() <= tor_simulation::planes::MAX_PLANES,
            "Too many planes",
        )?;
        let mut members = BTreeSet::new();
        for (name, decl) in &self.manifest.planes {
            let context = |message: &str| format!("Plane {name}: {message}");
            require(
                name != "material"
                    && !name.is_empty()
                    && name.len() <= 64
                    && name.bytes().all(|b| {
                        b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-'
                    }),
                context("invalid name"),
            )?;
            require(
                description(&decl.mysterious) && description(&decl.revealed),
                context("descriptions must be 1-240 bytes without control characters"),
            )?;
            require(
                decl.lore_dc
                    .is_none_or(|dc| (0..=tor_simulation::planes::MAX_PLANE_DC).contains(&dc)),
                context("lore_dc is out of range"),
            )?;
            require(
                decl.movement_modes.len() <= 1,
                context("at most one movement mode"),
            )?;
            for mode in &decl.movement_modes {
                require(
                    mode.mode == ETHER_GLIDE,
                    context("the only movement mode is ether_glide"),
                )?;
                require(
                    (0..=tor_simulation::planes::MAX_PLANE_DC).contains(&mode.check.dc),
                    context("movement check dc is out of range"),
                )?;
            }
            for family in &decl.derive_families {
                require(
                    self.manifest.procedural_worlds.contains_key(family),
                    context(&format!("unknown procedural family {family}")),
                )?;
            }
            for species in &decl.cross_plane_reach {
                require(
                    tor_simulation::content_ids::SpeciesId::new(species).is_ok(),
                    context(&format!("invalid species {species}")),
                )?;
            }
            for channel in &decl.channels {
                require(
                    channel.to != *name
                        && (channel.to == "material"
                            || self.manifest.planes.contains_key(&channel.to)),
                    context("a channel must lead to another declared plane or material"),
                )?;
                require(
                    (1..=tor_simulation::planes::MAX_CHANNEL_RANGE).contains(&channel.range),
                    context("channel range must be 1-16"),
                )?;
                require(
                    channel.perception == "mirror" && channel.light == "fixed",
                    context("the only channel mode is perception = \"mirror\", light = \"fixed\""),
                )?;
            }
            for derive in &decl.derive {
                require(
                    self.index.region(derive.id).is_none()
                        && self.procedural_family(derive.id).is_none()
                        && derive.id != 0,
                    context(&format!(
                        "derived region {} collides with another region",
                        derive.id
                    )),
                )?;
            }
        }
        for (name, decl) in &self.manifest.planes {
            let context = |message: &str| format!("Plane {name}: {message}");
            for derive in &decl.derive {
                require(
                    self.derived_region(derive.from).is_none(),
                    context("a derived region cannot be derived from another"),
                )?;
                require(
                    self.index.region(derive.from).is_some(),
                    context(&format!(
                        "derive source {} must be an authored region",
                        derive.from
                    )),
                )?;
            }
            for region in decl.regions.iter().chain(decl.derive.iter().map(|d| &d.id)) {
                require(
                    members.insert(*region),
                    context(&format!("region {region} is on more than one plane")),
                )?;
            }
        }
        // Build the topology once, as a game would, to validate every overlap.
        let mut world =
            tor_world::World::new(vec![], vec![]).map_err(|e| fail(format!("{e:?}")))?;
        for (region, plane) in self.plane_regions() {
            world
                .set_region_plane(RegionId(region), plane)
                .map_err(|_| fail(format!("Region {region}: invalid plane")))?;
        }
        for overlap in self.package_overlaps(0)? {
            for (region, low, high) in [
                (overlap.from, overlap.min, overlap.max),
                (
                    overlap.to,
                    overlap.image().map_or(overlap.origin, |i| i.0),
                    overlap.image().map_or(overlap.origin, |i| i.1),
                ),
            ] {
                let size = self.structural_metadata(0, region.0)?.size;
                let inside = |p: Position| {
                    (0..size[0]).contains(&p.x)
                        && (0..size[1]).contains(&p.y)
                        && (0..size[2]).contains(&p.z)
                };
                require(
                    inside(low) && inside(high),
                    format!(
                        "Overlap with region {}: volume outside the region",
                        region.0
                    ),
                )?;
            }
            world.add_overlap(overlap).map_err(|_| {
                fail(format!(
                    "Overlap {} -> {}: invalid or conflicting volume",
                    overlap.from.0, overlap.to.0
                ))
            })?;
        }
        for group in tor_world::overlap_groups(world.overlaps()) {
            require(
                group.len() <= MAX_OVERLAP_GROUP,
                format!("An overlap group has more than {MAX_OVERLAP_GROUP} regions"),
            )?;
        }
        Ok(())
    }

    /// Install plane rules, membership and overlaps on a new game.
    pub(crate) fn install_planes(&self, game: &mut Game, seed: u64) -> Result<(), Failure> {
        if self.manifest.planes.is_empty() {
            return Ok(());
        }
        let ids = self.plane_ids();
        let specs = self
            .manifest
            .planes
            .iter()
            .map(|(name, decl)| {
                (
                    ids[name.as_str()],
                    PlaneSpec {
                        name: name.clone(),
                        mysterious: decl.mysterious.clone(),
                        revealed: decl.revealed.clone(),
                        glide: decl.movement_modes.first().map(|mode| GlideRule {
                            skill: mode.check.skill,
                            dc: mode.check.dc,
                        }),
                        channels: decl
                            .channels
                            .iter()
                            .map(|channel| ChannelSpec {
                                to: if channel.to == "material" {
                                    PlaneId::MATERIAL
                                } else {
                                    ids[channel.to.as_str()]
                                },
                                range: channel.range,
                            })
                            .collect(),
                        cross_plane_reach: decl
                            .cross_plane_reach
                            .iter()
                            .filter_map(|species| {
                                tor_simulation::content_ids::SpeciesId::new(species).ok()
                            })
                            .collect(),
                    },
                )
            })
            .collect();
        game.install_planes(specs)
            .map_err(|error| fail(format!("Planes: {error:?}")))?;
        for (region, plane) in self.plane_regions() {
            game.set_region_plane(RegionId(region), plane)
                .map_err(|error| fail(format!("Region {region} plane: {error:?}")))?;
        }
        for overlap in self.package_overlaps(seed)? {
            game.add_overlap(overlap)
                .map_err(|error| fail(format!("Overlap: {error:?}")))?;
        }
        for range in self.package_overlap_ranges()? {
            game.add_overlap_range(range)
                .map_err(|error| fail(format!("Overlap range: {error:?}")))?;
        }
        Ok(())
    }

    /// The definition behind a derived region, or `None` for other regions.
    pub(crate) fn derived_structural_definition(
        &self,
        seed: u64,
        region: u64,
    ) -> Option<Result<Arc<RegionDef>, Failure>> {
        if let Some((plane, source)) = self.family_shadow(region) {
            let derive = DeriveDecl {
                from: source,
                id: region,
                volume: None,
            };
            return Some((|| {
                let source = self.structural_definition(seed, source)?;
                Ok(Arc::new(self.derived_definition(&source, plane, &derive)?))
            })());
        }
        let (plane, derive) = self.derived_region(region)?;
        Some((|| {
            let source = self.structural_definition(seed, derive.from)?;
            Ok(Arc::new(self.derived_definition(&source, plane, derive)?))
        })())
    }
}

/// One procedural family derived onto one plane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FamilyShadow<'a> {
    pub plane: &'a str,
    pub base: u64,
    pub span: u64,
    /// The shadow span's first region.
    pub first: u64,
    pub max: Position,
}

/// How many planes derive each family.
pub(crate) fn family_shadow_counts(planes: &BTreeMap<String, PlaneDecl>) -> BTreeMap<String, u64> {
    let mut counts = BTreeMap::new();
    for decl in planes.values() {
        for family in &decl.derive_families {
            *counts.entry(family.clone()).or_insert(0) += 1;
        }
    }
    counts
}

impl Package {
    /// Every family derived onto a plane, with its shadow span.
    pub(crate) fn family_shadows(&self) -> Vec<FamilyShadow<'_>> {
        let mut shadows = Vec::new();
        let mut index: BTreeMap<&str, u64> = BTreeMap::new();
        for (plane, decl) in &self.manifest.planes {
            for family_id in &decl.derive_families {
                let Some(family) = self.manifest.procedural_worlds.get(family_id) else {
                    continue;
                };
                let Some(span) = crate::procedural_world::family_block_size(family) else {
                    continue;
                };
                let k = index.entry(family_id.as_str()).or_insert(0);
                *k += 1;
                let Some(first) = span
                    .checked_mul(*k)
                    .and_then(|offset| family.base_region.checked_add(offset))
                else {
                    continue;
                };
                shadows.push(FamilyShadow {
                    plane,
                    base: family.base_region,
                    span,
                    first,
                    max: position([family.size[0] - 1, family.size[1] - 1, family.size[2] - 1]),
                });
            }
        }
        shadows
    }

    /// The plane and material source of a family shadow region.
    pub(crate) fn family_shadow(&self, region: u64) -> Option<(&str, u64)> {
        self.family_shadows().into_iter().find_map(|shadow| {
            let offset = region.checked_sub(shadow.first)?;
            (offset < shadow.span).then(|| (shadow.plane, shadow.base + offset))
        })
    }

    /// Overlap ranges for every family derived onto a plane.
    pub(crate) fn package_overlap_ranges(&self) -> Result<Vec<tor_world::OverlapRange>, Failure> {
        let ids = self.plane_ids();
        Ok(self
            .family_shadows()
            .into_iter()
            .map(|shadow| tor_world::OverlapRange {
                from: RegionId(shadow.base),
                to: RegionId(shadow.first),
                count: shadow.span,
                max: shadow.max,
                plane: ids[shadow.plane],
            })
            .collect())
    }
}
