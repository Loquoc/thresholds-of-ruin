//! Exact application stdout retained for the explicit text screenshot command.
//! This is not terminal emulation: echoed input, terminal fonts and other apps
//! are deliberately outside the capture.
use std::{
    fmt,
    io::{self, Write},
    sync::{Mutex, OnceLock},
};
use tor_client_common::{captures::CapturedAttachment, Connection};
use tor_protocol::*;

#[derive(Default)]
struct Transcript {
    text: String,
    basis: Option<(InputContext, BranchId, SnapshotId)>,
}

impl Transcript {
    fn append(&mut self, text: &str) {
        if text.len() >= MAX_TEXT_ATTACHMENT_BYTES {
            let mut start = text.len() - MAX_TEXT_ATTACHMENT_BYTES;
            while !text.is_char_boundary(start) {
                start += 1;
            }
            self.text.clear();
            self.text.push_str(&text[start..]);
            return;
        }
        self.text.push_str(text);
        if self.text.len() > MAX_TEXT_ATTACHMENT_BYTES {
            let mut start = self.text.len() - MAX_TEXT_ATTACHMENT_BYTES;
            while !self.text.is_char_boundary(start) {
                start += 1;
            }
            self.text.drain(..start);
        }
    }

    fn capture(&self) -> Result<CapturedAttachment, String> {
        let (context, branch, snapshot) = self
            .basis
            .clone()
            .ok_or("No game presentation has been shown yet.")?;
        CapturedAttachment::new(
            context,
            branch,
            snapshot,
            AttachmentMediaType::Text,
            self.text.as_bytes().to_vec(),
        )
    }
}

fn transcript() -> &'static Mutex<Transcript> {
    static OUTPUT: OnceLock<Mutex<Transcript>> = OnceLock::new();
    OUTPUT.get_or_init(|| Mutex::new(Transcript::default()))
}

pub(crate) fn emit(args: fmt::Arguments<'_>) {
    let text = args.to_string();
    io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .expect("write client output");
    transcript()
        .lock()
        .expect("client output lock")
        .append(&text);
}

/// Called only after the client has presented this state, never just on receipt.
pub(crate) fn presented(connection: &Connection) {
    transcript().lock().expect("client output lock").basis = Some((
        connection.state.input_context(),
        connection.state.branch().clone(),
        connection.state.state().snapshot.clone(),
    ));
}

pub(crate) fn capture() -> Result<CapturedAttachment, String> {
    transcript().lock().expect("client output lock").capture()
}

pub(crate) fn is_capture_command(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("screenshot")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retained_stdout_preserves_exact_whitespace_and_formatting_bytes() {
        let mut transcript = Transcript::default();
        transcript.append("Room\n  north\twall\n");
        transcript.append("\u{1b}[1mBold\u{1b}[0m\n> ");
        assert_eq!(
            transcript.text,
            "Room\n  north\twall\n\u{1b}[1mBold\u{1b}[0m\n> "
        );
        assert!(transcript.capture().is_err());
    }

    #[test]
    fn retained_tail_is_bounded_on_utf8_boundaries_without_invented_text() {
        let mut transcript = Transcript::default();
        transcript.append(&"x".repeat(MAX_TEXT_ATTACHMENT_BYTES - 1));
        transcript.append("é界");
        assert!(transcript.text.len() <= MAX_TEXT_ATTACHMENT_BYTES);
        assert!(transcript.text.ends_with("é界"));
        assert!(transcript
            .text
            .bytes()
            .take(transcript.text.len() - 5)
            .all(|b| b == b'x'));
        transcript.append(&"界".repeat(MAX_TEXT_ATTACHMENT_BYTES));
        assert!(transcript.text.len() <= MAX_TEXT_ATTACHMENT_BYTES);
        assert!(transcript.text.chars().all(|c| c == '界'));
    }

    #[test]
    fn screenshot_is_a_standalone_command_not_a_report_or_note_side_effect() {
        assert!(is_capture_command(" Screenshot  "));
        for other in [
            "note screenshot",
            "report broken door",
            "screenshot other",
            "",
        ] {
            assert!(!is_capture_command(other));
        }
    }
}
