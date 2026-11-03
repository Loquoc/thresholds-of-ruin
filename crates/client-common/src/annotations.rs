//! Shared annotation text helpers; these do not capture or submit anything.
use tor_protocol::MAX_NOTE_BYTES;

pub const PLAY_TEST_REPORT_PREFIX: &str = "PLAY TEST REPORT\n\n";
pub const MAX_PLAY_TEST_REPORT_BYTES: usize = MAX_NOTE_BYTES - PLAY_TEST_REPORT_PREFIX.len();

/// Format a report as an ordinary annotation, preserving its supplied body.
pub fn play_test_report(text: &str) -> Result<String, String> {
    if text.trim().is_empty()
        || text.len() > MAX_PLAY_TEST_REPORT_BYTES
        || text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(format!(
            "Reports need 1–{MAX_PLAY_TEST_REPORT_BYTES} UTF-8 bytes and no terminal control characters."
        ));
    }
    Ok(format!("{PLAY_TEST_REPORT_PREFIX}{text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_preserve_text_with_a_literal_heading() {
        assert_eq!(
            play_test_report("  Door stuck!\n\tAgain?  ").unwrap(),
            "PLAY TEST REPORT\n\n  Door stuck!\n\tAgain?  "
        );
    }

    #[test]
    fn reports_budget_the_prefix_and_count_utf8_bytes() {
        let body = "é".repeat(MAX_PLAY_TEST_REPORT_BYTES / 2);
        assert_eq!(play_test_report(&body).unwrap().len(), MAX_NOTE_BYTES);
        assert!(play_test_report(&format!("{body}x")).is_err());
        for invalid in ["", " \n\t", "bad\u{1b}[2J", "bad\0", "bad\r"] {
            assert!(play_test_report(invalid).is_err(), "{invalid:?}");
        }
    }
}
