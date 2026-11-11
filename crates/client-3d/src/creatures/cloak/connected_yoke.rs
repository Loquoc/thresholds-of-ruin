//! Noncommitting foundation for a sewn three-branch junction.
//! This straight prism is not a garment solver, source mask, or accepted fit.
use super::*;

#[derive(Debug)]
struct Junction {
    stations: [Vec3; 2],
    thickness: f32,
    offsets: Vec<Vec3>,
    owners: Vec<usize>,
    faces: Vec<[usize; 3]>,
}

impl Junction {
    /// Three rectangular cloth branches meet in one closed T cross-section.
    /// The canonical seam runs along +Y. Bent seams require a separate,
    /// qualified miter construction and are explicitly unsupported here.
    fn new(stations: [Vec3; 2], reach: f32, thickness: f32) -> Result<Self, &'static str> {
        if !stations.iter().all(|p| p.is_finite())
            || !reach.is_finite()
            || !thickness.is_finite()
            || thickness <= 0.0
            || reach <= thickness
        {
            return Err("invalid junction dimensions");
        }
        let delta = stations[1] - stations[0];
        if delta.x != 0.0 || delta.z != 0.0 || delta.y <= thickness {
            return Err("unsupported seam direction or collapsed station interval");
        }
        let h = thickness * 0.5;
        let ring = [
            Vec3::new(-reach, 0.0, -h),
            Vec3::new(reach, 0.0, -h),
            Vec3::new(reach, 0.0, h),
            Vec3::new(h, 0.0, h),
            Vec3::new(h, 0.0, reach),
            Vec3::new(-h, 0.0, reach),
            Vec3::new(-h, 0.0, h),
            Vec3::new(-reach, 0.0, h),
        ];
        let mut offsets = ring.into_iter().chain(ring).collect::<Vec<_>>();
        offsets.extend([Vec3::ZERO; 2]);
        let owners = (0..18)
            .map(|i| {
                if i == 16 {
                    0
                } else if i >= 8 {
                    1
                } else {
                    0
                }
            })
            .collect();
        let mut faces = Vec::new();
        for i in 0..8 {
            let j = (i + 1) % 8;
            faces.extend([
                [i, i + 8, j],
                [j, i + 8, j + 8],
                [16, i, j],
                [17, j + 8, i + 8],
            ]);
        }
        Ok(Self {
            stations,
            thickness,
            offsets,
            owners,
            faces,
        })
    }

    /// Transactional supported station motion; not curved garment deformation.
    fn move_stations(&mut self, stations: [Vec3; 2]) -> Result<(), &'static str> {
        let delta = stations[1] - stations[0];
        if !stations.iter().all(|p| p.is_finite())
            || delta.x != 0.0
            || delta.z != 0.0
            || delta.y <= self.thickness
        {
            return Err("unsupported moved seam");
        }
        self.stations = stations;
        Ok(())
    }

    fn positions(&self) -> Vec<Vec3> {
        self.offsets
            .iter()
            .zip(&self.owners)
            .map(|(o, &s)| self.stations[s] + o)
            .collect()
    }

    /// Render corners may have distinct normals while sharing canonical motion.
    fn corners(&self) -> Vec<(usize, Vec3, Vec3)> {
        let p = self.positions();
        self.faces
            .iter()
            .flat_map(|f| {
                let n = (p[f[1]] - p[f[0]]).cross(p[f[2]] - p[f[0]]).normalize();
                f.map(|i| (self.owners[i], p[i], n))
            })
            .collect()
    }
}

#[derive(Debug, PartialEq)]
enum PairObligation {
    FullNonadjacentClearance,
    JunctionPhysicalCheck,
}

/// A seam ID alone is insufficient: both exact incident faces must be owned.
/// Neither result authorizes skipping clearance or the physical junction check.
fn pair_obligation(a: usize, b: usize, owned: &[[usize; 2]]) -> PairObligation {
    if a != b && owned.iter().any(|p| *p == [a, b] || *p == [b, a]) {
        PairObligation::JunctionPhysicalCheck
    } else {
        PairObligation::FullNonadjacentClearance
    }
}

