//! A resolved head component owns both its cache dependencies and its geometry.
//! Unrelated garments reuse the head; neck adornments cannot inherit an older
//! outfit merely because it selected the same headwear.
use super::super::outfit::{Earrings, Neck};
use super::*;

#[derive(Clone, Copy, Debug)]
struct Finish {
    headwear: Headwear,
    neck_band: Option<Neck>,
    earrings: Option<Earrings>,
    cuirass: bool,
    hood_binding: Option<Vec3>,
}

#[derive(Clone, Copy)]
pub(super) struct Recipe {
    finish: Finish,
}

impl Recipe {
    pub(super) fn resolve(look: &Look) -> Self {
        let outfit = Outfit::worn(look);
        Self {
            finish: Finish {
                headwear: outfit.head,
                neck_band: outfit.trimmings.neck,
                earrings: outfit.trimmings.earrings,
                cuirass: outfit.outer == Some(Outer::Cuirass),
                hood_binding: look
                    .equipment
                    .outerwear()
                    .filter(|kind| {
                        *kind == super::outfit::outerwear::OuterwearAppearance::HoodedCloak
                    })
                    .filter(|_| matches!(outfit.head, Headwear::Hood { .. }))
                    .map(|kind| kind.recipe().trim),
            },
        }
    }

    pub(super) fn key(&self, look: &Look) -> String {
        let body = look.body_key.as_deref().unwrap_or(&look.key);
        format!("{body}:{:?}", self.finish)
    }

