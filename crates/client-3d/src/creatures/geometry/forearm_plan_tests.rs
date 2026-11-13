use super::*;

fn look() -> Look {
    Look::with_choice(
        CreatureModel::Human,
        22,
        Some(Variant {
            woman: true,
            ..default()
        }),
    )
}

#[test]
fn cuff_variants_name_the_existing_fit_policy() {
    let look = look();
    let mut outfit = Outfit::base(&look);
    outfit.cuffs = Some(Vec3::X);
    let bare = Plan::resolve(&look, &outfit);
    assert_eq!(bare.surface, Surface::Bare(look.skin));
    // Even a bare visible forearm retains the existing eased-sleeve fit field.
    assert_eq!(bare.cuff, Some(Cuff::EasedSleeve(Vec3::X)));
    outfit.arms = Arms::Sleeves(Vec3::Y);
    assert_eq!(
        Plan::resolve(&look, &outfit).surface,
        Surface::Sleeve(Vec3::Y)
    );
    outfit.arms = Arms::Plate;
    let plate = Plan::resolve(&look, &outfit);
    assert_eq!(plate.surface, Surface::Metal(STEEL));
    assert_eq!(plate.cuff, Some(Cuff::RetainedMetal(Vec3::X)));
    outfit.cuffs = None;
    assert_eq!(Plan::resolve(&look, &outfit).cuff, None);
}

#[test]
fn every_styled_cut_preserves_base_material_scale_and_precedence() {
    let look = look();
    let mut robed = look.clone();
    robed.equipment.select("robe").unwrap();
    let robe = Composition::resolve(&robed)
        .outfit
        .robe
        .expect("equipped robe fixture");
    for (cut, cloth, own_bell) in [
        (SleeveCut::Puff, false, false),
        (SleeveCut::Bell, true, true),
        (SleeveCut::Gauntlet, true, true),
        (SleeveCut::Draped, true, true),
        (SleeveCut::OffShoulder, false, false),
        (SleeveCut::Short, false, false),
    ] {
        let mut outfit = Outfit::base(&look);
        outfit.arms = Arms::Styled(crate::creatures::outfit::Sleeve {
            color: Vec3::Y,
            cut,
            puff: false,
            puff_lining: None,
            sheer: false,
            flare: Some(Vec3::Z),
        });
        outfit.robe = Some(robe);
        outfit.cuffs = Some(Vec3::X);
        let plan = Plan::resolve(&look, &outfit);
        assert_eq!(
            plan.surface,
            if cloth {
                Surface::Sleeve(Vec3::Y)
            } else {
                Surface::Bare(look.skin)
            },
            "{cut:?}"
        );
        let expected = if own_bell {
            Cuff::AuthoredBell {
                color: Vec3::Z,
                trim: Vec3::Z * 0.8,
                scale: 1.15,
                material: if cut == SleeveCut::Gauntlet {
                    BellMaterial::Sheer
                } else {
                    BellMaterial::Matte
                },
            }
        } else {
            Cuff::AuthoredBell {
                color: look.skin,
                trim: robe.trim,
                scale: 1.0,
                material: BellMaterial::Matte,
            }
        };
        assert_eq!(plan.cuff, Some(expected), "{cut:?}");
        if own_bell {
            let Arms::Styled(mut sleeve) = outfit.arms else {
                unreachable!()
            };
            sleeve.flare = None;
            outfit.arms = Arms::Styled(sleeve);
            assert!(
                matches!(Plan::resolve(&look, &outfit).cuff, Some(Cuff::AuthoredBell { color, trim, .. }) if color == Vec3::Y && trim == Vec3::Y * 0.8)
            );
        }
        outfit.robe = None;
        if !own_bell {
            assert_eq!(
                Plan::resolve(&look, &outfit).cuff,
                Some(Cuff::EasedSleeve(Vec3::X))
            );
        }
    }
    let mut outfit = Outfit::base(&look);
    outfit.arms = Arms::Plate;
    outfit.robe = Some(robe);
    outfit.cuffs = Some(Vec3::X);
    assert_eq!(
        Plan::resolve(&look, &outfit).cuff,
        Some(Cuff::AuthoredBell {
            color: STEEL,
            trim: robe.trim,
            scale: 1.0,
            material: BellMaterial::Metal
        })
    );
}

