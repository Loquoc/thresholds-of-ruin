//! Cosmetic effect facts. Mechanical cues and source identity are separate.
use crate::{EffectAppearance, ObjectPresentation};
use serde::{Deserialize, Serialize};

/// Publicly visible material, explicitly authored rather than inferred from damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectVisual {
    Neutral,
    Flame,
    Frost,
    Sparks,
    Vapor,
    Radiance,
}

/// A frozen observer-safe appearance, not an actionable item or an asset URL.
/// Clients may simplify its style but must not infer undisclosed mechanics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct EffectPresentation {
    pub form: EffectAppearance,
    pub visual: EffectVisual,
    pub style: Option<String>,
    pub object: Option<ObjectPresentation>,
}

impl EffectPresentation {
    /// Conservative fallback for an effect with no authored public styling.
    pub fn neutral(form: EffectAppearance) -> Self {
        Self {
            form,
            visual: EffectVisual::Neutral,
            style: None,
            object: None,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self
            .style
            .as_deref()
            .is_some_and(|style| !valid_style(style))
        {
            return Err("invalid effect style");
        }
        if let Some(object) = &self.object {
            object.validate()?;
            if self.form != EffectAppearance::Missile {
                return Err("object appearance requires a missile");
            }
        }
        Ok(())
    }
}

fn valid_style(style: &str) -> bool {
    !style.is_empty()
        && style.len() <= 80
        && style.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        })
}

fn read_style<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    struct Style;
    impl serde::de::Visitor<'_> for Style {
        type Value = String;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a bounded cosmetic style key")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<String, E> {
            if valid_style(value) {
                Ok(value.to_owned())
            } else {
                Err(E::custom("invalid effect style"))
            }
        }
    }
    fn key<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
        d.deserialize_str(Style)
    }
    #[derive(Deserialize)]
    struct Key(#[serde(deserialize_with = "key")] String);
    Option::<Key>::deserialize(d).map(|value| value.map(|value| value.0))
}

fn read_object<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<ObjectPresentation>, D::Error> {
    Option::<ObjectPresentation>::deserialize(d)
}

impl<'de> Deserialize<'de> for EffectPresentation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            form: EffectAppearance,
            visual: EffectVisual,
            #[serde(deserialize_with = "read_style")]
            style: Option<String>,
            #[serde(deserialize_with = "read_object")]
            object: Option<ObjectPresentation>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        let value = Self {
            form: fields.form,
            visual: fields.visual,
            style: fields.style,
            object: fields.object,
        };
        value.validate().map_err(serde::de::Error::custom)?;
        Ok(value)
    }
}