    pub(super) fn emit(&self, p: &mut Parts, l: &Look, body: HumanoidBody) {
        let style = l.style;
        let fem = body.woman;
        let b = body.build;
        let blocky = style == Style::Guardian;
        let furry = style == Style::Yeti;
        let Finish {
            headwear,
            neck_band,
            earrings,
            cuirass,
            hood_binding,
        } = self.finish;
        let neck = if furry { l.cloth } else { l.skin };
        let slim = neck_slim(fem);
        if blocky || furry || cuirass {
            p.matte.prism(
                Vec3::Y * -0.03,
                Vec3::Y * 0.08,
                0.06 * b * slim,
                0.052 * b * slim,
                7,
                neck,
                0.0,
            );
        } else {
            biped_neck_on_body(&mut p.matte, body, neck);
            if let Some(worn) = neck_band {
                accessory_parts::neck_band_on_support(
                    p,
                    worn,
                    b,
                    slim,
                    Some(&face_shape(l, !matches!(style, Style::Hobgoblin | Style::Troll)).form),
                );
            }
        }
        match style {
            Style::Guardian => {
                stone(
                    &mut p.matte,
                    Vec3::Y * 0.15,
                    Vec3::new(0.13, 0.12, 0.13),
                    l.skin,
                    5,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.2, 0.115),
                    Vec3::new(0.12, 0.03, 0.03),
                    l.skin * 0.8,
                    6,
                );
                stone(
                    &mut p.matte,
                    Vec3::new(0.0, 0.052, 0.11),
                    Vec3::new(0.08, 0.022, 0.03),
                    l.skin * 0.75,
                    7,
                );
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.13), 0.05, 0.022);
                // The chin ends below the smile instead of hiding its middle.
                // Wear chips inward from the face slab's .13 front plane.
                let smile: [Vec2; 25] = std::array::from_fn(|i| {
                    let u = i as f32 / 24.0 * 2.0 - 1.0;
                    Vec2::new(u * 0.052, 0.085 + u * u * 0.01)
                });
                surface_stroke(
                    &mut p.matte,
                    &smile,
                    0.003,
                    &|at| (Vec3::new(at.x, at.y, 0.13), Vec3::Z),
                    DARK * 2.0,
                );
            }
            Style::Yeti => {
                p.matte.blob(
                    Vec3::new(0.0, 0.14, 0.0),
                    Vec3::new(0.15, 0.15, 0.14),
                    6,
                    0.06,
                    l.cloth,
                );
                hair_tufts(
                    p,
                    Vec3::new(0.0, 0.24, -0.02),
                    0.12,
                    9,
                    Vec3::new(0.0, -0.4, -1.0),
                    l.cloth,
                    41,
                );
                // One dark ape face joins brow, flat nose and muzzle. The
                // fitted mouth is independent of the surrounding fur shell.
                use crate::sdf::{Sculpt, Shape::Ellipsoid};
                let mut face = Sculpt::default();
                face.add(
                    Ellipsoid(Vec3::new(0.0, 0.12, 0.09), Vec3::new(0.1, 0.1, 0.07)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.175, 0.12), Vec3::new(0.09, 0.025, 0.03)),
                    0.018,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.069, 0.132), Vec3::new(0.061, 0.035, 0.041)),
                    0.018,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.119, 0.16), Vec3::new(0.033, 0.019, 0.021)),
                    0.012,
                );
                face.mesh(
                    &mut p.matte,
                    Vec3::new(-0.115, 0.012, 0.012),
                    Vec3::new(0.115, 0.228, 0.195),
                    0.0055,
                    &|at, _| {
                        let muzzle =
                            (1.0 - (at.x / 0.07).powi(2) - ((at.y - 0.069) / 0.045).powi(2))
                                .clamp(0.0, 1.0);
                        l.skin * (1.0 - muzzle * 0.12)
                    },
                );
                let mouth_height = |x: f32| 0.08 + (x / 0.046).powi(2) * 0.006;
                let mouth: [Vec2; 25] = std::array::from_fn(|i| {
                    let x = (i as f32 / 24.0 * 2.0 - 1.0) * 0.046;
                    Vec2::new(x, mouth_height(x))
                });
                surface_stroke(
                    &mut p.matte,
                    &mouth,
                    0.0032,
                    &|at| torso_surface_frame(&face, at, true),
                    l.skin * 0.42,
                );
                for side in [-1.0f32, 1.0] {
                    let (at, normal) =
                        torso_surface_frame(&face, Vec2::new(side * 0.013, 0.114), true);
                    p.matte.blob(
                        at + normal * 0.0012,
                        Vec3::new(0.006, 0.0035, 0.002),
                        0,
                        0.0,
                        l.skin * 0.42,
                    );
                }
                eyes(p, l.eyes, Vec3::new(0.0, 0.15, 0.15), 0.035, 0.016);
                for side in [-1.0f32, 1.0] {
                    p.matte.prism(
                        Vec3::new(side * 0.1, 0.24, 0.0),
                        Vec3::new(side * 0.18, 0.34, -0.06),
                        0.035,
                        0.0,
                        6,
                        BONE,
                        0.0,
                    );
                    let x = side * 0.035;
                    let (root, normal) =
                        torso_surface_frame(&face, Vec2::new(x, mouth_height(x)), true);
                    let root = root - normal * 0.002;
                    p.matte.prism(
                        root,
                        root + Vec3::new(side * 0.005, -0.04, 0.01),
                        0.012,
                        0.0,
                        4,
                        BONE,
                        0.0,
                    );
                }
            }
            _ => {
                // A slightly large head reads well from above.
                let size = if fem { 1.1 } else { 1.14 };
                p.set_transform(
                    Affine3A::from_translation(Vec3::Y * 0.08)
                        * Affine3A::from_scale(Vec3::splat(size))
                        * Affine3A::from_translation(Vec3::Y * -0.08),
                );
                human_face(p, l, !matches!(style, Style::Hobgoblin | Style::Troll));
                if shows_hair(l) {
                    hair(p, l, !loose_fall(l));
                }
                if (l.v.beard
                    && matches!(
                        style,
                        Style::Hero | Style::Caster | Style::Scout | Style::Common | Style::Zombie
                    ))
                    || (style == Style::Leprechaun && !fem)
                {
                    beard(p, hair_color(l));
                }
                // What the outfit puts on the head.
                match headwear {
                    Headwear::Equipment(kind) => {
                        equipment_headwear::emit(p, kind);
                        // Covered scalp stays hidden; the selected hairstyle
                        // emerges below the equipment's actual rear opening.
                        equipment_hair(p, l, kind);
                    }
                    Headwear::Helm => {
                        let mut helm_shell = p.metal.empty_like();
                        native_helm_shell(&mut helm_shell);
                        // Retain shell-only support; visor, face and plume are not fit surfaces.
                        // The visor: a dark slit, a raised ridge down the face, breathing holes.
                        p.matte.cuboid(
                            Vec3::new(0.0, 0.17, 0.128),
                            Vec3::new(0.095, 0.011, 0.02),
                            Quat::IDENTITY,
                            DARK,
                        );
                        p.metal.cuboid(
                            Vec3::new(0.0, 0.12, 0.138),
                            Vec3::new(0.01, 0.06, 0.012),
                            Quat::IDENTITY,
                            STEEL * 1.1,
                        );
                        for side in [-1.0f32, 1.0] {
                            for k in 0..3 {
                                p.matte.blob(
                                    Vec3::new(side * 0.05, 0.085 + k as f32 * 0.02, 0.124),
                                    Vec3::splat(0.006),
                                    0,
                                    0.0,
                                    DARK,
                                );
                            }
                        }
                        native_helm_rim(&mut p.metal, &helm_shell);
                        p.metal.absorb(helm_shell);
                        if fem {
                            braid(p, Vec3::new(0.0, 0.05, -0.12), hair_color(l));
                        }
                        // A plume of the house colour sweeping back from the crest.
                        for k in 0..6 {
                            let spread = (k as f32 - 2.5) * 0.012;
                            let base = Vec3::new(spread, 0.33, 0.03);
                            tube(
                                &mut p.matte,
                                &[
                                    base,
                                    base + Vec3::new(0.0, 0.09, -0.04),
                                    base + Vec3::new(spread, 0.1, -0.16),
                                    base + Vec3::new(spread * 2.0, 0.02, -0.26),
                                ],
                                &[
                                    (0.016, 0.022),
                                    (0.018, 0.026),
                                    (0.013, 0.018),
                                    (0.002, 0.002),
                                ],
                                l.cloth * (0.9 + 0.04 * k as f32),
                            );
                        }
                    }
                    Headwear::HoodDown(hood) => {
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut cowl = Sculpt::default();
                            cowl.add(
                                Ellipsoid(Vec3::new(0.0, 0.0, -0.12), Vec3::new(0.12, 0.07, 0.06)),
                                0.0,
                            );
                            for side in [-1.0f32, 1.0] {
                                cowl.add(
                                    Ellipsoid(
                                        Vec3::new(side * 0.1, -0.02, -0.06),
                                        Vec3::new(0.05, 0.05, 0.07),
                                    ),
                                    0.05,
                                );
                            }
                            // The hollow of the hood, open toward the neck.
                            cowl.carve(
                                Ellipsoid(
                                    Vec3::new(0.0, 0.03, -0.1),
                                    Vec3::new(0.085, 0.045, 0.04),
                                ),
                                0.02,
                            );
                            cowl.mesh(
                                &mut p.matte,
                                Vec3::new(-0.2, -0.1, -0.22),
                                Vec3::new(0.2, 0.1, 0.02),
                                0.007,
                                &|at, _| hood * 0.95 * crate::sdf::Sculpt::weave(at),
                            );
                        }
                    }
                    Headwear::Hood {
                        color: hood,
                        pointed,
                    } => {
                        // A deep hood: the shell rounds over the head, a thick
                        // rolled rim frames the face, and the cloth drapes down
                        // the back of the neck onto the shoulders.
                        // One sculpted piece of cloth: a shell rounding over the
                        // head, the face opening cut cleanly with only a soft
                        // thickness at its edge, the cloth falling behind the
                        // neck onto the shoulders.
                        {
                            use crate::sdf::{Sculpt, Shape::*};
                            let mut h = Sculpt::default();
                            h.add(
                                Ellipsoid(
                                    Vec3::new(0.0, 0.195, -0.035),
                                    Vec3::new(0.138, 0.165, 0.145),
                                ),
                                0.0,
                            )
                            .add(
                                Ellipsoid(Vec3::new(0.0, 0.07, -0.11), Vec3::new(0.12, 0.13, 0.07)),
                                0.05,
                            )
                            .add(
                                Ellipsoid(
                                    Vec3::new(0.0, -0.03, -0.09),
                                    Vec3::new(0.16, 0.05, 0.09),
                                ),
                                0.05,
                            )
                            // The face opening, tilted to follow the face.
                            .carve(
                                Ellipsoid(Vec3::new(0.0, 0.14, 0.13), Vec3::new(0.088, 0.13, 0.11)),
                                0.012,
                            )
                            // The underside around the neck and the front of the shoulders.
                            .carve(
                                Ellipsoid(Vec3::new(0.0, -0.06, 0.1), Vec3::new(0.2, 0.08, 0.14)),
                                0.02,
                            );
                            h.mesh(
                                &mut p.matte,
                                Vec3::new(-0.2, -0.1, -0.22),
                                Vec3::new(0.2, 0.38, 0.14),
                                0.0065,
                                &|_at, n| {
                                    // A slightly darker edge where the cloth turns inward.
                                    (if n.z > 0.6 { hood * 0.88 } else { hood })
                                        * crate::sdf::Sculpt::weave(_at)
                                },
                            );
                        }
                        if let Some(trim) = hood_binding {
                            equipment_outerwear::emit_hood_rim(p, hood, trim);
                        }
                        if fem || style == Style::Wraith {
                            // Locks of hair escape the hood to frame the face.
                            let c = hair_color(l);
                            for side in [-1.0f32, 1.0] {
                                lock(
                                    &mut p.matte,
                                    &[
                                        Vec3::new(side * 0.075, 0.2, 0.07),
                                        Vec3::new(side * 0.088, 0.13, 0.07),
                                        Vec3::new(side * 0.08, 0.05, 0.06),
                                    ],
                                    0.026,
                                    0.016,
                                    c,
                                );
                            }
                        }
                        // Some hoods end in a long soft point that flops back.
                        if pointed {
                            tube(
                                &mut p.matte,
                                &[
                                    Vec3::new(0.0, 0.3, -0.06),
                                    Vec3::new(0.0, 0.37, -0.14),
                                    Vec3::new(0.0, 0.33, -0.24),
                                    Vec3::new(0.0, 0.22, -0.27),
                                ],
                                &[(0.075, 0.06), (0.05, 0.045), (0.03, 0.028), (0.004, 0.004)],
                                hood * 0.95,
                            );
                        }
                    }
                    Headwear::WingedHelm { metal, feathers } => {
                        headwear_parts::winged_helm(p, metal, feathers);
                        // The crown is tucked under the cap; a braid emerges
                        // below its rear rim, preserving the wearer's hair color.
                        if fem && grows_scalp_hair(l) {
                            headwear_parts::winged_helm_braid(p, hair_color(l));
                        }
                    }
                    Headwear::Crown { metal, gem } => headwear_parts::crown(p, metal, gem),
                    Headwear::Bare => {}
                    Headwear::Tricorn { color, trim } => headwear_parts::tricorn(p, color, trim),
                    Headwear::WitchHat { color, band } => headwear_parts::witch_hat(p, color, band),
                    Headwear::Wreath(color) => headwear_parts::wreath(p, color),
                    Headwear::LeafCrown(color) => headwear_parts::leaf_crown(p, color),
                    Headwear::Circlet { color, gem, broad } => {
                        headwear_parts::circlet(p, color, gem, broad)
                    }
                    Headwear::Veiled { cap, veil } => headwear_parts::veiled(p, cap, veil),
                }
                if let Some(recipe) = earrings {
                    accessory_parts::earrings(
                        p,
                        recipe,
                        &face_shape(l, !matches!(l.style, Style::Hobgoblin | Style::Troll)).form,
                        EarAnatomy::default(),
                    );
                }
                match style {
                    Style::Hero => {}
                    Style::Hobgoblin => {
                        for side in [-1.0f32, 1.0] {
                            p.matte.prism(
                                Vec3::new(side * 0.1, 0.17, 0.0),
                                Vec3::new(side * 0.26, 0.23, -0.05),
                                0.035,
                                0.0,
                                5,
                                l.skin,
                                0.0,
                            );
                            p.matte.prism(
                                Vec3::new(side * 0.03, 0.08, 0.1),
                                Vec3::new(side * 0.035, 0.12, 0.11),
                                0.01,
                                0.0,
                                3,
                                BONE,
                                0.0,
                            );
                        }
                        // A big ruddy nose and a stiff dark mohawk, or, for her, a wild topknot.
                        p.matte.blob(
                            Vec3::new(0.0, 0.132, 0.115),
                            Vec3::new(0.04, 0.031, 0.045),
                            2,
                            0.0,
                            Vec3::new(0.78, 0.36, 0.3),
                        );
                        if fem {
                            let c = hair_color(l);
                            p.matte.blob(
                                Vec3::new(0.0, 0.28, -0.04),
                                Vec3::new(0.035, 0.03, 0.035),
                                1,
                                0.0,
                                c,
                            );
                            for k in 0..5 {
                                let a = k as f32 * 1.25;
                                lock(
                                    &mut p.matte,
                                    &[
                                        Vec3::new(0.0, 0.29, -0.04),
                                        Vec3::new(a.cos() * 0.05, 0.33, -0.05 + a.sin() * 0.05),
                                        Vec3::new(a.cos() * 0.1, 0.27, -0.06 + a.sin() * 0.1),
                                    ],
                                    0.024,
                                    0.014,
                                    c,
                                );
                            }
                        }
                        for k in 0..(if fem { 0 } else { 6 }) {
                            let z = 0.06 - k as f32 * 0.045;
                            let root = Vec3::new(0.0, 0.25 - (k as f32 - 2.0).powi(2) * 0.004, z);
                            tube(
                                &mut p.matte,
                                &[root, root + Vec3::new(0.0, 0.06, -0.03)],
                                &[(0.012, 0.03), (0.002, 0.004)],
                                DARK * 3.0,
                            );
                        }
                    }
                    Style::Zombie => {
                        p.matte.blob(
                            Vec3::new(-0.06, 0.2, 0.08),
                            Vec3::new(0.025, 0.02, 0.015),
                            2,
                            0.3,
                            Vec3::new(0.35, 0.12, 0.1),
                        );
                    }
                    Style::Vampire => {
                        p.matte.prism(
                            Vec3::new(0.0, 0.25, 0.07),
                            Vec3::new(0.0, 0.18, 0.11),
                            0.03,
                            0.0,
                            4,
                            DARK,
                            0.0,
                        );
                    }
                    _ => rogue::head_details(p, l),
                }
            }
        }
    }
}

