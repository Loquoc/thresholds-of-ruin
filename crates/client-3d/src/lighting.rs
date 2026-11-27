//! Illumination is a disclosed server fact, separate from visual awareness.
use super::*;
use bevy::pbr::NotShadowCaster;
#[cfg(test)]
mod tests;

/// Renderer treatment of a scene root. Descendants inherit it, but only
/// `Lit` permits decorative light sources. Dark perception is still current.
#[derive(Component, Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Appearance {
    Lit,
    Unlit,
    Remembered,
    Undisclosed,
}

impl Appearance {
    pub(super) fn terrain(seen: bool, lit: bool) -> Self {
        match (seen, lit) {
            (true, true) => Self::Lit,
            (true, false) => Self::Unlit,
            (false, _) => Self::Remembered,
        }
    }
}

/// Per-body visual opacity for an unclaimed historical corpse.
#[derive(Component, Clone, Copy)]
pub(super) struct CorpseOpacity(pub f32);

fn at(view: &View, position: Position) -> Appearance {
    view.visible
        .iter()
        .find(|cell| cell.position == position)
        .map_or(Appearance::Undisclosed, |cell| {
            Appearance::terrain(true, cell.lit)
        })
}

pub(super) fn is_lit(view: &View, position: Position) -> bool {
    at(view, position) == Appearance::Lit
}

/// Actors and loose items may be disclosed independently of the terrain.
pub(super) fn disclosed(view: &View, position: Position) -> Appearance {
    match at(view, position) {
        Appearance::Lit => Appearance::Lit,
        _ => Appearance::Unlit,
    }
}

fn scope(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<&Appearance>,
    dead: &Query<&creatures::Snuffed>,
) -> (Appearance, bool, Entity) {
    let mut snuffed = false;
    loop {
        snuffed |= dead.contains(entity);
        if let Ok(lit) = roots.get(entity) {
            return (*lit, snuffed, entity);
        }
        let Ok(parent) = parents.get(entity) else {
            // No disclosed source cell, so never invent a light source.
            return (Appearance::Undisclosed, snuffed, entity);
        };
        entity = parent.parent();
    }
}

/// Moving cosmetic effects inherit only the cell they currently occupy in the
/// disclosed chart. They cannot light an unknown or dark trajectory segment.
#[allow(clippy::type_complexity)]
pub(super) fn effects(
    mut commands: Commands,
    game: Res<Game>,
    effects: Query<
        (Entity, &Transform, Option<&Appearance>),
        Or<(With<creatures::Bolt>, With<creatures::Fade>)>,
    >,
) {
    for (entity, transform, previous) in &effects {
        let p = transform.translation;
        let position = Position {
            x: p.x.round() as i32 - game.offset.x,
            y: p.z.round() as i32 - game.offset.y,
            z: (p.y + 0.5).floor() as i32 - game.offset.z,
        };
        let appearance = game
            .view
            .as_ref()
            .map_or(Appearance::Undisclosed, |view| at(view, position));
        if previous != Some(&appearance) {
            commands.entity(entity).insert(appearance);
        }
    }
}

pub(super) trait ShadeMaterial: Material + Clone {
    fn base(&self) -> &StandardMaterial;
    fn base_mut(&mut self) -> &mut StandardMaterial;

    fn suppress_emission(&mut self) {
        self.base_mut().emissive = LinearRgba::BLACK;
    }
}

impl ShadeMaterial for StandardMaterial {
    fn base(&self) -> &StandardMaterial {
        self
    }

    fn base_mut(&mut self) -> &mut StandardMaterial {
        self
    }
}

impl ShadeMaterial for crate::surface::ModelMaterial {
    fn base(&self) -> &StandardMaterial {
        &self.base
    }

    fn base_mut(&mut self) -> &mut StandardMaterial {
        &mut self.base
    }

    fn suppress_emission(&mut self) {
        self.base.emissive = LinearRgba::BLACK;
        // The surface shader can add rim, sheen or wrap on top of base
        // emission. Leave grain, roughness and sculpt lighting intact.
        self.extension.params.finish.x = 0.0;
    }
}

