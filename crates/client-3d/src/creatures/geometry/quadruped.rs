use super::*;

/// The actual rider support, shared by sculpt emission and fitted attachments.
fn centaur_torso_support(l: &Look) -> crate::sdf::Sculpt {
    let core = super::anatomy::TorsoCoreProfile::centaur(l);
    let bust = super::anatomy::centaur_bust(l);
    // Sculpted, as the people's torsos are: the waist rising out
    // of the withers into ribs, chest and rounded shoulders.
    use crate::sdf::{Sculpt, Shape::*};
    let mut body = Sculpt::default();
    body.add(
        // The lower human root sits inside the withers instead
        // of hanging out over the horse's breast below the belt.
        Ellipsoid(Vec3::new(0.0, -0.1, -0.055), Vec3::new(0.15, 0.15, 0.12)),
        0.0,
    )
    .add(Ellipsoid(core.waist.0, core.waist.1), 0.06)
    .add(Ellipsoid(core.chest.ribs.0, core.chest.ribs.1), 0.06)
    .add(Ellipsoid(core.chest.upper.0, core.chest.upper.1), 0.14)
    .add(
        Capsule(
            Vec3::new(0.0, 0.5, -0.005),
            Vec3::new(0.0, 0.6, -0.005),
            0.065,
            0.055,
        ),
        0.04,
    );
    // Deltoids belong to the animated arms; the rib/clavicle
    // support stays on the torso without retaining lateral caps.
    if let Some(bust) = bust {
        bust_supports(&mut body, &bust, 0.0);
    } else {
        // The male rider has the same broad continuous chest
        // plane as the bipeds, with no paired forward cups.
        body.add(
            Ellipsoid(Vec3::new(0.0, 0.38, 0.015), Vec3::new(0.185, 0.1, 0.1)),
            0.04,
        )
        .add(
            Ellipsoid(Vec3::new(0.0, 0.17, 0.075), Vec3::new(0.07, 0.09, 0.04)),
            0.04,
        );
    }
    body
}

/// One authored boundary owns both the painted wrap and its sewn edges.
/// Fit is independent of this rider-specific sweetheart style.
#[derive(Clone, Copy)]
struct CentaurChestWrap {
    low: f32,
    high: f32,
    dip: f32,
    apex_x: f32,
}

impl CentaurChestWrap {
    fn new(bust: Bust) -> Self {
        let apex = bust.apex(1.0);
        Self {
            low: bust.fold - 0.05,
            high: apex.y + bust.radii.y * 0.62,
            dip: apex.y - 0.005,
            apex_x: apex.x,
        }
    }

    fn edges(self, x: f32) -> (f32, f32) {
        let t = (x.abs() / self.apex_x).min(1.6);
        let high = if t < 1.0 {
            self.dip + (self.high - self.dip) * (1.0 - (1.0 - t).powi(2))
        } else {
            self.high - (t - 1.0) * 0.06
        };
        (self.low, high)
    }

    fn covers(self, at: Vec3) -> bool {
        let (low, high) = self.edges(at.x);
        at.y > low && at.y < if at.z > 0.0 { high } else { 0.41 }
    }

    fn trim(self, support: &crate::sdf::Sculpt, top: bool) -> FittedStrap {
        BoundaryTrimProfile {
            half_width: 0.012,
            support_fraction: 0.94,
            terminal_width_fraction: 0.2,
            taper_start: 0.75,
        }
        .fit(
            support,
            &|x| {
                let (low, high) = self.edges(x);
                if top {
                    high
                } else {
                    low
                }
            },
            true,
            // Reserve the ribbon's full thickness plus a small seam allowance.
            0.011,
        )
    }

    fn emit_trim(self, b: &mut Builder, support: &crate::sdf::Sculpt) {
        for top in [true, false] {
            self.trim(support, top).emit(b, LEATHER * 0.7, true);
        }
    }
}

#[cfg(test)]
#[path = "chest_wrap_tests.rs"]
mod chest_wrap_tests;

fn centaur_waist_belt(b: &mut Builder, support: &crate::sdf::Sculpt) {
    radial_parts::RadialBandProfile {
        heights: Vec2::new(0.015, 0.055),
        support_to_output_y: 0.0,
        tilt: 0.0,
        clearance: 0.004,
        thickness: 0.003,
        rows: 4,
        sides: 48,
    }
    .emit(b, support, LEATHER * 0.72);
}

fn centaur_forearm_band(b: &mut Builder, width: f32, muscle: f32) {
    let support = forearm_form(width, 1.0, muscle);
    radial_parts::RadialBandProfile {
        heights: Vec2::new(-0.25, -0.13),
        support_to_output_y: 0.0,
        tilt: 0.0,
        clearance: 0.003 * width,
        thickness: 0.003 * width,
        rows: 6,
        sides: 24,
    }
    .emit(b, &support, LEATHER);
}

/// Blue-violet charcoal keeps a black coat readable under ordinary light.
/// The dorsal planes carry a little more pigment value; this is not emission.
pub(super) fn equine_hide(base: Vec3, style: Style, normal: Vec3) -> Vec3 {
    if style == Style::Unicorn {
        let dorsal = (normal.y * 0.5 + 0.5).clamp(0.0, 1.0);
        base.lerp(Vec3::new(0.23, 0.25, 0.34), dorsal * 0.3)
    } else {
        base
    }
}