const NATIVE_HELM_OPENING: f32 = 0.04;

fn native_helm_shell(b: &mut Builder) {
    tube(
        b,
        &[
            Vec3::Y * NATIVE_HELM_OPENING,
            Vec3::Y * 0.14,
            Vec3::Y * 0.26,
            Vec3::Y * 0.31,
        ],
        &[(0.128, 0.132), (0.136, 0.14), (0.13, 0.134), (0.11, 0.112)],
        STEEL,
    );
    b.blob(Vec3::Y * 0.3, Vec3::new(0.11, 0.05, 0.112), 0, 0.0, STEEL);
}

fn native_helm_rim(b: &mut Builder, shell: &Builder) {
    let profile = radial_parts::RoundedBandProfile {
        height: NATIVE_HELM_OPENING + 0.014,
        half_height: 0.014,
        clearance: 0.0017,
        radial_half: 0.00565,
        sides: 40,
        sections: 8,
    };
    let color = Vec3::new(0.8, 0.62, 0.3);
    let Some(rim) = profile.resolve(
        |y, angle, gap| {
            let origin = Vec3::Y * y;
            let out = Vec3::new(angle.sin(), 0.0, angle.cos());
            shell.outer_surface(origin, out).map(|hit| hit + out * gap)
        },
        color,
    ) else {
        return;
    };
    for triangle in rim.surface_triangles() {
        b.tri(triangle, color);
    }
}