pub(super) struct MaterialShadeState<M: ShadeMaterial> {
    meshes: HashMap<Entity, MeshShade<M>>,
    dark: Option<Handle<M>>,
    readable: HashMap<(AssetId<M>, bool), Handle<M>>,
    fades: HashMap<(Entity, AssetId<M>), FadeMaterial<M>>,
}

impl<M: ShadeMaterial> Default for MaterialShadeState<M> {
    fn default() -> Self {
        Self {
            meshes: HashMap::default(),
            dark: None,
            readable: HashMap::new(),
            fades: HashMap::default(),
        }
    }
}

struct MeshShade<M: Material> {
    source: Handle<M>,
    applied: AssetId<M>,
    suppressed_shadow: bool,
}

struct FadeMaterial<M: Material> {
    handle: Handle<M>,
    source_alpha: f32,
    opacity: f32,
}

impl<M: ShadeMaterial> MaterialShadeState<M> {
    fn readable(
        &mut self,
        assets: &mut Assets<M>,
        source: &Handle<M>,
        remembered: bool,
    ) -> Option<Handle<M>> {
        use std::collections::hash_map::Entry;
        let slot = match self.readable.entry((source.id(), remembered)) {
            Entry::Occupied(slot) => return Some(slot.get().clone()),
            Entry::Vacant(slot) => slot,
        };
        let mut copy = assets.get(source)?.clone();
        copy.suppress_emission();
        let base = copy.base_mut();
        let mut tint = base.base_color.to_linear();
        // Emissive recipes must not shine without real light. Keep their
        // geometry and albedo, and let the scene's studio fill reveal them.
        base.unlit = remembered;
        if remembered {
            // The terrain mesh has already converted vertex pigments to gray.
            let value = (tint.red * 0.2126 + tint.green * 0.7152 + tint.blue * 0.0722) * 0.46;
            tint.red = value;
            tint.green = value;
            tint.blue = value;
        } else {
            tint.red *= 0.86;
            tint.green *= 0.95;
            tint.blue *= 1.02;
        }
        base.base_color = tint.into();
        Some(slot.insert(assets.add(copy)).clone())
    }