#[test]
fn stone_early_return_and_metal_plus_fur_are_preserved() {
    let mut look = look();
    look.equipment.select("robe").unwrap();
    look.equipment.select("padded-gloves").unwrap();
    let mut outfit = Outfit::base(&look);
    outfit.arms = Arms::Plate;
    outfit.cuffs = Some(Vec3::X);
    outfit.trimmings.bracelets = Some(Vec3::Z);
    look.style = Style::Guardian;
    let stone = Plan::resolve(&look, &outfit);
    assert_eq!(stone.surface, Surface::Stone(look.skin));
    assert!(
        stone.cuff.is_none()
            && stone.bracelet.is_none()
            && stone.fur.is_none()
            && stone.outer_sleeve.is_none()
            && stone.glove.is_none()
    );
    look.style = Style::Yeti;
    let furry_metal = Plan::resolve(&look, &outfit);
    assert_eq!(furry_metal.surface, Surface::Metal(STEEL));
    assert_eq!(furry_metal.cuff, Some(Cuff::RetainedMetal(Vec3::X)));
    assert_eq!(
        furry_metal.bracelet,
        Some(Bracelet {
            color: Vec3::Z,
            support: BraceletSupport::ForearmAndCuff
        })
    );
    assert_eq!(furry_metal.fur, Some(look.cloth));
    outfit.arms = Arms::Bare;
    assert_eq!(
        Plan::resolve(&look, &outfit).surface,
        Surface::NoOrganicBase
    );
}

#[test]
fn equipment_removal_restores_plan_and_uncertain_pieces_are_kept() {
    let original = look().dressed(Wardrobe::RoyalPrincess);
    let original_plan = Plan::resolve(&original, &Composition::resolve(&original).outfit);
    assert!(original_plan.bracelet.is_some());
    for outer in ["robe", "mummy-wrapping", "apron"] {
        let mut equipped = original.clone();
        equipped.equipment.select(outer).unwrap();
        equipped.equipment.select("fencing-gloves").unwrap();
        let plan = Plan::resolve(&equipped, &Composition::resolve(&equipped).outfit);
        assert_eq!(
            plan.bracelet.map(|b| b.color),
            original_plan.bracelet.map(|b| b.color),
            "open/partial outer layer is not proof of hiding"
        );
        assert_eq!(plan.glove, Some(equipment_gloves::GloveAppearance::Fencing));
        assert_eq!(plan.outer_sleeve, equipped.equipment.outerwear());
        equipped.equipment = Equipment::default();
        assert_eq!(
            Plan::resolve(&equipped, &Composition::resolve(&equipped).outfit),
            original_plan
        );
    }
}

fn assert_same_parts(actual: &Parts, expected: &Parts) {
    for (a, b) in [
        &actual.matte,
        &actual.metal,
        &actual.satin,
        &actual.sheer,
        &actual.ghost,
        &actual.glow,
    ]
    .into_iter()
    .zip([
        &expected.matte,
        &expected.metal,
        &expected.satin,
        &expected.sheer,
        &expected.ghost,
        &expected.glow,
    ]) {
        a.assert_same_recipe(b);
    }
}