#[cfg(test)]
mod study_hood_tests {
    use super::*;

    #[test]
    fn study_hood_binding_is_a_head_cache_dependency() {
        let mut base = Look::of(CreatureModel::Human, 17);
        let kind = super::super::super::outfit::outerwear::OuterwearAppearance::HoodedCloak;
        let mut outfit = Outfit::native(&base);
        outfit.head = Headwear::Hood {
            color: kind.recipe().cloth,
            pointed: false,
        };
        base.outfit = Some(outfit);
        let native = Recipe::resolve(&base);
        let mut equipment = super::super::super::outfit::Equipment::default();
        equipment.select(kind.name()).unwrap();
        let equipped = base.clone().equipped(equipment);
        let study = Recipe::resolve(&equipped);
        assert_eq!(native.finish.headwear, study.finish.headwear);
        assert_eq!(native.finish.hood_binding, None);
        assert_eq!(study.finish.hood_binding, Some(kind.recipe().trim));
        assert_ne!(native.key(&base), study.key(&equipped));
    }
}

#[cfg(test)]
mod earring_tests {
    use super::*;

    // Index count measures emitted triangles; final vertex deduplication is not additive.
    fn mesh_index_count(parts: Parts) -> usize {
        let mut materials = Assets::<StandardMaterial>::default();
        let mut meshes = Assets::<Mesh>::default();
        let model = parts.finish(
            &mut meshes,
            &crate::props::PropMaterials::new(&mut materials),
        );
        model
            .0
            .iter()
            .map(|(mesh, _)| meshes.get(mesh).unwrap().indices().unwrap().len())
            .sum()
    }

