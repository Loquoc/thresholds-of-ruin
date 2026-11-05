//! Mouse selection in the same disclosed 3D cells that the scene renders.
use crate::{ivec, stand};
use bevy::{
    math::bounding::{Aabb3d, RayCast3d},
    prelude::*,
    render::primitives::Aabb,
};
use tor_client_3d_model::{known_cells, Focus, View};
use tor_protocol::Position;

pub(super) fn floor_cell(
    ray: Ray3d,
    view: &View,
    offset: IVec3,
    cutaway: bool,
) -> Option<(Position, f32)> {
    PreparedFloor::new(view, offset, cutaway).select(ray)
}

/// Frame-local terrain preparation. Never retained across observation or scene changes.
struct PreparedFloor(Vec<(Position, Vec3)>);

impl PreparedFloor {
    fn new(view: &View, offset: IVec3, cutaway: bool) -> Self {
        #[cfg(test)]
        crate::loot_labels::work::add(|counts| counts.maps += 1);
        let mut cells = std::collections::HashMap::new();
        for cell in known_cells(view) {
            #[cfg(test)]
            crate::loot_labels::work::add(|counts| counts.terrain += 1);
            cells.entry(cell.position).or_insert(cell);
        }
        Self(
            cells
                .values()
                .filter_map(|cell| {
                    if cell.wall
                        || (!cutaway && i64::from(cell.position.z) - i64::from(view.feet.z) >= 2)
                    {
                        return None;
                    }
                    let below = Position {
                        z: cell.position.z.checked_sub(1)?,
                        ..cell.position
                    };
                    cells
                        .get(&below)
                        .is_some_and(|c| c.wall && c.door.is_none())
                        .then(|| (cell.position, stand(ivec(cell.position) + offset)))
                })
                .collect(),
        )
    }

    fn select(&self, ray: Ray3d) -> Option<(Position, f32)> {
        self.0
            .iter()
            .filter_map(|&(position, center)| {
                let distance = ray.intersect_plane(center, InfinitePlane3d::new(Vec3::Y))?;
                let point = ray.get_point(distance);
                ((point.x - center.x).abs() <= 0.5 && (point.z - center.z).abs() <= 0.5)
                    .then_some((position, distance))
            })
            .min_by(|a, b| {
                a.1.total_cmp(&b.1)
                    .then_with(|| (a.0.x, a.0.y, a.0.z).cmp(&(b.0.x, b.0.y, b.0.z)))
            })
    }
}

/// Prepared once for a label update, borrowing the current animated mesh transforms.
/// It cannot outlive its query borrow and is not an observation-revision cache.
pub(super) struct Prepared<'a> {
    floor: PreparedFloor,
    meshes: Vec<(Focus, &'a Aabb, &'a GlobalTransform)>,
}

impl Prepared<'_> {
    pub(super) fn select(&self, ray: Ray3d) -> Option<Focus> {
        let mut nearest = self
            .floor
            .select(ray)
            .map(|(at, distance)| (Focus::Cell(at), distance));
        for &(target, bounds, transform) in &self.meshes {
            if let Some(distance) = model_distance(ray, bounds, transform) {
                if nearest.is_none_or(|(_, old)| distance < old) {
                    nearest = Some((target, distance));
                }
            }
        }
        nearest.map(|(target, _)| target)
    }
}

#[derive(Component, Clone, Copy)]
pub(super) struct Pick(pub Focus);

/// A mesh whose bounds only keep deforming cloth or hair from being culled:
/// a generous box around its whole reach, not its shape. Picking ignores it
/// and finds its wearer by the body instead, or a figure several cells away
/// would claim the cursor whenever its swinging box crossed the ray.
#[derive(Component, Clone, Copy, Default)]
pub(crate) struct LooseBounds;

type Decorations = Or<(With<crate::Glint>, With<crate::HostileRing>)>;
/// Meshes whose bounds describe their shape.
type Shaped = (With<Mesh3d>, Without<LooseBounds>);

#[derive(bevy::ecs::system::SystemParam)]
pub(super) struct Models<'w, 's> {
    meshes: Query<
        'w,
        's,
        (
            Entity,
            &'static Aabb,
            &'static GlobalTransform,
            &'static InheritedVisibility,
        ),
        Shaped,
    >,
    parents: Query<'w, 's, &'static ChildOf>,
    targets: Query<'w, 's, &'static Pick>,
    decorations: Query<'w, 's, (), Decorations>,
}

impl Models<'_, '_> {
    fn target(&self, mut root: Entity, view: &View) -> Option<Focus> {
        loop {
            if self.decorations.contains(root) {
                return None;
            }
            if let Ok(target) = self.targets.get(root) {
                return target.0.position(view).map(|_| target.0);
            }
            root = self.parents.get(root).ok()?.parent();
        }
    }