#[test]
fn connected_yoke_junction_is_closed_oriented_and_has_physical_thickness() {
    use std::collections::BTreeMap;
    let j = Junction::new([Vec3::ZERO, Vec3::Y * 0.2], 0.06, THICKNESS).unwrap();
    let p = j.positions();
    let mut edges = BTreeMap::<(usize, usize), (usize, i32)>::new();
    let mut volume = 0.0;
    for f in &j.faces {
        assert!((p[f[1]] - p[f[0]]).cross(p[f[2]] - p[f[0]]).length() > 1e-7);
        volume += p[f[0]].dot(p[f[1]].cross(p[f[2]])) / 6.0;
        for (a, b) in [(f[0], f[1]), (f[1], f[2]), (f[2], f[0])] {
            let e = edges.entry((a.min(b), a.max(b))).or_default();
            e.0 += 1;
            e.1 += if a < b { 1 } else { -1 };
        }
    }
    assert!(edges.values().all(|&(n, sign)| n == 2 && sign == 0));
    assert_eq!(
        p.len() as isize - edges.len() as isize + j.faces.len() as isize,
        2
    );
    // Independent three-rectangle union area, multiplied by seam length.
    let expected = (2.0 * 0.06 * THICKNESS + THICKNESS * (0.06 - THICKNESS * 0.5)) * 0.2;
    assert!((volume - expected).abs() < 1e-8);
    assert!((p[2].z - p[1].z - 0.012).abs() < 1e-7);
    assert!((p[4].x - p[5].x - 0.012).abs() < 1e-7);
}

#[test]
fn connected_yoke_render_corners_share_motion_without_averaging_branch_normals() {
    let mut j = Junction::new([Vec3::ZERO, Vec3::Y * 0.2], 0.06, THICKNESS).unwrap();
    let before = j.corners();
    let shift = Vec3::new(0.02, 0.01, -0.03);
    j.move_stations(j.stations.map(|p| p + shift)).unwrap();
    let after = j.corners();
    assert_eq!(before.len(), 96);
    for ((owner, p, n), (new_owner, q, m)) in before.iter().zip(&after) {
        assert_eq!(owner, new_owner);
        assert!((*q - *p - shift).length() < 1e-7);
        assert!((*n - *m).length() < 1e-6);
    }
    let mut distinct = false;
    for a in &before {
        for b in &before {
            if a.1 == b.1 && a.2.dot(b.2) < 0.5 {
                distinct = true;
            }
        }
    }
    assert!(
        distinct,
        "junction corner normals must remain branch-specific"
    );
}

#[test]
fn connected_yoke_rejects_unsupported_junctions_and_never_blanket_exempts_sources() {
    for (stations, reach, thickness) in [
        ([Vec3::ZERO, Vec3::ZERO], 0.06, THICKNESS),
        ([Vec3::ZERO, Vec3::new(0.01, 0.2, 0.0)], 0.06, THICKNESS),
        ([Vec3::ZERO, Vec3::Y], 0.005, THICKNESS),
        ([Vec3::ZERO, Vec3::Y], 0.06, 0.0),
        ([Vec3::ZERO, Vec3::Y], f32::NAN, THICKNESS),
    ] {
        assert!(Junction::new(stations, reach, thickness).is_err());
    }
    assert_eq!(
        pair_obligation(5, 7, &[[5, 7]]),
        PairObligation::JunctionPhysicalCheck
    );
    assert_eq!(
        pair_obligation(7, 5, &[[5, 7]]),
        PairObligation::JunctionPhysicalCheck
    );
    assert_eq!(
        pair_obligation(5, 8, &[[5, 7]]),
        PairObligation::FullNonadjacentClearance
    );
    assert_eq!(
        pair_obligation(5, 5, &[[5, 5]]),
        PairObligation::FullNonadjacentClearance
    );
    assert!((LAYER_GAP + THICKNESS - 0.032).abs() < 1e-8);
}