/// Horses and their kin; with `centaur`, a rider's torso rises from the withers.
pub(super) fn quadruped(kit: &mut Kit, lift: Entity, look: &Look, centaur: bool) {
    let l = look.clone();
    let style = l.style;
    let quagga = style == Style::Quagga;
    // All four unposed soles meet the floor; movement crouches this linked
    // rest height rather than inheriting unrelated horse-length dimensions.
    let equine = super::anatomy::EquineBody::resolve(&l);
    let body_height = super::anatomy::EquineBody::BODY_HEIGHT;
    kit.rig.ankle_floor = super::anatomy::EquineBody::ANKLE_HEIGHT;
    kit.rig.legs = (super::anatomy::EquineBody::upper(true).length()
        + super::anatomy::EquineBody::lower(true).length())
        / 0.84;
    // Shorter supports take shorter distance steps at the same travel speed.
    kit.rig.stride = kit.rig.stride.max(0.85 / kit.rig.legs);
    kit.rig.girth = 0.32 * equine.breadth;
    // A quagga's coat: chestnut behind, cream and brown stripes over the
    // forequarters, neck and face, a pale belly. Painted, not banded.
    let coat = move |dir: Vec3, at: Vec3| -> Vec3 {
        if !quagga {
            return equine_hide(l.skin, style, dir);
        }
        let dark = Vec3::new(0.36, 0.2, 0.1);
        let cream = l.cloth;
        // A cream chest and belly; stripes band the sides.
        if dir.y < -0.5 || (dir.z > 0.45 && at.z > 0.2) {
            return cream;
        }
        let front = ease((at.z + 0.1) / 0.5);
        // Each quagga's stripes are its own: spacing, slant and phase.
        let pv = f32::from(l.v.pattern);
        let wave = (at.z * (30.0 + 3.0 * pv)
            + at.y * (4.0 + 2.5 * (pv - 1.5))
            + (at.y * 5.0).sin() * 1.2
            + pv * 1.7)
            .sin();
        let striped = cream.lerp(dark, ((wave - 0.15) * 6.0).clamp(0.0, 1.0));
        l.skin.lerp(striped, front)
    };
    let body = kit.joint(
        J::Body,
        lift,
        Vec3::new(0.0, body_height, 0.0),
        "body",
        |p| {
            // One continuous hide over the rib cage, scapulae and haunches. The
            // limbs emerge inside these masses; separate round shoulder patches
            // make every horse, unicorn and centaur look assembled from toys.
            use crate::sdf::{Sculpt, Shape::*};
            let wide = Vec3::new(equine.breadth, 1.0, 1.0);
            let mut barrel = Sculpt::default();
            barrel
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.015, -0.015),
                        Vec3::new(0.215, 0.265, 0.43) * wide,
                    ),
                    0.0,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, -0.055, 0.30),
                        Vec3::new(0.18, 0.245, 0.20) * wide,
                    ),
                    0.075,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, 0.025, -0.36),
                        Vec3::new(0.225, 0.245, 0.215) * wide,
                    ),
                    0.075,
                )
                .add(
                    Ellipsoid(
                        Vec3::new(0.0, 0.18, 0.25),
                        Vec3::new(0.105, 0.09, 0.19) * wide,
                    ),
                    0.06,
                );
            for side in [-1.0f32, 1.0] {
                barrel
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.09 * equine.breadth, -0.10, 0.34),
                            Vec3::new(0.11, 0.18, 0.14) * wide,
                        ),
                        0.05,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.11 * equine.breadth, 0.015, 0.29),
                            Vec3::new(0.125, 0.22, 0.17) * wide,
                        ),
                        0.075,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.125 * equine.breadth, 0.015, -0.35),
                            Vec3::new(0.14, 0.24, 0.205) * wide,
                        ),
                        0.075,
                    );
            }
            barrel.mesh(
                &mut p.matte,
                Vec3::new(-0.37, -0.38, -0.65),
                Vec3::new(0.37, 0.34, 0.62),
                0.012,
                &|at, normal| coat(normal, at),
            );
        },
    );
    let neck_at = Vec3::new(0.0, 0.12, 0.45);
    if centaur {
        let torso = kit.joint(J::Torso, body, Vec3::new(0.0, 0.15, 0.42), "torso", |p| {
            // One torso rising out of the withers: waist, ribs, chest.
            let core = super::anatomy::TorsoCoreProfile::centaur(&l);
            // Her chest is fitted to the rider's own rib cage, as a biped's is.
            let bust = super::anatomy::centaur_bust(&l);
            let wrap = bust.map(CentaurChestWrap::new);
            if let Some(bust) = bust {
                for side in [-1.0f32, 1.0] {
                    let (center, radii) = bust.mass(side, 0.0);
                    p.soft.push(crate::props::SoftRegion {
                        center,
                        radii,
                        facing: 1.0,
                        give: 0.45 * (0.5 + 0.6 * bust.volume),
                    });
                }
            }
            let body_shape = {
                let body = centaur_torso_support(&l);
                body.mesh(
                    &mut p.matte,
                    Vec3::new(-0.34, -0.3, -0.22),
                    Vec3::new(0.34, 0.64, 0.27),
                    0.0085,
                    &|at, _| {
                        // The wrapped leather follows the sculpted chest and
                        // waist, preserving the figure instead of a rigid tube.
                        let wrapped = wrap.is_some_and(|wrap| wrap.covers(at));
                        if at.y < 0.035 {
                            // The coat transition is concealed by the belt;
                            // any overlap below it belongs to the equine body.
                            l.skin
                        } else if wrapped {
                            LEATHER
                        } else {
                            l.cloth
                        }
                    },
                );
                body
            };
            if let Some(wrap) = wrap {
                wrap.emit_trim(&mut p.matte, &body_shape);
            }
            // Body-specific knots and clasp style, shared fitted attachment path.
            let path = [
                (core.waist_anchor(-0.7), 0.026),
                (core.chest_anchor(-0.275) - Vec2::Y * 0.06, 0.026),
                (core.chest_anchor(0.2) + Vec2::Y * 0.07, 0.026),
                (
                    Vec2::new(core.chest.upper.1.x * 0.75, core.shoulder.y),
                    0.025,
                ),
            ];
            for front in [false, true] {
                let strap = FittedStrap::fit(
                    &body_shape,
                    &path,
                    front,
                    0.01,
                    StrapEdgePolicy::EndUnsupportedRows,
                );
                strap.emit(&mut p.satin, LEATHER * 0.72, true);
                if front {
                    if let Some(mount) = strap.at_height(path[2].0.y) {
                        p.metal.blob(
                            mount.position + mount.normal * 0.008,
                            Vec3::splat(0.018),
                            0,
                            0.0,
                            Vec3::new(0.74, 0.5, 0.26),
                        );
                    }
                }
            }
            centaur_waist_belt(&mut p.satin, &body_shape);
        });
        kit.joint(J::Head, torso, Vec3::Y * 0.6, "head", |p| {
            // The rider's face and hair are a person's; men wear full beards.
            let rider = Look {
                skin: l.cloth,
                ..l.clone()
            };
            biped_neck(&mut p.matte, 1.0, neck_slim(rider.v.woman), l.cloth);
            p.set_transform(
                Affine3A::from_translation(Vec3::Y * 0.08)
                    * Affine3A::from_scale(Vec3::splat(1.1))
                    * Affine3A::from_translation(Vec3::Y * -0.08),
            );
            human_face(p, &rider, true);
            if let Some(kind) = l.equipment.headwear() {
                equipment_headwear::emit(p, kind);
                equipment_hair(p, &rider, kind);
            } else {
                hair(p, &rider, true);
            }
            if !rider.v.woman {
                beard(p, hair_color(&rider));
            }
        });
        let arms = super::anatomy::RiderArms::resolve(&l);
        for (side, arm, fore, hand) in [
            (1.0f32, J::ArmL, J::ForeL, J::HandL),
            (-1.0, J::ArmR, J::ForeR, J::HandR),
        ] {
            let tag = if side > 0.0 { "l" } else { "r" };
            let upper = kit.joint(
                arm,
                torso,
                super::anatomy::TorsoCoreProfile::centaur(&l).shoulder_seat(side),
                &format!("arm{tag}"),
                |p| {
                    organic_upper_arm(
                        &mut p.matte,
                        side,
                        arms.width,
                        1.0,
                        arms.muscle,
                        true,
                        l.cloth,
                    );
                },
            );
            let lower = kit.joint(fore, upper, Vec3::Y * -0.3, &format!("fore{tag}"), |p| {
                organic_forearm(&mut p.matte, arms.width, 1.0, arms.muscle, true, l.cloth);
                centaur_forearm_band(&mut p.satin, arms.width, arms.muscle);
                if let Some(gloves) = l.equipment.hands {
                    equipment_gloves::emit_cuff(p, gloves, Vec3::Y * -0.3, arms.width, 1.0);
                    equipment_gloves::emit_hand(
                        p,
                        gloves,
                        Vec3::Y * -0.3,
                        side,
                        arms.width,
                        false,
                        false,
                    );
                } else {
                    fist(p, Vec3::Y * -0.3, side, arms.width, l.cloth);
                }
            });
            let grip = kit
                .commands
                .spawn((
                    Transform::from_translation(Vec3::Y * -0.31),
                    Visibility::default(),
                ))
                .id();
            kit.commands.entity(lower).add_child(grip);
            kit.rig.joints[hand as usize] = Some(grip);
            // Without its rest offset the grip would be posed at the elbow.
            kit.rig.rest[hand as usize] = Vec3::Y * -0.31;
        }
        let hand = kit.rig.joint(J::HandR).unwrap();
        kit.attach(
            hand,
            Transform::from_rotation(Quat::from_rotation_x(FRAC_PI_2)),
            "spear",
            |p| {
                p.matte
                    .prism(Vec3::Y * -0.7, Vec3::Y * 0.9, 0.02, 0.02, 6, WOOD, 0.0);
                p.satin
                    .prism(Vec3::Y * 0.78, Vec3::Y * 0.9, 0.024, 0.024, 6, LEATHER, 0.0);
                p.metal
                    .prism(Vec3::Y * 0.9, Vec3::Y * 1.14, 0.045, 0.0, 4, STEEL, 0.0);
                p.matte.prism(
                    Vec3::Y * 0.86,
                    Vec3::new(0.05, 0.7, 0.0),
                    0.012,
                    0.0,
                    3,
                    Vec3::new(0.6, 0.15, 0.1),
                    0.0,
                );
            },
        );
    } else {
        let neck = kit.joint(J::Neck, body, neck_at, "neck", |p| {
            let top = Vec3::new(0.0, 0.36, 0.3);
            let neck_coat = |dir: Vec3, at: Vec3| -> Vec3 {
                if quagga {
                    let wave = (at.y * 52.0 + at.z * 12.0).sin();
                    l.cloth.lerp(
                        Vec3::new(0.38, 0.21, 0.11),
                        ((wave - 0.05) * 3.0).clamp(0.0, 1.0),
                    )
                } else {
                    equine_hide(l.skin, style, dir)
                }
            };
            // The crest, throat and root form one angled neck. Its broad
            // root continues into the chest, covering the articulated join.
            use crate::sdf::{Sculpt, Shape::*};
            let mut neck_shape = Sculpt::default();
            neck_shape
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.045, -0.06), Vec3::new(0.125, 0.18, 0.165)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.1, 0.065), Vec3::new(0.112, 0.19, 0.15)),
                    0.055,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.245, 0.19), Vec3::new(0.085, 0.15, 0.12)),
                    0.045,
                )
                .add(Ellipsoid(top, Vec3::new(0.074, 0.095, 0.09)), 0.035);
            neck_shape.mesh(
                &mut p.matte,
                Vec3::new(-0.17, -0.25, -0.25),
                Vec3::new(0.17, 0.49, 0.42),
                0.009,
                &|at, normal| {
                    if quagga && normal.z > 0.7 && at.y < 0.32 {
                        l.cloth
                    } else {
                        neck_coat(normal, at)
                    }
                },
            );
            let mane = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            // The mane: locks along the crest. A unicorn's falls long and
            // sweeps to one side; a quagga's stands up in a short brush.
            let locks = if style == Style::Unicorn { 20 } else { 18 };
            for k in 0..locks {
                let t = k as f32 / (locks - 1) as f32;
                // Along the crest, the top of the arched neck.
                let crest = Vec3::new(0.0, 0.06 + t * 0.36, -0.16 + t * 0.4);
                let root = neck_shape.onto(crest - Vec3::Z * 0.18, Vec3::Z);
                let shade = mane * (0.8 + 0.35 * rand(5, k));
                if style == Style::Unicorn {
                    let side = if k % 3 == 0 { -1.0 } else { 1.0 };
                    let len = 0.26 + 0.12 * rand(6, k);
                    let curl = 0.03 * (k as f32 * 1.3).sin();
                    // Each lock lifts off the crest, then falls close along the neck.
                    let path = [
                        root,
                        root + Vec3::new(side * 0.03, 0.05, -0.05),
                        root + Vec3::new(side * 0.08, 0.0, -0.09),
                        root + Vec3::new(side * 0.1 + curl, -len * 0.55, -0.1 + 0.05 * rand(7, k)),
                        root + Vec3::new(side * 0.09 - curl + 0.02 * rand(8, k), -len, -0.06),
                    ];
                    let r = 0.03 + 0.012 * rand(9, k);
                    tube(
                        &mut p.matte,
                        &path,
                        &[
                            (r, r * 0.7),
                            (r * 1.1, r * 0.8),
                            (r, r * 0.7),
                            (r * 0.6, r * 0.5),
                            (0.003, 0.003),
                        ],
                        shade,
                    );
                } else {
                    let path = [
                        root,
                        root + Vec3::new(0.0, 0.06, -0.035),
                        root + Vec3::new(0.0, 0.1, -0.06),
                    ];
                    tube(
                        &mut p.matte,
                        &path,
                        &[(0.016, 0.03), (0.014, 0.026), (0.002, 0.004)],
                        shade,
                    );
                }
            }
        });
        kit.joint(J::Snout, neck, Vec3::new(0.0, 0.4, 0.31), "head", |p| {
            let face = |normal: Vec3, at: Vec3| -> Vec3 {
                if quagga && (at.z * 30.0 - at.y * 20.0).sin() > 0.35 && at.z < 0.2 {
                    Vec3::new(0.38, 0.21, 0.11)
                } else if quagga {
                    l.cloth
                } else {
                    equine_hide(l.skin, style, normal)
                }
            };
            // A touch larger than life, as the renders draw it.
            p.set_transform(Affine3A::from_scale(Vec3::splat(1.28)));
            use crate::sdf::{Sculpt, Shape::*};
            let mut skull = Sculpt::default();
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, 0.01, 0.03), Vec3::new(0.093, 0.105, 0.13)),
                    0.0,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.018, 0.16), Vec3::new(0.079, 0.072, 0.16)),
                    0.03,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.075, 0.16), Vec3::new(0.070, 0.047, 0.15)),
                    0.025,
                );
            for side in [-1.0f32, 1.0] {
                skull
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.052, -0.035, 0.03),
                            Vec3::new(0.039, 0.057, 0.077),
                        ),
                        0.025,
                    )
                    .add(
                        Ellipsoid(
                            Vec3::new(side * 0.061, 0.042, 0.057),
                            Vec3::new(0.027, 0.024, 0.043),
                        ),
                        0.016,
                    );
            }
            skull
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.055, 0.29), Vec3::new(0.083, 0.066, 0.095)),
                    0.04,
                )
                .add(
                    Ellipsoid(Vec3::new(0.0, -0.105, 0.235), Vec3::new(0.067, 0.038, 0.12)),
                    0.025,
                );
            let eye_frames = animal_eye_sockets(
                &mut skull,
                Vec3::new(0.0, 0.034, 0.076),
                0.077,
                0.018,
                Vec3::new(0.92, 0.08, 0.38),
            );
            skull.mesh(
                &mut p.matte,
                Vec3::new(-0.14, -0.17, -0.12),
                Vec3::new(0.14, 0.15, 0.405),
                0.006,
                &|at, normal| {
                    if at.z > 0.275 && at.y < -0.025 {
                        if quagga {
                            DARK * 3.0
                        } else {
                            equine_hide(l.skin, style, normal) * 0.88
                        }
                    } else {
                        face(normal, at)
                    }
                },
            );
            for side in [-1.0f32, 1.0] {
                p.matte.blob(
                    skull.onto(Vec3::new(side * 0.043, -0.045, 0.46), -Vec3::Z) + Vec3::Z * 0.003,
                    Vec3::new(0.012, 0.016, 0.01),
                    0,
                    0.0,
                    DARK,
                );
                // Leaf-shaped ears, cupped forward, darker inside.
                let base = Vec3::new(side * 0.045, 0.08, -0.01);
                let ear = [
                    base,
                    base + Vec3::new(side * 0.01, 0.07, 0.0),
                    base + Vec3::new(side * 0.02, 0.14, -0.03),
                ];
                tube(
                    &mut p.matte,
                    &ear,
                    &[(0.034, 0.016), (0.03, 0.014), (0.004, 0.004)],
                    l.skin,
                );
                tube(
                    &mut p.matte,
                    &ear.map(|v| v + Vec3::Z * 0.008),
                    &[(0.022, 0.007), (0.019, 0.007), (0.002, 0.002)],
                    l.skin * 0.55,
                );
            }
            // The forelock spills over the brow.
            let lock = if style == Style::Unicorn {
                l.cloth
            } else {
                l.trim
            };
            hair_tufts(
                p,
                Vec3::new(0.0, 0.1, 0.0),
                0.05,
                5,
                Vec3::new(0.0, -0.3, 1.0),
                lock,
                11,
            );
            animal_eyes(
                p,
                eye_frames,
                0.018,
                Vec3::new(0.44, 0.24, 0.09),
                l.skin,
                l.eyes,
            );
            if style == Style::Unicorn {
                p.accent = Some(Accent::Violet);
                let (a, b) = (Vec3::new(0.0, 0.09, 0.07), Vec3::new(0.0, 0.4, 0.28));
                p.metal
                    .prism(a, b, 0.034, 0.0, 10, Vec3::new(0.12, 0.1, 0.16), 0.0);
                for k in 0..4 {
                    let t = 0.12 + k as f32 * 0.2;
                    p.glow.torus(
                        a.lerp(b, t),
                        (b - a).normalize(),
                        0.034 * (1.0 - t) + 0.005,
                        0.005,
                        12,
                        3,
                        Vec3::ONE,
                    );
                }
                hair_tufts(
                    p,
                    Vec3::new(0.0, -0.12, 0.27),
                    0.025,
                    4,
                    Vec3::new(0.0, -1.0, -0.2),
                    l.cloth,
                    61,
                );
            }
        });
    }
    kit.joint(J::Tail, body, Vec3::new(0.0, 0.1, -0.50), "tail", |p| {
        let c = if style == Style::Unicorn {
            l.cloth
        } else {
            l.trim
        };
        tube(
            &mut p.matte,
            &[
                Vec3::ZERO,
                Vec3::new(0.0, -0.06, -0.06),
                Vec3::new(0.0, -0.14, -0.09),
            ],
            &[(0.05, 0.05), (0.04, 0.042), (0.03, 0.03)],
            l.skin,
        );
        // A flowing switch of hair: strands that fan and curl.
        let strands = if style == Style::Unicorn { 12 } else { 9 };
        for k in 0..strands {
            let a = (k as f32 - (strands - 1) as f32 * 0.5) * 0.11;
            let sway = (k as f32 * 1.7).sin() * 0.03;
            tube(
                &mut p.matte,
                &[
                    Vec3::new(0.0, -0.1, -0.08),
                    Vec3::new(a * 0.12, -0.26, -0.15),
                    Vec3::new(a * 0.25 + sway, -0.42, -0.16),
                    Vec3::new(a * 0.32 + sway, -0.56, -0.12),
                ],
                &[(0.034, 0.034), (0.034, 0.03), (0.024, 0.02), (0.002, 0.002)],
                c * (0.8 + 0.4 * rand(13, k)),
            );
        }
    });
    let legs = [
        (J::FL, J::FLs, J::HoofFL, equine.hip(1.0, true), "fl", true),
        (J::FR, J::FRs, J::HoofFR, equine.hip(-1.0, true), "fr", true),
        (
            J::BL,
            J::BLs,
            J::HoofBL,
            equine.hip(1.0, false),
            "bl",
            false,
        ),
        (
            J::BR,
            J::BRs,
            J::HoofBR,
            equine.hip(-1.0, false),
            "br",
            false,
        ),
    ];
    for (thigh, shin, hoof, at, tag, front) in legs {
        let leg_color = if quagga { l.cloth } else { l.skin };
        let upper_at = super::anatomy::EquineBody::upper(front);
        let ankle_at = super::anatomy::EquineBody::lower(front);
        let upper_end = Vec2::new(0.05, 0.058);
        let lower_root = Vec2::new(0.05, 0.056);
        let upper_scale = storybook_part(kit.rig.plan, thigh);
        let lower_scale = storybook_part(kit.rig.plan, shin);
        // A pivot-centered cover contains both scaled end rings as the lower
        // leg turns. Size it from those rings, not the larger muscle above.
        let joint_radius = (upper_end.max_element() * upper_scale.max_element())
            .max(lower_root.max_element() * lower_scale.max_element())
            + 0.0015;
        let joint_radii = Vec3::splat(joint_radius) / upper_scale;
        let root_overlap = lower_root.min_element() * 0.25;
        let upper = kit.joint(thigh, body, at, &format!("leg{tag}"), |p| {
            // A muscled upper leg tapering to a bony knee.
            let (top, mid) = if front { (0.125, 0.092) } else { (0.15, 0.11) };
            let path = [
                Vec3::Y * 0.13,
                upper_at * 0.28 + Vec3::new(0.0, 0.0, if front { 0.005 } else { -0.025 }),
                upper_at * 0.76,
                upper_at,
            ]
            .map(|p| p / upper_scale);
            tube(
                &mut p.matte,
                &path,
                &[
                    (top * 0.6, top * 0.8),
                    (mid * 0.82, mid),
                    (0.056, 0.064),
                    (upper_end.x, upper_end.y),
                ],
                l.skin,
            );
            // Enough rings keep the small cover inside its silhouette margin.
            // The original offset knee/hock landmark remains visible below.
            let resolution = p.matte.res;
            p.matte.res = (18, 32);
            p.matte
                .blob(upper_at / upper_scale, joint_radii, 1, 0.0, l.skin);
            p.matte.res = resolution;
            p.matte.blob(
                upper_at / upper_scale,
                Vec3::new(0.048, 0.054, 0.05),
                1,
                0.0,
                l.skin,
            );
        });
        let lower = kit.joint(shin, upper, upper_at, &format!("shin{tag}"), |p| {
            // Keep the hidden overlap separate: tube paint uses arc length,
            // so inserting it into the cannon would shift the stocking fade.
            p.matte.tube(
                &[Vec3::Y * root_overlap, Vec3::ZERO],
                &[lower_root * 0.85, lower_root],
                12,
                &|_, _, _| l.skin,
            );
            // The stocking fades into the cannon bone instead of ending in
            // a hard sleeve. The fetlock and forward pastern carry the hoof.
            let radii = [
                lower_root,
                Vec2::new(0.035, 0.041),
                Vec2::new(0.043, 0.051),
                Vec2::new(0.038, 0.045),
                Vec2::new(0.045, 0.05),
            ];
            p.matte.tube(
                &[
                    Vec3::ZERO,
                    ankle_at * 0.48 / lower_scale,
                    ankle_at * 0.81 / lower_scale,
                    ankle_at * 0.94 / lower_scale,
                    ankle_at / lower_scale,
                ],
                &radii,
                12,
                &|_, _, t| l.skin.lerp(leg_color, ease((t - 0.08) / 0.28)),
            );
        });
        kit.joint(hoof, lower, ankle_at, &format!("hoof{tag}"), |p| {
            equine_parts::hoof(&mut p.matte, DARK * 2.4);
        });
    }
}