    pub(super) fn prepare(&self, view: &View, offset: IVec3, cutaway: bool) -> Prepared<'_> {
        let floor = PreparedFloor::new(view, offset, cutaway);
        let meshes = self
            .meshes
            .iter()
            .filter_map(|(entity, bounds, transform, visible)| {
                #[cfg(test)]
                crate::loot_labels::work::add(|counts| counts.meshes += 1);
                if !visible.get() {
                    return None;
                }
                Some((self.target(entity, view)?, bounds, transform))
            })
            .collect();
        Prepared { floor, meshes }
    }

    pub(super) fn select(
        &self,
        ray: Ray3d,
        view: &View,
        offset: IVec3,
        cutaway: bool,
    ) -> Option<Focus> {
        let mut nearest = floor_cell(ray, view, offset, cutaway)
            .map(|(at, distance)| (Focus::Cell(at), distance));
        for (entity, bounds, transform, visible) in &self.meshes {
            #[cfg(test)]
            crate::loot_labels::work::add(|counts| counts.meshes += 1);
            if !visible.get() {
                continue;
            }
            let Some(target) = self.target(entity, view) else {
                continue;
            };
            if let Some(distance) = model_distance(ray, bounds, transform) {
                if nearest.is_none_or(|(_, old)| distance < old) {
                    nearest = Some((target, distance));
                }
            }
        }
        nearest.map(|(target, _)| target)
    }
}

/// Intersect a mesh's retained local bounds, preserving rotation and scale.
/// No CPU copy of uploaded procedural triangles is needed.
fn model_distance(ray: Ray3d, bounds: &Aabb, transform: &GlobalTransform) -> Option<f32> {
    let affine = transform.affine();
    if !affine.matrix3.determinant().is_finite() || affine.matrix3.determinant().abs() < 1e-8 {
        return None;
    }
    let inverse = affine.inverse();
    let origin = inverse.transform_point3(ray.origin);
    let direction = inverse.transform_vector3(*ray.direction);
    let scale = direction.length();
    let cast = RayCast3d::new(origin, Dir3::new(direction).ok()?, f32::MAX);
    cast.aabb_intersection_at(&Aabb3d::new(bounds.center, bounds.half_extents))
        .map(|distance| distance / scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tor_protocol::CellView;

    fn cell(at: Position, wall: bool) -> CellView {
        CellView {
            object_presentation: None,
            position: at,
            wall,
            lit: true,
            key: format!("{at:?}"),
            material: String::new(),
            asset: None,
            trap: None,
            door: None,
            stairs_up: false,
            stairs_down: false,
            place_hint: false,
        }
    }

    #[test]
    fn a_ray_selects_the_elevated_known_surface_instead_of_the_players_floor() {
        let mut view = crate::test_support::view(0);
        let upper = Position { x: 3, y: 0, z: 1 };
        view.visible = vec![cell(upper, false), cell(Position { z: 0, ..upper }, true)];
        let ray = Ray3d::new(Vec3::new(3.0, 5.0, 0.0), Dir3::NEG_Y);
        assert_eq!(
            floor_cell(ray, &view, IVec3::ZERO, true).map(|(at, _)| at),
            Some(upper)
        );
    }

    #[test]
    fn unknown_space_does_not_become_an_infinite_selectable_floor() {
        let view = crate::test_support::view(0);
        let ray = Ray3d::new(Vec3::new(3.0, 5.0, 0.0), Dir3::NEG_Y);
        assert!(floor_cell(ray, &view, IVec3::ZERO, true).is_none());
    }

    #[test]
    fn culling_only_bounds_never_capture_the_cursor() {
        use bevy::ecs::system::SystemState;
        let at = Position { x: 5, y: 0, z: 0 };
        let mut view = crate::test_support::view(0);
        view.visible = vec![cell(at, true)];
        // The ray passes 1 m from the mesh's origin: inside its 1.5 m box only.
        let ray = Ray3d::new(Vec3::new(2.0, 10.0, 0.0), Dir3::NEG_Y);
        let pick = |loose: bool| {
            let mut world = World::new();
            let owner = world
                .spawn((
                    Pick(Focus::Cell(at)),
                    Transform::default(),
                    GlobalTransform::default(),
                ))
                .id();
            let mut mesh = world.spawn((
                Mesh3d(Handle::default()),
                Aabb {
                    center: Vec3::ZERO.into(),
                    half_extents: Vec3::splat(1.5).into(),
                },
                GlobalTransform::from_xyz(3.0, 0.0, 0.0),
                InheritedVisibility::VISIBLE,
                ChildOf(owner),
            ));
            if loose {
                mesh.insert(LooseBounds);
            }
            let mut state = SystemState::<Models>::new(&mut world);
            state.get(&world).select(ray, &view, IVec3::ZERO, true)
        };
        assert!(
            matches!(pick(false), Some(Focus::Cell(cell)) if cell == at),
            "a mesh's own bounds are pickable"
        );
        assert!(
            pick(true).is_none(),
            "a culling box is not the figure's shape"
        );
    }

    #[test]
    fn model_bounds_preserve_nonuniform_scale_rotation_and_world_distance() {
        let bounds = Aabb {
            center: Vec3::ZERO.into(),
            half_extents: Vec3::new(1.0, 0.5, 0.25).into(),
        };
        let transform = GlobalTransform::from(
            Transform::from_xyz(3.0, 2.0, 0.0)
                .with_scale(Vec3::new(2.0, 3.0, 4.0))
                .with_rotation(Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)),
        );
        let ray = Ray3d::new(Vec3::new(3.0, 10.0, 0.0), Dir3::NEG_Y);
        assert!((model_distance(ray, &bounds, &transform).unwrap() - 6.5).abs() < 1e-5);
        assert!(model_distance(
            Ray3d::new(Vec3::new(5.0, 10.0, 0.0), Dir3::NEG_Y),
            &bounds,
            &transform
        )
        .is_none());
    }
}
