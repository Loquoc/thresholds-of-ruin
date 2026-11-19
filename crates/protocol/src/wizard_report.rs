//! Bounded private wizard query output, separate from observations and history.
use serde::{Deserialize, Serialize};

pub const MAX_WIZARD_REPORT_LINES: usize = 64;
pub const MAX_WIZARD_REPORT_LINE_BYTES: usize = 256;
pub const MAX_WIZARD_REPORT_TITLE_BYTES: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct WizardReport {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidWizardReport;

impl std::fmt::Display for InvalidWizardReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("wizard report contains invalid text or exceeds its display budget")
    }
}
impl std::error::Error for InvalidWizardReport {}

fn valid_text<const N: usize>(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= N && !value.chars().any(char::is_control)
}

impl WizardReport {
    pub fn validate(&self) -> Result<(), InvalidWizardReport> {
        (valid_text::<MAX_WIZARD_REPORT_TITLE_BYTES>(&self.title)
            && self.lines.len() <= MAX_WIZARD_REPORT_LINES
            && self
                .lines
                .iter()
                .all(|line| valid_text::<MAX_WIZARD_REPORT_LINE_BYTES>(line)))
        .then_some(())
        .ok_or(InvalidWizardReport)
    }
}

fn read_text<'de, D: serde::Deserializer<'de>, const N: usize>(d: D) -> Result<String, D::Error> {
    let value = String::deserialize(d)?;
    if valid_text::<N>(&value) {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(InvalidWizardReport))
    }
}

impl<'de> Deserialize<'de> for WizardReport {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(transparent)]
        struct Line(
            #[serde(deserialize_with = "read_text::<_, MAX_WIZARD_REPORT_LINE_BYTES>")] String,
        );
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            #[serde(deserialize_with = "read_text::<_, MAX_WIZARD_REPORT_TITLE_BYTES>")]
            title: String,
            #[serde(deserialize_with = "crate::bounded::bounded::<_, _, MAX_WIZARD_REPORT_LINES>")]
            lines: Vec<Line>,
        }
        let fields: Fields = crate::bounded::named_map(d)?;
        Ok(Self {
            title: fields.title,
            lines: fields.lines.into_iter().map(|line| line.0).collect(),
        })
    }
}

pub(crate) fn read_reply<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<(crate::ReplyContext, String, Box<WizardReport>), D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Fields {
        context: crate::ReplyContext,
        #[serde(deserialize_with = "read_text::<_, 128>")]
        request_id: String,
        report: Box<WizardReport>,
    }
    let fields: Fields = crate::bounded::named_map(d)?;
    Ok((fields.context, fields.request_id, fields.report))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_exact_byte_and_line_bounds_and_rejects_control_text() {
        let accepted = WizardReport {
            title: "x".repeat(64),
            lines: vec!["é".repeat(128); 64],
        };
        assert!(accepted.validate().is_ok());
        let serialized = serde_json::to_string(&accepted).unwrap();
        assert_eq!(
            serde_json::from_str::<WizardReport>(&serialized).unwrap(),
            accepted
        );
        for invalid in [
            WizardReport {
                title: "x".repeat(65),
                ..accepted.clone()
            },
            WizardReport {
                lines: vec!["a".into(); 65],
                ..accepted.clone()
            },
            WizardReport {
                lines: vec!["é".repeat(129)],
                ..accepted.clone()
            },
            WizardReport {
                lines: vec!["embedded\nescape\x1b[31m".into()],
                ..accepted
            },
        ] {
            assert!(invalid.validate().is_err());
            assert!(
                serde_json::from_value::<WizardReport>(serde_json::to_value(invalid).unwrap())
                    .is_err()
            );
        }
        for malformed in [
            serde_json::json!(["Location", []]),
            serde_json::json!({"title":"Location", "lines":[], "extra":0}),
            serde_json::json!({"title":"Location"}),
        ] {
            assert!(serde_json::from_value::<WizardReport>(malformed).is_err());
        }
    }
}