    fn faded(
        &mut self,
        assets: &mut Assets<M>,
        source: &Handle<M>,
        root: Entity,
        opacity: f32,
    ) -> Option<Handle<M>> {
        use std::collections::hash_map::Entry;
        match self.fades.entry((root, source.id())) {
            Entry::Occupied(mut slot) => {
                let fade = slot.get_mut();
                if fade.opacity != opacity {
                    assets
                        .get_mut(&fade.handle)?
                        .base_mut()
                        .base_color
                        .set_alpha(fade.source_alpha * opacity);
                    fade.opacity = opacity;
                }
                Some(fade.handle.clone())
            }
            Entry::Vacant(slot) => {
                let mut copy = assets.get(source)?.clone();
                let source_alpha = copy.base().base_color.alpha();
                copy.base_mut().base_color.set_alpha(source_alpha * opacity);
                // Recipe transparency still contributes to the dissolve.
                copy.base_mut().alpha_mode = AlphaMode::Blend;
                let handle = assets.add(copy);
                slot.insert(FadeMaterial {
                    handle: handle.clone(),
                    source_alpha,
                    opacity,
                });
                Some(handle)
            }
        }
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn shade_with<M: ShadeMaterial>(
    commands: &mut Commands,
    assets: &mut Assets<M>,
    parents: &Query<&ChildOf>,
    roots: &Query<&Appearance>,
    changed_roots: &Query<(), Changed<Appearance>>,
    removed_materials: &mut RemovedComponents<MeshMaterial3d<M>>,
    dead: &Query<&creatures::Snuffed>,
    dead_changed: bool,
    fading: &Query<&CorpseOpacity>,
    changed_fades: &Query<Entity, Changed<CorpseOpacity>>,
    ended_fades: &mut RemovedComponents<CorpseOpacity>,
    children: &Query<&Children>,
    materials: &mut Query<
        (Entity, &mut MeshMaterial3d<M>, Has<NotShadowCaster>),
        Without<HostileRing>,
    >,
    state: &mut MaterialShadeState<M>,
    make_dark: impl FnOnce(&mut Assets<M>) -> Handle<M>,
    resolve_snuff: impl Fn(bool, &Handle<M>) -> Option<Handle<M>>,
) {
    let materials_removed = removed_materials.read().count() > 0;
    let fading_ended = ended_fades.read().count() > 0;
    let refresh = !changed_roots.is_empty()
        || dead_changed
        || materials_removed
        || fading_ended
        || materials
            .iter_mut()
            .any(|(_, material, _)| material.is_changed());
    if !refresh && changed_fades.is_empty() {
        return;
    }
    let dark = state.dark.get_or_insert_with(|| make_dark(assets)).clone();
    if refresh {
        state.meshes.retain(|entity, _| materials.contains(*entity));
        state
            .readable
            .retain(|(source, _), _| assets.contains(*source));
    }
    if fading_ended {
        state.fades.retain(|(root, _), _| fading.contains(*root));
    }
    let mut apply = |entity, mut material: Mut<MeshMaterial3d<M>>, shadowless: bool| {
        let mesh = state.meshes.entry(entity).or_insert_with(|| MeshShade {
            source: material.0.clone(),
            applied: material.0.id(),
            suppressed_shadow: false,
        });
        // Distinguish an authored replacement from our own dark or fading copy.
        if material.0.id() != mesh.applied && material.0 != dark {
            mesh.source = material.0.clone();
        }
        let (appearance, snuffed, root) = scope(entity, parents, roots, dead);
        if let Some(handle) = resolve_snuff(snuffed, &mesh.source) {
            mesh.source = handle;
        }
        let source = mesh.source.clone();
        let mut wanted = match appearance {
            Appearance::Lit => source,
            Appearance::Unlit | Appearance::Remembered => state
                .readable(assets, &source, appearance == Appearance::Remembered)
                .unwrap_or_else(|| dark.clone()),
            Appearance::Undisclosed => dark.clone(),
        };
        let fade = fading
            .get(root)
            .ok()
            .and_then(|opacity| state.faded(assets, &wanted, root, opacity.0));
        let mesh = state.meshes.get_mut(&entity).unwrap();
        if let Some(handle) = fade {
            wanted = handle;
        }
        // Remembered geometry and fading corpses must not cast a live shadow.
        if fading.contains(root) || appearance == Appearance::Remembered {
            if !mesh.suppressed_shadow && !shadowless {
                commands.entity(entity).insert(NotShadowCaster);
                mesh.suppressed_shadow = true;
            }
        } else if mesh.suppressed_shadow {
            commands.entity(entity).remove::<NotShadowCaster>();
            mesh.suppressed_shadow = false;
        }
        mesh.applied = wanted.id();
        if material.0 != wanted {
            material.0 = wanted;
        }
    };
    if refresh {
        for (entity, material, shadowless) in materials {
            apply(entity, material, shadowless);
        }
    } else {
        // Normal dissolve frames touch only the affected rigs, not the dungeon.
        for root in changed_fades {
            for entity in std::iter::once(root).chain(children.iter_descendants(root)) {
                if let Ok((_, material, shadowless)) = materials.get_mut(entity) {
                    apply(entity, material, shadowless);
                }
            }
        }
    }
}

/// Apply readable current darkness or subdued map memory, without changing
/// the scene's authoritative light sources.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn shade(
    mut commands: Commands,
    library: Res<PropLibrary>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    parents: Query<&ChildOf>,
    roots: Query<&Appearance>,
    changed_roots: Query<(), Changed<Appearance>>,
    mut removed_materials: RemovedComponents<MeshMaterial3d<StandardMaterial>>,
    dead: Query<&creatures::Snuffed>,
    changed_dead: Query<(), Changed<creatures::Snuffed>>,
    mut removed_dead: RemovedComponents<creatures::Snuffed>,
    fading: Query<&CorpseOpacity>,
    changed_fades: Query<Entity, Changed<CorpseOpacity>>,
    mut ended_fades: RemovedComponents<CorpseOpacity>,
    children: Query<&Children>,
    mut materials: Query<
        (
            Entity,
            &mut MeshMaterial3d<StandardMaterial>,
            Has<NotShadowCaster>,
        ),
        Without<HostileRing>,
    >,
    mut state: Local<MaterialShadeState<StandardMaterial>>,
) {
    let dead_changed = !changed_dead.is_empty() || removed_dead.read().count() > 0;
    shade_with(
        &mut commands,
        &mut assets,
        &parents,
        &roots,
        &changed_roots,
        &mut removed_materials,
        &dead,
        dead_changed,
        &fading,
        &changed_fades,
        &mut ended_fades,
        &children,
        &mut materials,
        &mut state,
        |assets| {
            assets.add(StandardMaterial {
                base_color: Color::srgb(0.15, 0.17, 0.22),
                unlit: true,
                ..default()
            })
        },
        |snuffed, source| {
            if snuffed {
                library.materials.dim(source)
            } else {
                library.materials.undim(source)
            }
        },
    );
}

/// Mirror of [`shade`] for meshes upgraded to [`crate::surface::ModelMaterial`].
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn shade_models(
    mut commands: Commands,
    mut assets: ResMut<Assets<crate::surface::ModelMaterial>>,
    parents: Query<&ChildOf>,
    roots: Query<&Appearance>,
    changed_roots: Query<(), Changed<Appearance>>,
    mut removed_materials: RemovedComponents<MeshMaterial3d<crate::surface::ModelMaterial>>,
    dead: Query<&creatures::Snuffed>,
    fading: Query<&CorpseOpacity>,
    changed_fades: Query<Entity, Changed<CorpseOpacity>>,
    mut ended_fades: RemovedComponents<CorpseOpacity>,
    children: Query<&Children>,
    mut materials: Query<
        (
            Entity,
            &mut MeshMaterial3d<crate::surface::ModelMaterial>,
            Has<NotShadowCaster>,
        ),
        Without<HostileRing>,
    >,
    mut state: Local<MaterialShadeState<crate::surface::ModelMaterial>>,
) {
    shade_with(
        &mut commands,
        &mut assets,
        &parents,
        &roots,
        &changed_roots,
        &mut removed_materials,
        &dead,
        false,
        &fading,
        &changed_fades,
        &mut ended_fades,
        &children,
        &mut materials,
        &mut state,
        |assets| {
            assets.add(crate::surface::ModelMaterial {
                base: StandardMaterial {
                    base_color: Color::srgb(0.15, 0.17, 0.22),
                    unlit: true,
                    ..default()
                },
                extension: crate::surface::ModelExtension {
                    params: crate::surface::SurfaceRecipe::WOOL.to_surface_params(false, false),
                    noise: None,
                },
            })
        },
        |_, _| None,
    );
}

/// Point lights must obey the same rule as their visible glowing meshes.
pub(super) fn lamps(
    parents: Query<&ChildOf>,
    roots: Query<&Appearance>,
    dead: Query<&creatures::Snuffed>,
    mut lights: Query<(Entity, &mut PointLight), Without<Torch>>,
    mut levels: Local<HashMap<Entity, (f32, f32)>>,
) {
    levels.retain(|entity, _| lights.contains(*entity));
    for (entity, mut light) in &mut lights {
        let (appearance, snuffed, _) = scope(entity, &parents, &roots, &dead);
        let (source, applied) = levels
            .entry(entity)
            .or_insert((light.intensity, light.intensity));
        if light.intensity != *applied && !snuffed {
            *source = light.intensity;
        }
        let wanted = if appearance == Appearance::Lit && !snuffed {
            *source
        } else {
            0.0
        };
        if light.intensity != wanted {
            light.intensity = wanted;
        }
        *applied = wanted;
    }
}