#[test]
fn production_recipe_matches_frozen_preplan_geometry_and_materials() {
    for (model, wardrobe, items) in [
        (CreatureModel::Human, Wardrobe::Adventurer, vec![]),
        (
            CreatureModel::Human,
            Wardrobe::Ranger,
            vec!["riding-gloves"],
        ),
        (CreatureModel::Orc, Wardrobe::Knight, vec![]),
        (
            CreatureModel::Human,
            Wardrobe::Knight,
            vec!["robe", "padded-gloves"],
        ),
        (CreatureModel::Human, Wardrobe::Pharaoh, vec![]),
        (CreatureModel::Human, Wardrobe::Pirate, vec![]),
        (CreatureModel::Nymph, Wardrobe::RoyalPrincess, vec!["robe"]),
        (
            CreatureModel::Nymph,
            Wardrobe::RoyalPrincess,
            vec!["mummy-wrapping", "fencing-gloves"],
        ),
        (CreatureModel::Yeti, Wardrobe::Native, vec![]),
        (
            CreatureModel::Guardian,
            Wardrobe::Native,
            vec!["robe", "old-gloves"],
        ),
    ] {
        let mut l = Look::with_choice(
            model,
            22,
            Some(Variant {
                woman: true,
                build: 1,
                ..default()
            }),
        )
        .dressed(wardrobe);
        for item in items {
            l.equipment.select(item).unwrap();
        }
        let body = HumanoidBody::resolve(&l);
        let outfit = Composition::resolve(&l).outfit;
        for side in [-1.0, 1.0] {
            let mut actual = Parts::default();
            let mut expected = Parts::default();
            actual.soften();
            expected.soften();
            Plan::resolve(&l, &outfit).emit(&mut actual, body, side);
            preplan_forearm(&mut expected, body, &l, &outfit, side);
            if model == CreatureModel::Nymph
                && wardrobe == Wardrobe::RoyalPrincess
                && l.equipment.outerwear() == Some(OuterwearAppearance::Robe)
            {
                // This one fixture intentionally gains an underlying bracelet.
                // All other geometry/materials remain exactly the frozen recipe.
                assert_eq!(actual.metal.surface_triangles().count(), 384);
                assert_eq!(expected.metal.surface_triangles().count(), 0);
                actual.metal.assert_same_recipe_after(384, &expected.metal);
                for (a, b) in [
                    &actual.matte,
                    &actual.satin,
                    &actual.sheer,
                    &actual.ghost,
                    &actual.glow,
                ]
                .into_iter()
                .zip([
                    &expected.matte,
                    &expected.satin,
                    &expected.sheer,
                    &expected.ghost,
                    &expected.glow,
                ]) {
                    a.assert_same_recipe(b);
                }
            } else {
                assert_same_parts(&actual, &expected);
            }
        }
    }
}

#[test]
fn plan_is_resolved_before_support_geometry_and_emission_does_not_reselect() {
    let biped = include_str!("biped.rs");
    let plan = biped.find("forearm_plan::Plan::resolve(").unwrap();
    assert!(plan < biped.find("BootShape::fitted(").unwrap());
    assert_eq!(biped.matches("forearm_plan::Plan::resolve(").count(), 1);
    let source = include_str!("forearm_plan.rs");
    let emit = &source[source.find("pub fn emit(").unwrap()..];
    assert!(
        !emit.contains("look.equipment")
            && !emit.contains("outfit.")
            && !emit.contains("Composition::resolve")
    );
}