#[test]
fn connected_yoke_head_policy_is_independent_of_inner_entity_and_invalidates_pose() {
    let body = HumanoidBody::resolve(&Look::with_choice(
        CreatureModel::Human,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    ));
    let r = outfit::outerwear::OuterwearAppearance::LeatherCloak.recipe();
    let outfit::outerwear::OuterwearCut::Drape(cut) = r.cut else {
        panic!("drape fixture")
    };
    let mut recipe = Recipe::new(body, r.cloth, r.lining, false);
    recipe.set_cut(cut);
    recipe.head_attachment = HeadAttachmentPolicy::SupportedCollar;
    // Nonempty independent source, deliberately away from the collar: this
    // tests source ownership/epochs, not the failing layered garment fit.
    let triangles = Arc::new(vec![[
        Vec3::new(-0.1, 3.0, 0.0),
        Vec3::new(0.1, 3.0, 0.0),
        Vec3::new(0.0, 3.1, 0.1),
    ]]);
    let snapshot = |angle| {
        Arc::new(ContactSurface::new(vec![
            super::super::visual_contact::PosedTriangles::new(
                triangles.clone(),
                Affine3A::from_quat(Quat::from_rotation_y(angle)),
            ),
        ]))
    };
    let initial = snapshot(0.0);
    assert_eq!(initial.parts[0].triangles.len(), 1);
    recipe.set_head_surface(initial);
    assert!(recipe.inner_layer.is_none());
    let mut materials = Assets::<StandardMaterial>::default();
    let mut library = PropLibrary::new(crate::props::PropMaterials::new(&mut materials));
    let mut meshes = Assets::<Mesh>::default();
    let mut world = World::new();
    let parent = world.spawn_empty().id();
    let mut queue = bevy::ecs::world::CommandQueue::default();
    let mut main = spawn(
        &mut Commands::new(&mut queue, &world),
        &mut library,
        &mut meshes,
        parent,
        recipe,
        false,
    );
    assert!(main.inner_layer.is_none());
    assert!(main.head_surface.is_some());
    assert!(main.attachment_limits.is_some());
    assert_eq!(main.attachment_reference.len(), 2 * (COLUMNS + 1));
    assert_eq!(
        main.contact_fallbacks, 0,
        "positive policy fixture must initialize successfully"
    );
    let posed = snapshot(0.1);
    main.last_rotation = Some(Quat::IDENTITY);
    let old = main.head_surface.clone().unwrap();
    main.set_head_surface(posed.clone());
    assert!(!Arc::ptr_eq(&old, main.head_surface.as_ref().unwrap()));
    assert!(main.last_rotation.is_none());
    main.last_rotation = Some(Quat::IDENTITY);
    main.set_head_surface(posed);
    assert_eq!(main.last_rotation, Some(Quat::IDENTITY));
    main.head_attachment = HeadAttachmentPolicy::None;
    main.set_head_surface(snapshot(0.2));
    assert_eq!(main.last_rotation, Some(Quat::IDENTITY));
    // No dynamic fit/mesh commit is attempted for this policy-only fixture.
}

#[test]
fn connected_yoke_nonrigid_axial_motion_and_rejected_bend_are_transactional() {
    let mut j = Junction::new([Vec3::ZERO, Vec3::Y * 0.2], 0.06, THICKNESS).unwrap();
    let before = j.corners();
    j.move_stations([Vec3::ZERO, Vec3::Y * 0.24]).unwrap();
    let after = j.corners();
    for ((owner, p, _), (_, q, _)) in before.iter().zip(&after) {
        let expected = if *owner == 0 {
            Vec3::ZERO
        } else {
            Vec3::Y * 0.04
        };
        assert!((*q - *p - expected).length() < 1e-7);
    }
    assert!(j
        .move_stations([Vec3::ZERO, Vec3::new(0.01, 0.24, 0.0)])
        .is_err());
    assert_eq!(
        j.corners(),
        after,
        "unsupported bend must not partially commit"
    );
    assert!(j.move_stations([Vec3::ZERO, Vec3::ZERO]).is_err());
    assert_eq!(j.corners(), after);
}