    fn head_count(look: &Look) -> usize {
        let mut parts = Parts::default();
        Recipe::resolve(look).emit(&mut parts, look, HumanoidBody::resolve(look));
        mesh_index_count(parts)
    }

    #[test]
    fn leaf_crown_is_single_replaceable_headwear_owner() {
        for woman in [false, true] {
            let native = Look::with_choice(
                CreatureModel::Nymph,
                17,
                Some(Variant { woman, ..default() }),
            );
            assert!(matches!(Outfit::worn(&native).head, Headwear::LeafCrown(_)));
            let royal = native.clone().dressed(Wardrobe::RoyalPrincess);
            if woman {
                assert!(matches!(Outfit::worn(&royal).head, Headwear::Crown { .. }));
            }
            let mut bare = native.clone();
            let mut outfit = Outfit::worn(&bare);
            outfit.head = Headwear::Bare;
            bare.outfit = Some(outfit);
            let mut leaves = Parts::default();
            headwear_parts::leaf_crown(&mut leaves, native.cloth);
            assert_eq!(
                head_count(&native) - head_count(&bare),
                mesh_index_count(leaves)
            );
            assert_ne!(
                Recipe::resolve(&native).key(&native),
                Recipe::resolve(&bare).key(&bare)
            );
            assert_ne!(
                Recipe::resolve(&native).key(&native),
                Recipe::resolve(&royal).key(&royal)
            );
        }
    }