// Frozen pre-plan production lower-arm closure. This differential witness is
// deliberately independent of Plan; selection assertions above own intended
// semantics. Keep it frozen while checking this behavior-preserving migration.
fn preplan_forearm(p: &mut Parts, body: HumanoidBody, l: &Look, outfit: &Outfit, side: f32) {
    let b = body.build;
    let arm_len = body.arm_length;
    let blocky = l.style == Style::Guardian;
    let furry = l.style == Style::Yeti;
    let metal = outfit.arms == Arms::Plate;
    let bare_arms = outfit.arms.bare();
    let sleeve = outfit.sleeve(l.skin);
    let fore_color = outfit.forearm(l.skin);
    let styled = match outfit.arms {
        Arms::Styled(sleeve) => Some(sleeve),
        _ => None,
    };
    if blocky {
        stone(
            &mut p.matte,
            Vec3::Y * -0.15,
            Vec3::new(0.075, 0.15, 0.075),
            l.skin * 0.9,
            9,
        );
        stone(
            &mut p.matte,
            Vec3::Y * -0.34,
            Vec3::new(0.105, 0.085, 0.105),
            l.skin,
            10,
        );
        return;
    }
    if metal {
        // An elbow cop with a fan.
        p.metal.blob(
            Vec3::new(0.0, 0.0, -0.01),
            Vec3::new(0.07, 0.065, 0.07),
            2,
            0.0,
            STEEL * 1.05,
        );
        p.metal.blob(
            Vec3::new(side * 0.055, 0.0, -0.01),
            Vec3::new(0.012, 0.05, 0.045),
            2,
            0.0,
            STEEL,
        );
    }
    let m = if metal { &mut p.metal } else { &mut p.matte };
    if metal {
        m.blob(Vec3::ZERO, Vec3::splat(0.058 * b), 2, 0.0, sleeve);
        metal_forearm(m, b, arm_len, body.profile.muscle, sleeve);
    } else if !furry {
        let fore_bare = bare_arms
            && !styled.is_some_and(|styled| {
                matches!(styled.cut, SleeveCut::Gauntlet | SleeveCut::Draped)
            });
        organic_forearm(m, b, arm_len, body.profile.muscle, fore_bare, fore_color);
    }
    let cuff = outfit.cuffs;
    if let Some(styled) = styled.filter(|styled| {
        matches!(
            styled.cut,
            SleeveCut::Bell | SleeveCut::Gauntlet | SleeveCut::Draped
        )
    }) {
        // A bell flares from the elbow; a gauntlet's drape is sheer.
        let flare = styled.flare.unwrap_or(styled.color);
        let builder = if styled.cut == SleeveCut::Gauntlet {
            &mut p.sheer
        } else {
            &mut p.matte
        };
        biped_cuff(
            builder,
            b * 1.15,
            arm_len,
            body.profile.muscle,
            true,
            flare,
            flare * 0.8,
        );
    } else if let Some(robe) = outfit.robe {
        biped_cuff(m, b, arm_len, body.profile.muscle, true, sleeve, robe.trim);
    } else if let Some(cuff) = cuff {
        if metal {
            // Keep the vambrace separate from its elbow cop.
            metal_forearm_cuff(m, b, arm_len, cuff);
        } else {
            biped_cuff(m, b, arm_len, body.profile.muscle, false, cuff, cuff * 0.86);
        }
    }
    if let Some(metal) = outfit.trimmings.bracelets {
        accessory_parts::bracelet(p, metal, b, arm_len);
    }
    if furry {
        fur_parts::Coat::hanging_limb(
            body.hand.length(),
            Vec2::splat(0.085 * b),
            0.095 * arm_len,
            l.cloth,
            43 + side as u64,
        )
        .emit(p);
    }
    if let Some(outerwear) = l.equipment.outerwear() {
        equipment_outerwear::emit_robe_sleeve(p, outerwear, 0.26 * arm_len, b, false);
    }
    if let Some(gloves) = l.equipment.hands {
        equipment_gloves::emit_cuff(p, gloves, body.hand, b, arm_len);
    }
}

