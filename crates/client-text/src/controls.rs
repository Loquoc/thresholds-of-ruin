//! The text client's own command for each control a server message names.
//! These are words the parser accepts; the server never supplies a key.
use tor_protocol::ControlHint;

/// The command to type for `control`, quoted as the player would enter it.
pub fn command(control: ControlHint) -> String {
    let command = match control {
        ControlHint::Move => return "a direction such as \"north\"".into(),
        ControlHint::Wait => "wait",
        ControlHint::Search => "search",
        ControlHint::PickUp => "take",
        ControlHint::Inventory => "inventory",
        ControlHint::Equip => "wear",
        ControlHint::Drink => "drink",
        ControlHint::Read => "read",
        ControlHint::Zap => "zap",
        ControlHint::Throw => "throw",
        ControlHint::Travel => "go to",
        ControlHint::Look => "look",
        ControlHint::OpenDoor => "open",
        ControlHint::Rest => "rest",
        ControlHint::Save => "save",
        ControlHint::Help => "help",
    };
    format!("\"{command}\"")
}
