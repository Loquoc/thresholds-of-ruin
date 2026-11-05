//! The same canned workload is used by tests and the opt-in release example.
use super::*;

fn retained_history(history: usize, attachments: usize) {
    let directory = tempfile::tempdir().unwrap();
    let sample =
        retention_profile::run(history, attachments, &directory.path().join("retention.db"));
    println!("retention-sample {sample}");
}

macro_rules! retention_cases {
    ($(($name:ident, $history:expr, $attachments:expr)),+ $(,)?) => {
        $(#[test]
        fn $name() {
            retained_history($history, $attachments);
        })+
    };
}

retention_cases!(
    (history_0_attachment_1, 0, 1),
    (history_100_attachment_1, 100, 1),
    (history_1000_attachment_1, 1000, 1),
    (history_10000_attachment_1, 10000, 1),
    (history_0_attachments_8, 0, 8),
    (history_100_attachments_8, 100, 8),
    (history_1000_attachments_8, 1000, 8),
    (history_10000_attachments_8, 10000, 8),
);