#[test]
fn ordinary_cloth_bells_cover_gloves_without_retargeting_other_layers() {
    let mut look = Look::with_choice(
        CreatureModel::Mage,
        7,
        Some(Variant {
            woman: true,
            ..default()
        }),
    );
    look.equipment.select("padded-gloves").unwrap();
    let plan = Plan::resolve(&look, &Composition::resolve(&look).outfit);
    assert!(
        plan.glove_under_cloth_bell(),
        "opaque native bell should cover a forearm-fitted glove cuff"
    );
    for surface in [
        Surface::Metal(Vec3::ONE),
        Surface::NoOrganicBase,
        Surface::Stone(Vec3::ONE),
    ] {
        assert!(!Plan { surface, ..plan }.glove_under_cloth_bell());
    }
    assert!(!Plan {
        fur: Some(Vec3::ONE),
        ..plan
    }
    .glove_under_cloth_bell());
    assert!(!Plan {
        bracelet: Some(Bracelet {
            color: Vec3::ONE,
            support: BraceletSupport::Forearm
        }),
        ..plan
    }
    .glove_under_cloth_bell());
    assert!(!Plan {
        outer_sleeve: Some(OuterwearAppearance::Robe),
        ..plan
    }
    .glove_under_cloth_bell());
    assert!(!Plan {
        glove: None,
        ..plan
    }
    .glove_under_cloth_bell());
    for cuff in [
        None,
        Some(Cuff::EasedSleeve(Vec3::ONE)),
        Some(Cuff::RetainedMetal(Vec3::ONE)),
        Some(Cuff::AuthoredBell {
            color: Vec3::ONE,
            trim: Vec3::ONE,
            scale: 1.0,
            material: BellMaterial::Sheer,
        }),
        Some(Cuff::AuthoredBell {
            color: Vec3::ONE,
            trim: Vec3::ONE,
            scale: 1.0,
            material: BellMaterial::Metal,
        }),
    ] {
        assert!(!Plan { cuff, ..plan }.glove_under_cloth_bell());
    }
}

#[test]
fn cloth_bell_gloves_use_forearm_geometry_and_clear_the_outer_sleeve() {
    for woman in [false, true] {
        for build in 0..3 {
            for appearance in equipment_gloves::GloveAppearance::ALL {
                let mut look = Look::with_choice(
                    CreatureModel::Mage,
                    7,
                    Some(Variant {
                        woman,
                        build,
                        ..default()
                    }),
                );
                look.equipment.select(appearance.name()).unwrap();
                let body = HumanoidBody::resolve(&look);
                let plan = Plan::resolve(&look, &Composition::resolve(&look).outfit);
                assert!(plan.glove_under_cloth_bell());
                let mut actual = Parts::default();
                let mut compact = Parts::default();
                actual.soften();
                compact.soften();
                plan.emit(&mut actual, body, 1.0);
                Plan { cuff: None, ..plan }.emit(&mut compact, body, 1.0);
                for (full, under) in [
                    &actual.matte,
                    &actual.metal,
                    &actual.satin,
                    &actual.sheer,
                    &actual.ghost,
                    &actual.glow,
                ]
                .into_iter()
                .zip([
                    &compact.matte,
                    &compact.metal,
                    &compact.satin,
                    &compact.sheer,
                    &compact.ghost,
                    &compact.glow,
                ]) {
                    full.assert_preserves_recipe_prefix(under);
                }
                let Some(Cuff::AuthoredBell {
                    color, trim, scale, ..
                }) = plan.cuff
                else {
                    panic!("native bell");
                };
                let mut bell = Parts::default();
                bell.soften();
                biped_cuff(
                    &mut bell.matte,
                    body.build * scale,
                    body.arm_length,
                    body.profile.muscle,
                    true,
                    color,
                    trim,
                );
                let mut hits = 0;
                for y in [-0.24, -0.225, -0.21] {
                    for col in 0..24 {
                        let angle = col as f32 * TAU / 24.0;
                        let out = Vec3::new(angle.cos(), 0.0, angle.sin());
                        let origin = Vec3::Y * (y * body.arm_length);
                        let Some(inner) = bell.matte.inner_surface(origin, out) else {
                            continue;
                        };
                        let cuff = [&compact.matte, &compact.metal, &compact.satin]
                            .into_iter()
                            .filter_map(|b| b.outer_surface(origin, out))
                            .map(|p| (p - origin).dot(out))
                            .reduce(f32::max)
                            .expect("glove cuff support");
                        let gap = (inner - origin).dot(out) - cuff;
                        assert!(gap>0.0005*body.build,
                            "{appearance:?} woman{woman} build{build} y{y} angle{angle} cuff/sleeve gap{gap}");
                        hits += 1;
                    }
                }
                assert!(hits >= 48, "enough actual bell overlap witnesses");
            }
        }
    }
}