    #[test]
    fn nymph_anatomy_does_not_emit_removable_leaf_crown() {
        let native = Look::with_choice(
            CreatureModel::Nymph,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let mut covered = native.clone();
        let mut outfit = Outfit::worn(&native);
        outfit.head = Headwear::Helm;
        covered.outfit = Some(outfit);
        let count = |look: &Look| {
            let mut parts = Parts::default();
            rogue::head_details(&mut parts, look);
            mesh_index_count(parts)
        };
        assert!(count(&covered) > 0, "pointed ears remain anatomy");
        assert_eq!(
            count(&native),
            count(&covered),
            "removable crown must not be emitted by anatomy"
        );
    }

    #[test]
    fn native_earrings_have_one_outfit_owner_in_actual_meshes() {
        for model in [CreatureModel::Nymph, CreatureModel::Medusa] {
            let native = Look::with_choice(
                model,
                17,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            );
            let mut anatomy = Parts::default();
            rogue::head_details(&mut anatomy, &native);
            let metal_only = Parts {
                metal: anatomy.metal,
                ..default()
            };
            assert_eq!(
                mesh_index_count(metal_only),
                0,
                "{model:?}: anatomy must not emit outfit jewelry"
            );
            for look in [
                native.clone(),
                native.clone().dressed(Wardrobe::RoyalPrincess),
                native.clone().dressed(Wardrobe::SylvanDuelist),
            ] {
                let mut outfit = Outfit::worn(&look);
                let selected = outfit.trimmings.earrings.expect("selected pair");
                let mut pair = Parts::default();
                accessory_parts::earrings(
                    &mut pair,
                    selected,
                    &face_shape(
                        &look,
                        !matches!(look.style, Style::Hobgoblin | Style::Troll),
                    )
                    .form,
                    EarAnatomy::default(),
                );
                let pair_indices = mesh_index_count(pair);
                assert!(pair_indices > 0);
                outfit.trimmings.earrings = None;
                let mut bare = look.clone();
                bare.outfit = Some(outfit);
                assert_eq!(
                    head_count(&look) - head_count(&bare),
                    pair_indices,
                    "{model:?}: exactly one selected pair"
                );
                assert_ne!(
                    Recipe::resolve(&look).key(&look),
                    Recipe::resolve(&bare).key(&bare)
                );
            }
            let man = Look::with_choice(
                model,
                17,
                Some(Variant {
                    woman: false,
                    ..default()
                }),
            );
            assert!(Outfit::worn(&man).trimmings.earrings.is_none());
        }
    }

    #[test]
    fn earring_recipe_kind_and_material_are_head_cache_inputs() {
        let base = Look::with_choice(
            CreatureModel::Nymph,
            17,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let mut keys = std::collections::HashSet::new();
        for earrings in [
            None,
            Some(Earrings::Drops { metal: Vec3::ONE }),
            Some(Earrings::Drops {
                metal: Vec3::splat(0.5),
            }),
            Some(Earrings::Hoops { metal: Vec3::ONE }),
            Some(Earrings::GemDrops {
                metal: Vec3::ONE,
                gem: Vec3::X,
            }),
            Some(Earrings::GemDrops {
                metal: Vec3::ONE,
                gem: Vec3::Y,
            }),
        ] {
            let mut look = base.clone();
            let mut outfit = Outfit::native(&look);
            outfit.trimmings.earrings = earrings;
            look.outfit = Some(outfit);
            assert!(keys.insert(Recipe::resolve(&look).key(&look)));
        }
    }
}

#[cfg(test)]
mod native_helm_rim_tests {
    use super::*;
    #[test]
    fn native_helm_rim_upper_edge_clears_actual_shell() {
        let mut shell = Parts::default().metal;
        native_helm_shell(&mut shell);
        let mut rim = shell.empty_like();
        native_helm_rim(&mut rim, &shell);
        let tris: Vec<_> = rim.surface_triangles().collect();
        let top = tris
            .iter()
            .flatten()
            .map(|p| p.y)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut count = 0;
        let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
        let mut worst = Vec3::ZERO;
        for p in tris.iter().flatten().filter(|p| (p.y - top).abs() < 1e-6) {
            let origin = Vec3::Y * p.y;
            let out = (*p - origin).normalize();
            let hit = shell
                .outer_surface(origin, out)
                .expect("upper rim requires shell support");
            let gap = (*p - hit).dot(out);
            if gap < min {
                min = gap;
                worst = *p;
            }
            max = max.max(gap);
            count += 1;
        }
        eprintln!(
            "native helm actual upper-edge witnesses={count} min={min} max={max} worst={worst:?}"
        );
        assert!(count >= 18);
        assert!(
            min >= 0.0,
            "gold upper edge intersects shell: min={min} worst={worst:?}"
        );
        assert!(
            max <= 0.018,
            "gold upper edge stands off too far: max={max}"
        );
    }
}

#[cfg(test)]
mod native_helm_rim_controls {
    use super::*;
    #[test]
    fn native_helm_shell_recipe_is_unchanged() {
        let mut actual = Parts::default().metal;
        let mut expected = actual.empty_like();
        native_helm_shell(&mut actual);
        tube(
            &mut expected,
            &[
                Vec3::Y * 0.04,
                Vec3::Y * 0.14,
                Vec3::Y * 0.26,
                Vec3::Y * 0.31,
            ],
            &[(0.128, 0.132), (0.136, 0.14), (0.13, 0.134), (0.11, 0.112)],
            STEEL,
        );
        expected.blob(Vec3::Y * 0.3, Vec3::new(0.11, 0.05, 0.112), 0, 0.0, STEEL);
        actual.assert_same_recipe(&expected);
    }
    #[test]
    fn native_helm_rim_is_supported_with_bounded_relief() {
        let mut shell = Parts::default().metal;
        native_helm_shell(&mut shell);
        let mut rim = shell.empty_like();
        native_helm_rim(&mut rim, &shell);
        let triangles: Vec<_> = rim.surface_triangles().collect();
        assert_eq!(
            triangles.len(),
            640,
            "complete closed-band topology required"
        );
        let key = |p: Vec3| p.to_array().map(|v| (v * 1e7).round() as i64);
        let mut edges = std::collections::BTreeMap::new();
        for tri in &triangles {
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                let (a, b) = (key(a), key(b));
                assert_ne!(a, b, "degenerate rim edge");
                let (pair, sign) = if a < b { ((a, b), 1) } else { ((b, a), -1) };
                let count = edges.entry(pair).or_insert((0usize, 0i32));
                count.0 += 1;
                count.1 += sign;
            }
        }
        assert!(
            edges
                .values()
                .all(|&(count, winding)| count == 2 && winding == 0),
            "rim must be closed with consistently wound shared edges"
        );
        let (mut min_gap, mut max_gap) = (f32::INFINITY, f32::NEG_INFINITY);
        let mut worst = Vec3::ZERO;
        let mut samples = 0usize;
        for tri in triangles {
            for point in [
                tri[0],
                tri[1],
                tri[2],
                (tri[0] + tri[1]) * 0.5,
                (tri[1] + tri[2]) * 0.5,
                (tri[2] + tri[0]) * 0.5,
                (tri[0] + tri[1] + tri[2]) / 3.0,
            ] {
                assert!(point.is_finite());
                assert!((0.04 - 1e-6..=0.068 + 1e-6).contains(&point.y));
                let origin = Vec3::Y * point.y;
                let outward = (point - origin).normalize();
                let support = shell
                    .outer_surface(origin, outward)
                    .expect("supported full band");
                let gap = (point - support).dot(outward);
                if gap < min_gap {
                    min_gap = gap;
                    worst = point;
                }
                max_gap = max_gap.max(gap);
                samples += 1;
            }
        }
        eprintln!("whole-band samples={samples} min={min_gap} max={max_gap} worst={worst:?}");
        assert!(
            min_gap >= 0.0009,
            "rim minimum relief {min_gap} at {worst:?}"
        );
        assert!(max_gap <= 0.0131, "rim maximum relief {max_gap}");
    }
}
