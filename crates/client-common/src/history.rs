//! History entries as players read them, shared by every client.
use tor_protocol::*;

use crate::direction_name;

/// The exact wording the actor previously received, when the server recorded it.
pub fn recorded_narration(entry: &HistoryEntry) -> Option<String> {
    (!entry.narration.is_empty()).then(|| entry.narration.join(" "))
}

/// One history entry as a line.
pub fn text(entry: &HistoryEntry) -> String {
    if let Some(text) = recorded_narration(entry) {
        return text;
    }
    match &entry.content {
        HistoryContent::Narration {} => "No narration was recorded.".into(),
        HistoryContent::PlaceRenamed { name, .. } => format!("Place named {}.", name),
        HistoryContent::Travel { .. } => "Travel requested.".into(),
        HistoryContent::Wizard { summary, .. } => summary.clone(),
        HistoryContent::Action { event, .. } => match event.as_ref() {
            Event::Moved { direction } => format!("Moved {}.", direction_name(*direction)),
            Event::Escaped {} => "Took the way out.".into(),
            Event::PlaneReturned => "Returned to the home plane.".into(),
            Event::Crumbled { quantity, .. } => {
                if *quantity == 1 {
                    "An item crumbled during pickup.".into()
                } else {
                    format!("{quantity} items crumbled during pickup.")
                }
            }
            Event::Taken { quantity, .. } => {
                format!(
                    "Picked up {}.",
                    if *quantity == 1 {
                        "an item".to_owned()
                    } else {
                        format!("{quantity} items")
                    }
                )
            }
            Event::Dropped { quantity, .. } => {
                format!(
                    "Dropped {}.",
                    if *quantity == 1 {
                        "an item".to_owned()
                    } else {
                        format!("{quantity} items")
                    }
                )
            }
            Event::DoorChanged { open, .. } => {
                format!("{} door.", if *open { "Opened" } else { "Closed" })
            }
            Event::Waited => "Waited.".into(),
            Event::PreparationPaused => "Preparation paused.".into(),
            Event::AbilityStarted { ability, .. } => format!("Prepared {}.", ability.name),
            Event::EscapeStarted { .. } => "Began trying to escape a hold.".into(),
            Event::ItemStarted { action } => match action {
                Action::Equip { .. } => "Began equipping an item.".into(),
                Action::Unequip { .. } => "Began removing an item.".into(),
                Action::Ignite { .. } => "Began igniting an item.".into(),
                Action::Extinguish { .. } => "Began extinguishing an item.".into(),
                Action::Drink { .. } => "Began drinking an item.".into(),
                Action::Eat { .. } => "Began eating an item.".into(),
                _ => "Began preparation.".into(),
            },
        },
        HistoryContent::Annotation {
            text, attachments, ..
        } => {
            let audience = if entry.audience == Audience::Private {
                "Private note"
            } else {
                "Note"
            };
            if attachments.is_empty() {
                format!("{audience}: {text}")
            } else {
                format!(
                    "{audience}: {text} [{} saved attachment(s)]",
                    attachments.len()
                )
            }
        }
    }
}
