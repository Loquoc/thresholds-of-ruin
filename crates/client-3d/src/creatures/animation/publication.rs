//! Publish evaluated poses without changing their values, clocks or ownership.
//! Keep the whole Transform: composition owns scale, not the animation pose.
use bevy::prelude::*;

pub(super) fn joint(transform: &mut Mut<Transform>, translation: Vec3, rotation: Quat) {
    let next = Transform {
        translation,
        rotation,
        ..**transform
    };
    transform.set_if_neq(next);
}

pub(super) fn rotation(transform: &mut Mut<Transform>, rotation: Quat) {
    let next = Transform {
        rotation,
        ..**transform
    };
    transform.set_if_neq(next);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracked(initial: Transform) -> (World, Entity) {
        let mut world = World::new();
        let entity = world.spawn(initial).id();
        world.increment_change_tick();
        world.clear_trackers();
        (world, entity)
    }

    #[test]
    fn identical_joint_pose_does_not_mark_transform_changed() {
        let initial = Transform::from_xyz(0.1, 2.0, -0.4)
            .with_rotation(Quat::from_rotation_y(0.37))
            .with_scale(Vec3::new(0.7, 1.3, 0.9));
        let (mut world, entity) = tracked(initial);
        let mut transform = world.get_mut::<Transform>(entity).unwrap();
        assert!(!transform.is_changed());
        joint(&mut transform, initial.translation, initial.rotation);
        assert_eq!(*transform, initial);
        assert!(
            !transform.is_changed(),
            "unchanged pose must not dirty its hierarchy"
        );
    }

    #[test]
    fn identical_drape_rotation_does_not_mark_transform_changed() {
        let initial = Transform::from_xyz(1.0, 2.0, 3.0)
            .with_rotation(Quat::from_rotation_x(0.4))
            .with_scale(Vec3::splat(1.7));
        let (mut world, entity) = tracked(initial);
        let mut transform = world.get_mut::<Transform>(entity).unwrap();
        rotation(&mut transform, initial.rotation);
        assert_eq!(*transform, initial);
        assert!(!transform.is_changed());
    }

    #[test]
    fn tiny_real_joint_change_is_published_exactly_without_touching_scale() {
        let initial = Transform::from_scale(Vec3::new(0.7, 1.3, 0.9));
        let (mut world, entity) = tracked(initial);
        let mut transform = world.get_mut::<Transform>(entity).unwrap();
        let translation = Vec3::new(1e-8, 0.0, 0.0);
        let rotation = Quat::from_rotation_x(1e-8);
        joint(&mut transform, translation, rotation);
        assert!(transform.is_changed());
        assert_eq!(transform.translation, translation);
        assert_eq!(transform.rotation, rotation);
        assert_eq!(transform.scale, initial.scale);
    }

    #[test]
    fn drape_rotation_preserves_translation_and_scale() {
        let initial = Transform::from_xyz(1.0, 2.0, 3.0).with_scale(Vec3::splat(0.3));
        let (mut world, entity) = tracked(initial);
        let mut transform = world.get_mut::<Transform>(entity).unwrap();
        let expected = Quat::from_rotation_z(-0.2);
        rotation(&mut transform, expected);
        assert!(transform.is_changed());
        assert_eq!(transform.rotation, expected);
        assert_eq!(transform.translation, initial.translation);
        assert_eq!(transform.scale, initial.scale);
    }
}
