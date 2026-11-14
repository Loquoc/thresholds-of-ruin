//! The native biped lower-arm recipe, selected before any support geometry.
//! This is a bounded composition plan, not a general occlusion solver. Open
//! sleeves, scallops and translucent drapes retain all their existing pieces.
use super::*;
use crate::creatures::outfit::outerwear::OuterwearAppearance;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Surface {
    Stone(Vec3),
    Bare(Vec3),
    Sleeve(Vec3),
    Metal(Vec3),
    NoOrganicBase,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum BellMaterial {
    Matte,
    Metal,
    Sheer,
}

/// Each fitted variant names its support rather than inferring it from color.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Cuff {
    AuthoredBell {
        color: Vec3,
        trim: Vec3,
        scale: f32,
        material: BellMaterial,
    },
    EasedSleeve(Vec3),
    RetainedMetal(Vec3),
}

/// Support is a fitting dependency, not a claim that the bracelet is visible.
#[derive(Clone, Copy, Debug, PartialEq)]
enum BraceletSupport {
    Forearm,
    ForearmAndCuff,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Bracelet {
    color: Vec3,
    support: BraceletSupport,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Plan {
    surface: Surface,
    cuff: Option<Cuff>,
    bracelet: Option<Bracelet>,
    fur: Option<Vec3>,
    outer_sleeve: Option<OuterwearAppearance>,
    glove: Option<equipment_gloves::GloveAppearance>,
}

impl Plan {
    /// Both forearm occurrences use this recipe in their own existing frame.
    /// No body shape, mesh, ray query or mutable construction state is needed.
    pub fn resolve(look: &Look, outfit: &Outfit) -> Self {
        if look.style == Style::Guardian {
            return Self {
                surface: Surface::Stone(look.skin),
                cuff: None,
                bracelet: None,
                fur: None,
                outer_sleeve: None,
                glove: None,
            };
        }
        let metal = outfit.arms == Arms::Plate;
        let furry = look.style == Style::Yeti;
        let styled = match outfit.arms {
            Arms::Styled(sleeve) => Some(sleeve),
            _ => None,
        };
        let surface = if metal {
            Surface::Metal(outfit.sleeve(look.skin))
        } else if furry {
            Surface::NoOrganicBase
        } else if outfit.arms.bare()
            && !styled.is_some_and(|s| matches!(s.cut, SleeveCut::Gauntlet | SleeveCut::Draped))
        {
            Surface::Bare(outfit.forearm(look.skin))
        } else {
            Surface::Sleeve(outfit.forearm(look.skin))
        };
        let mut bracelet_support = BraceletSupport::ForearmAndCuff;
        let cuff = if let Some(s) = styled.filter(|s| {
            matches!(
                s.cut,
                SleeveCut::Bell | SleeveCut::Gauntlet | SleeveCut::Draped
            )
        }) {
            let color = s.flare.unwrap_or(s.color);
            Some(Cuff::AuthoredBell {
                color,
                trim: color * 0.8,
                scale: 1.15,
                material: if s.cut == SleeveCut::Gauntlet {
                    BellMaterial::Sheer
                } else {
                    BellMaterial::Matte
                },
            })
        } else if let Some(robe) = outfit.robe {
            // A loose equipment robe does not become the seat of jewelry worn
            // on a bare arm. Styled bells, plate and fur keep their old policy.
            if matches!(surface, Surface::Bare(_))
                && look.equipment.outerwear() == Some(OuterwearAppearance::Robe)
            {
                bracelet_support = BraceletSupport::Forearm;
            }
            Some(Cuff::AuthoredBell {
                color: outfit.sleeve(look.skin),
                trim: robe.trim,
                scale: 1.0,
                material: if metal {
                    BellMaterial::Metal
                } else {
                    BellMaterial::Matte
                },
            })
        } else {
            outfit.cuffs.map(|color| {
                if metal {
                    Cuff::RetainedMetal(color)
                } else {
                    Cuff::EasedSleeve(color)
                }
            })
        };
        Self {
            surface,
            cuff,
            bracelet: outfit.trimmings.bracelets.map(|color| Bracelet {
                color,
                support: bracelet_support,
            }),
            fur: furry.then_some(look.cloth),
            outer_sleeve: look.equipment.outerwear(),
            glove: look.equipment.hands,
        }
    }

    fn glove_under_cloth_bell(self) -> bool {
        self.glove.is_some()
            && matches!(self.surface, Surface::Bare(_) | Surface::Sleeve(_))
            && matches!(
                self.cuff,
                Some(Cuff::AuthoredBell {
                    material: BellMaterial::Matte,
                    ..
                })
            )
            && self.bracelet.is_none()
            && self.fur.is_none()
            && self.outer_sleeve.is_none()
    }

    /// Material batching remains downstream of these selected relationships:
    /// bracelet fits the selected support; outer sleeve fits retained pieces
    /// plus fur. An ordinary opaque bell covers a forearm-fitted glove cuff;
    /// other layer combinations retain complete-arm glove support. A valid
    /// bracelet may remain hidden. Fit rejection is not a visibility decision.
    pub fn emit(self, p: &mut Parts, body: HumanoidBody, side: f32) {
        let b = body.build;
        let length = body.arm_length;
        let glove_under_cloth_bell = self.glove_under_cloth_bell();
        match self.surface {
            Surface::Stone(skin) => {
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.15,
                    Vec3::new(0.075, 0.15, 0.075),
                    skin * 0.9,
                    9,
                );
                stone(
                    &mut p.matte,
                    Vec3::Y * -0.34,
                    Vec3::new(0.105, 0.085, 0.105),
                    skin,
                    10,
                );
                return;
            }
            Surface::Metal(color) => {
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
                p.metal
                    .blob(Vec3::ZERO, Vec3::splat(0.058 * b), 2, 0.0, color);
                metal_forearm(&mut p.metal, b, length, body.profile.muscle, color);
            }
            Surface::Bare(color) => {
                organic_forearm(&mut p.matte, b, length, body.profile.muscle, true, color)
            }
            Surface::Sleeve(color) => {
                organic_forearm(&mut p.matte, b, length, body.profile.muscle, false, color)
            }
            Surface::NoOrganicBase => {}
        }
        let mut bracelet = self.bracelet.and_then(|request| {
            (request.support == BraceletSupport::Forearm)
                .then(|| accessory_parts::resolve_bracelet(p, request.color, b, length))
                .flatten()
        });
        // A cloth bell is the outer layer, not a shape the glove must copy.
        // Excluded jewelry/fur/wrappers keep their existing fit dependencies.
        if let Some(glove) = self.glove.filter(|_| glove_under_cloth_bell) {
            equipment_gloves::emit_cuff(p, glove, body.hand, b, length);
        }
        match self.cuff {
            Some(Cuff::AuthoredBell {
                color,
                trim,
                scale,
                material,
            }) => {
                let m = match material {
                    BellMaterial::Matte => &mut p.matte,
                    BellMaterial::Metal => &mut p.metal,
                    BellMaterial::Sheer => &mut p.sheer,
                };
                biped_cuff(m, b * scale, length, body.profile.muscle, true, color, trim);
            }
            Some(Cuff::EasedSleeve(color)) => biped_cuff(
                &mut p.matte,
                b,
                length,
                body.profile.muscle,
                false,
                color,
                color * 0.86,
            ),
            Some(Cuff::RetainedMetal(color)) => metal_forearm_cuff(&mut p.metal, b, length, color),
            None => {}
        }
        if let Some(request) = self.bracelet {
            if request.support == BraceletSupport::ForearmAndCuff {
                bracelet = accessory_parts::resolve_bracelet(p, request.color, b, length);
            }
            if let Some(ring) = bracelet {
                accessory_parts::emit_bracelet(p, ring, request.color);
            }
        }
        if let Some(color) = self.fur {
            fur_parts::Coat::hanging_limb(
                body.hand.length(),
                Vec2::splat(0.085 * b),
                0.095 * length,
                color,
                43 + side as u64,
            )
            .emit(p);
        }
        if let Some(outerwear) = self.outer_sleeve {
            equipment_outerwear::emit_robe_sleeve(p, outerwear, 0.26 * length, b, false);
        }
        if let Some(glove) = self.glove.filter(|_| !glove_under_cloth_bell) {
            equipment_gloves::emit_cuff(p, glove, body.hand, b, length);
        }
    }
}

#[cfg(test)]
#[path = "forearm_plan_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bracelet_support_tests.rs"]
mod bracelet_support_tests;