#[cfg(test)]
mod waist_belt_tests {
    use super::*;
    #[test]
    fn centaur_forearm_band_respects_small_detail_mesh_budget() {
        let mut band = Builder::default();
        centaur_forearm_band(&mut band, 1.0, 1.0);
        let triangles = band.surface_triangles().count();
        assert!(triangles <= 768, "small forearm band budget: {triangles}");
    }

    #[test]
    fn centaur_forearm_band_has_clearance_caps_and_adapts_to_build() {
        let mut widths = [[0.0f32; 3]; 2];
        for (sex, woman) in [false, true].into_iter().enumerate() {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let arms = super::super::super::anatomy::RiderArms::resolve(&look);
                let support = forearm_form(arms.width, 1.0, arms.muscle);
                let mut band = Builder::default();
                centaur_forearm_band(&mut band, arms.width, arms.muscle);
                let mut edges = std::collections::BTreeMap::new();
                let mut lo = f32::INFINITY;
                let mut hi = f32::NEG_INFINITY;
                for tri in band.surface_triangles() {
                    for at in [
                        tri[0],
                        tri[1],
                        tri[2],
                        (tri[0] + tri[1]) * 0.5,
                        (tri[1] + tri[2]) * 0.5,
                        (tri[2] + tri[0]) * 0.5,
                        (tri[0] + tri[1] + tri[2]) / 3.0,
                    ] {
                        let field = support.distance(at);
                        assert!(
                            field >= 0.0015 * arms.width && field <= 0.007 * arms.width,
                            "forearm clearance {field} at {at:?}"
                        );
                        widths[sex][build as usize] = widths[sex][build as usize].max(at.x.abs());
                        lo = lo.min(at.y);
                        hi = hi.max(at.y);
                    }
                    for i in 0..3 {
                        let mut edge = [
                            tri[i].to_array().map(f32::to_bits),
                            tri[(i + 1) % 3].to_array().map(f32::to_bits),
                        ];
                        edge.sort();
                        *edges.entry(edge).or_insert(0usize) += 1;
                    }
                }
                assert!((lo + 0.25).abs() < 1e-6 && (hi + 0.13).abs() < 1e-6);
                assert!(
                    edges.values().all(|n| *n == 2),
                    "band seam and cap rings are closed"
                );
            }
        }
        for (male, female) in widths[0].iter().zip(&widths[1]) {
            assert!(female < male, "preserve smaller female radial scale");
        }
        assert!(
            widths[0][0] < widths[0][2] && widths[1][0] < widths[1][2],
            "build affects actual fitted bands"
        );
    }

    #[test]
    fn centaur_glove_cuffs_continue_to_clear_fitted_forearm_bands() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let arms = super::super::super::anatomy::RiderArms::resolve(&look);
                for gloves in equipment_gloves::GloveAppearance::ALL {
                    let mut p = Parts::default();
                    organic_forearm(&mut p.matte, arms.width, 1.0, arms.muscle, true, Vec3::ONE);
                    centaur_forearm_band(&mut p.satin, arms.width, arms.muscle);
                    let mut samples = Vec::new();
                    for y in [-0.245, -0.225, -0.205] {
                        for i in 0..24 {
                            let a = (i as f32 + 0.37) * TAU / 24.0;
                            let out = Vec3::new(a.sin(), 0.0, a.cos());
                            let origin = Vec3::Y * y;
                            let band = p.satin.outer_surface(origin, out).expect("band support");
                            samples.push((origin, out, band));
                        }
                    }
                    let before = p.matte.surface_triangles().count();
                    equipment_gloves::emit_cuff(&mut p, gloves, Vec3::Y * -0.3, arms.width, 1.0);
                    let mut cuff_only = Builder::default();
                    for triangle in p.matte.surface_triangles().skip(before) {
                        cuff_only.tri(triangle, Vec3::ONE);
                    }
                    let mut inner_min = f32::INFINITY;
                    let mut inner_max = f32::NEG_INFINITY;
                    for (origin, out, band) in samples {
                        let cuff = cuff_only
                            .inner_surface(origin, out)
                            .expect("actual inner wall of overlapping glove cuff");
                        inner_min = inner_min.min((cuff - band).dot(out));
                        inner_max = inner_max.max((cuff - band).dot(out));
                        assert!(
                            (cuff - band).dot(out) > 0.001 * arms.width,
                            "{gloves:?} woman={woman} build={build}: inner cuff wall must remain outside band"
                        );
                    }
                    eprintln!("glove inner wall {gloves:?} woman={woman} build={build} gap_min={inner_min:.6} max={inner_max:.6}");
                }
            }
        }
    }

    #[test]
    fn centaur_glove_cuff_lips_fit_without_excessive_layer_gap() {
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let arms = super::super::super::anatomy::RiderArms::resolve(&look);
                for gloves in equipment_gloves::GloveAppearance::ALL {
                    let mut p = Parts::default();
                    organic_forearm(&mut p.matte, arms.width, 1.0, arms.muscle, true, Vec3::ONE);
                    centaur_forearm_band(&mut p.satin, arms.width, arms.muscle);
                    let mut samples = Vec::new();
                    let lip = match gloves {
                        equipment_gloves::GloveAppearance::Old => 0.105,
                        equipment_gloves::GloveAppearance::Padded => 0.110,
                        equipment_gloves::GloveAppearance::Riding => 0.120,
                        equipment_gloves::GloveAppearance::Fencing => 0.140,
                    };
                    for y in [-0.3 + lip - 0.012, -0.3 + lip - 0.004] {
                        for i in 0..24 {
                            let a = (i as f32 + 0.37) * TAU / 24.0;
                            let out = Vec3::new(a.sin(), 0.0, a.cos());
                            let origin = Vec3::Y * y;
                            let band = p.satin.outer_surface(origin, out).expect("band support");
                            samples.push((origin, out, band));
                        }
                    }
                    let before = p.matte.surface_triangles().count();
                    equipment_gloves::emit_cuff(&mut p, gloves, Vec3::Y * -0.3, arms.width, 1.0);
                    let mut cuff_only = Builder::default();
                    for triangle in p.matte.surface_triangles().skip(before) {
                        cuff_only.tri(triangle, Vec3::ONE);
                    }
                    let mut inner_min = f32::INFINITY;
                    let mut inner_max = f32::NEG_INFINITY;
                    for (origin, out, band) in samples {
                        let cuff = cuff_only
                            .inner_surface(origin, out)
                            .expect("actual inner wall of overlapping glove cuff");
                        inner_min = inner_min.min((cuff - band).dot(out));
                        inner_max = inner_max.max((cuff - band).dot(out));
                        assert!(
                            (cuff - band).dot(out) > 0.001 * arms.width,
                            "{gloves:?} woman={woman} build={build}: inner cuff wall must remain outside band"
                        );
                    }
                    assert!(
                        inner_max <= 0.012 * arms.width,
                        "{gloves:?} woman={woman} build={build} excessive lip gap={inner_max}"
                    );
                    eprintln!("glove lip inner wall {gloves:?} woman={woman} build={build} gap_min={inner_min:.6} max={inner_max:.6}");
                }
            }
        }
    }

    #[test]
    fn centaur_forearm_band_follows_emitted_skin_without_large_gaps() {
        let mut worst = f32::NEG_INFINITY;
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let arms = super::super::super::anatomy::RiderArms::resolve(&look);
                let mut skin = Builder::default();
                organic_forearm(&mut skin, arms.width, 1.0, arms.muscle, true, Vec3::ONE);
                let mut band = Builder::default();
                centaur_forearm_band(&mut band, arms.width, arms.muscle);
                assert!(!skin.is_empty() && !band.is_empty());
                let mut minimum = f32::INFINITY;
                let mut maximum = f32::NEG_INFINITY;
                for y in [-0.24, -0.22, -0.20, -0.18, -0.16, -0.14] {
                    for i in 0..48 {
                        let a = (i as f32 + 0.37) * TAU / 48.0;
                        let out = Vec3::new(a.sin(), 0.0, a.cos());
                        let origin = Vec3::Y * y;
                        let bare = skin
                            .outer_surface(origin, out)
                            .expect("actual forearm skin");
                        let worn = band
                            .outer_surface(origin, out)
                            .expect("actual band circumference");
                        let gap = (worn - bare).dot(out);
                        minimum = minimum.min(gap);
                        maximum = maximum.max(gap);
                    }
                }
                eprintln!("forearm band woman={woman} build={build} width={} muscle={} gap_min={minimum:.6} max={maximum:.6}",arms.width,arms.muscle);
                worst = worst.max(maximum);
            }
        }
        assert!(
            worst <= 0.010,
            "forearm band must stay within10mm of actual emitted skin: {worst}"
        );
    }

    #[test]
    fn centaur_belt_mesh_clearance_and_authored_height_follow_actual_anatomy() {
        let mut width = [0.0f32; 2];
        for (sex, woman) in [false, true].into_iter().enumerate() {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let support = centaur_torso_support(&look);
                let mut belt = Builder::default();
                centaur_waist_belt(&mut belt, &support);
                let mut low = f32::INFINITY;
                let mut high = f32::NEG_INFINITY;
                for tri in belt.surface_triangles() {
                    for at in [
                        tri[0],
                        tri[1],
                        tri[2],
                        (tri[0] + tri[1]) * 0.5,
                        (tri[1] + tri[2]) * 0.5,
                        (tri[2] + tri[0]) * 0.5,
                        (tri[0] + tri[1] + tri[2]) / 3.0,
                    ] {
                        let clearance = support.distance(at);
                        assert!(
                            (0.002..=0.0085).contains(&clearance),
                            "woman={woman} build={build} clearance={clearance} at={at:?}"
                        );
                        low = low.min(at.y);
                        high = high.max(at.y);
                        width[sex] = width[sex].max(at.x.abs());
                    }
                }
                assert!((low - 0.015).abs() < 1e-6 && (high - 0.055).abs() < 1e-6);
            }
        }
        assert!(
            width[1] < width[0] - 0.003,
            "female support's narrower waist must remain distinct"
        );
    }

    #[test]
    fn centaur_waist_belt_stays_close_to_actual_support() {
        let mut worst = f32::NEG_INFINITY;
        for woman in [false, true] {
            for build in 0..3 {
                let look = Look::with_choice(
                    CreatureModel::Centaur,
                    17,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                let support = centaur_torso_support(&look);
                let mut belt = Builder::default();
                centaur_waist_belt(&mut belt, &support);
                assert!(!belt.is_empty(), "positive emitted belt fixture");
                let mut maximum = f32::NEG_INFINITY;
                let mut minimum = f32::INFINITY;
                for y in [0.02, 0.035, 0.05] {
                    for i in 0..48 {
                        let angle = (i as f32 + 0.37) * TAU / 48.0;
                        let out = Vec3::new(angle.sin(), 0.0, angle.cos());
                        let center = Vec3::Y * y;
                        assert!(support.distance(center) < 0.0);
                        let (mut lo, mut hi) = (0.0, 0.4);
                        assert!(support.distance(center + out * hi) > 0.0);
                        for _ in 0..30 {
                            let mid = (lo + hi) * 0.5;
                            if support.distance(center + out * mid) > 0.0 {
                                hi = mid;
                            } else {
                                lo = mid;
                            }
                        }
                        let at = belt
                            .outer_surface(center, out)
                            .expect("closed belt circumference");
                        let gap = at.dot(out) - (lo + hi) * 0.5;
                        maximum = maximum.max(gap);
                        minimum = minimum.min(gap);
                    }
                }
                eprintln!("centaur belt woman={woman} build={build} radial_gap_min={minimum:.6} max={maximum:.6}");
                worst = worst.max(maximum);
            }
        }
        assert!(
            worst <= 0.012,
            "fitted belt must not float more than12mm from actual support: {worst}"
        );
    }
}
