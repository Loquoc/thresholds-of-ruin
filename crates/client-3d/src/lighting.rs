//! Illumination is a disclosed server fact, separate from visual awareness.
use super::*;
use bevy::pbr::NotShadowCaster;
#[cfg(test)]
mod tests;

/// Lighting of a scene root. Descendants (including held objects) inherit it.
#[derive(Component, Clone, Copy)]
pub(super) struct Lit(pub bool);

/// Per-body visual opacity for an unclaimed historical corpse.
#[derive(Component, Clone, Copy)]
pub(super) struct CorpseOpacity(pub f32);

pub(super) fn at(view: &View, position: Position) -> bool {
    view.visible
        .iter()
        .any(|cell| cell.position == position && cell.lit)
}

fn scope(
    mut entity: Entity,
    parents: &Query<&ChildOf>,
    roots: &Query<&Lit>,
    dead: &Query<&creatures::Snuffed>,
) -> (bool, bool, Entity) {
    let mut snuffed = false;
    loop {
        snuffed |= dead.contains(entity);
        if let Ok(lit) = roots.get(entity) {
            return (lit.0, snuffed, entity);
        }
        let Ok(parent) = parents.get(entity) else {
            // No disclosed source cell, so never invent a light source.
            return (false, snuffed, entity);
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
        (Entity, &Transform, Option<&Lit>),
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
        let lit = game.view.as_ref().is_some_and(|view| at(view, position));
        if previous.is_none_or(|previous| previous.0 != lit) {
            commands.entity(entity).insert(Lit(lit));
        }
    }
}

#[derive(Default)]
pub(super) struct ShadeState {
    meshes: HashMap<Entity, MeshShade>,
    dark: Option<Handle<StandardMaterial>>,
    fades: HashMap<(Entity, AssetId<StandardMaterial>), FadeMaterial>,
}

struct MeshShade {
    source: Handle<StandardMaterial>,
    applied: AssetId<StandardMaterial>,
    suppressed_shadow: bool,
}

struct FadeMaterial {
    handle: Handle<StandardMaterial>,
    source_alpha: f32,
    opacity: f32,
}

impl ShadeState {
    fn faded(
        &mut self,
        assets: &mut Assets<StandardMaterial>,
        source: &Handle<StandardMaterial>,
        root: Entity,
        opacity: f32,
    ) -> Option<Handle<StandardMaterial>> {
        use std::collections::hash_map::Entry;
        match self.fades.entry((root, source.id())) {
            Entry::Occupied(mut slot) => {
                let fade = slot.get_mut();
                if fade.opacity != opacity {
                    assets
                        .get_mut(&fade.handle)?
                        .base_color
                        .set_alpha(fade.source_alpha * opacity);
                    fade.opacity = opacity;
                }
                Some(fade.handle.clone())
            }
            Entry::Vacant(slot) => {
                let mut copy = assets.get(source)?.clone();
                let source_alpha = copy.base_color.alpha();
                copy.base_color.set_alpha(source_alpha * opacity);
                // Recipe transparency still contributes to the dissolve.
                copy.alpha_mode = AlphaMode::Blend;
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

/// Muted unlit silhouettes cannot receive the artistic directional/fill light.
/// Restore the recipe's material when authoritative illumination returns.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn shade(
    mut commands: Commands,
    library: Res<PropLibrary>,
    mut assets: ResMut<Assets<StandardMaterial>>,
    parents: Query<&ChildOf>,
    roots: Query<&Lit>,
    changed_roots: Query<(), Changed<Lit>>,
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
    mut state: Local<ShadeState>,
) {
    let materials_removed = removed_materials.read().count() > 0;
    let dead_removed = removed_dead.read().count() > 0;
    let fading_ended = ended_fades.read().count() > 0;
    let refresh = !changed_roots.is_empty()
        || !changed_dead.is_empty()
        || dead_removed
        || materials_removed
        || fading_ended
        || materials
            .iter_mut()
            .any(|(_, material, _)| material.is_changed());
    if !refresh && changed_fades.is_empty() {
        return;
    }
    let dark = state
        .dark
        .get_or_insert_with(|| {
            assets.add(StandardMaterial {
                base_color: Color::srgb(0.15, 0.17, 0.22),
                unlit: true,
                ..default()
            })
        })
        .clone();
    if refresh {
        state.meshes.retain(|entity, _| materials.contains(*entity));
    }
    if fading_ended {
        state.fades.retain(|(root, _), _| fading.contains(*root));
    }
    let mut apply =
        |entity, mut material: Mut<MeshMaterial3d<StandardMaterial>>, shadowless: bool| {
            let mesh = state.meshes.entry(entity).or_insert_with(|| MeshShade {
                source: material.0.clone(),
                applied: material.0.id(),
                suppressed_shadow: false,
            });
            // Distinguish an authored replacement from our own dark or fading copy.
            if material.0.id() != mesh.applied && material.0 != dark {
                mesh.source = material.0.clone();
            }
            let (lit, snuffed, root) = scope(entity, &parents, &roots, &dead);
            if let Some(handle) = if snuffed {
                library.materials.dim(&mesh.source)
            } else {
                library.materials.undim(&mesh.source)
            } {
                mesh.source = handle;
            }
            let mut wanted = if lit {
                mesh.source.clone()
            } else {
                dark.clone()
            };
            let fade = fading
                .get(root)
                .ok()
                .and_then(|opacity| state.faded(&mut assets, &wanted, root, opacity.0));
            let mesh = state.meshes.get_mut(&entity).unwrap();
            if let Some(handle) = fade {
                wanted = handle;
                // Alpha blending does not fade Bevy's shadow-map silhouette.
                // Drop that opaque trace as the visual body starts dissolving.
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
        for (entity, material, shadowless) in &mut materials {
            apply(entity, material, shadowless);
        }
    } else {
        // Normal dissolve frames touch only the affected rigs, not the dungeon.
        for root in &changed_fades {
            for entity in std::iter::once(root).chain(children.iter_descendants(root)) {
                if let Ok((_, material, shadowless)) = materials.get_mut(entity) {
                    apply(entity, material, shadowless);
                }
            }
        }
    }
}

/// Point lights must obey the same rule as their visible glowing meshes.
pub(super) fn lamps(
    parents: Query<&ChildOf>,
    roots: Query<&Lit>,
    dead: Query<&creatures::Snuffed>,
    mut lights: Query<(Entity, &mut PointLight), Without<Torch>>,
    mut levels: Local<HashMap<Entity, (f32, f32)>>,
) {
    levels.retain(|entity, _| lights.contains(*entity));
    for (entity, mut light) in &mut lights {
        let (lit, snuffed, _) = scope(entity, &parents, &roots, &dead);
        let (source, applied) = levels
            .entry(entity)
            .or_insert((light.intensity, light.intensity));
        if light.intensity != *applied && !snuffed {
            *source = light.intensity;
        }
        let wanted = if lit && !snuffed { *source } else { 0.0 };
        if light.intensity != wanted {
            light.intensity = wanted;
        }
        *applied = wanted;
    }
}
