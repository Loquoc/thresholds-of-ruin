//! Modal input reduction for one ASCII window.
//!
//! [`App::input`] offers each input to an ordered list of stages. A stage that
//! owns the input returns `Some(effect)`; the first such stage wins. The order is
//! the precedence contract: blocking screens first, then presentation paging,
//! open selections, cancellation, editors and finally ordinary play keys.
use super::*;

/// One precedence stage over any input.
type InputStage = fn(&mut App, &Input) -> Option<Effect>;
/// One precedence stage over a key press after clicks and text are resolved.
type KeyStage = fn(&mut App, Key) -> Option<Effect>;

/// Stages that may see clicks and text as well as keys, in precedence order.
const INPUT_STAGES: &[InputStage] = &[
    App::pace_input,
    App::journey_skip_input,
    App::stop_running_input,
    App::help_input,
    App::inventory_input,
    App::end_dismiss_input,
    App::message_log_input,
    App::unread_messages_input,
    App::weapon_picker_shortcut_input,
    App::ability_choice_input,
    App::directional_ability_input,
    App::attack_target_input,
    App::pickup_text_input,
    App::escape_input,
    App::save_quit_input,
    // Everything after this point needs a live, idle connection.
    App::offline_input,
    App::places_input,
    App::wizard_editor_input,
    App::note_editor_input,
    App::click_travel_input,
];

/// Key-only stages, in precedence order, before ordinary play keys.
const KEY_STAGES: &[KeyStage] = &[
    App::history_page_key,
    App::spectator_key,
    App::door_direction_key,
    App::pickup_key,
    App::travel_cursor_key,
    App::look_cursor_key,
];

impl App {
    /// Reduce one native or driver input to local state and a shell effect.
    pub fn input(&mut self, input: Input) -> Effect {
        // A pending weapon swap blocks everything; a screenshot is taken over
        // any screen.
        let first = self
            .pending_swap_input(&input)
            .or_else(|| self.screenshot_input(&input));
        if let Some(effect) = first {
            return effect;
        }
        if let Some(screen) = self.modal_screen() {
            return screen(self, input);
        }
        for stage in INPUT_STAGES {
            if let Some(effect) = stage(self, &input) {
                return effect;
            }
        }
        let Input::Key { key } = input else {
            return Effect::None;
        };
        for stage in KEY_STAGES {
            if let Some(effect) = stage(self, key) {
                return effect;
            }
        }
        self.play_key(key)
    }

    /// The full-screen or prompt mode that currently owns every other input.
    fn modal_screen(&self) -> Option<fn(&mut App, Input) -> Effect> {
        let screen: fn(&mut App, Input) -> Effect = if self.discovery_rows.is_some() {
            App::discovery_input
        } else if self.command_line.is_some() {
            App::command_input
        } else if self.movement_prefix.is_some() {
            App::prefix_input
        } else if self.charts_open {
            App::chart_input
        } else if self.projectile_selection.is_some() {
            App::projectile_input
        } else if self.weapon_confirmation.is_some() {
            App::weapon_confirmation_input
        } else if self.weapon_selection.is_some() {
            App::weapon_input
        } else if self.identification_choice().is_some() {
            App::identification_input
        } else if self.zap_selection.is_some() {
            App::zap_input
        } else if !self.hold_choices.is_empty() {
            App::hold_input
        } else if self.supply_selection.is_some() {
            App::supply_input
        } else if self.advancement_open {
            App::advancement_input
        } else if self.stats_open {
            App::stats_input
        } else {
            return None;
        };
        Some(screen)
    }

    fn pending_swap_input(&mut self, input: &Input) -> Option<Effect> {
        self.weapon_swap.as_ref()?;
        if matches!(input, Input::Key { key: Key::Escape }) {
            self.weapon_swap = None;
            self.status =
                "Stopped the Wield shortcut. Already submitted actions still follow normal timing."
                    .into();
        }
        Some(Effect::None)
    }

    fn screenshot_input(&mut self, input: &Input) -> Option<Effect> {
        if !is_screenshot(input) {
            return None;
        }
        if !self.connected || self.busy || self.state.is_none() {
            return Some(Effect::None);
        }
        if self.role == AccessRole::Spectator {
            self.status = "Spectator access is read-only.".into();
            return Some(Effect::None);
        }
        self.busy = true;
        self.running = None;
        Some(Effect::Screenshot)
    }

    fn discovery_input(&mut self, input: Input) -> Effect {
        let Some(rows) = &self.discovery_rows else {
            return Effect::None;
        };
        let most = rows.len().saturating_sub(DISCOVERY_ROWS);
        match input {
            Input::Key { key: Key::Escape } => self.discovery_rows = None,
            Input::Key { key: Key::Up } => {
                self.discovery_scroll = self.discovery_scroll.saturating_sub(1)
            }
            Input::Key { key: Key::Down } => {
                self.discovery_scroll = (self.discovery_scroll + 1).min(most)
            }
            _ => {}
        }
        Effect::None
    }

    fn stats_input(&mut self, input: Input) -> Effect {
        if let Input::Key { key } = input {
            if let Some(effect) = self.strike_page_key(key) {
                return effect;
            }
        }
        let most = self.stats_rows().len().saturating_sub(STATS_ROWS);
        self.stats_scroll = self.stats_scroll.min(most);
        if let Input::Key { key } = input {
            match key {
                Key::Up => self.stats_scroll = self.stats_scroll.saturating_sub(1),
                Key::Down => self.stats_scroll = self.stats_scroll.saturating_add(1).min(most),
                Key::OlderHistory => {
                    self.stats_scroll = self.stats_scroll.saturating_sub(STATS_ROWS)
                }
                Key::RecentHistory => {
                    self.stats_scroll = self.stats_scroll.saturating_add(STATS_ROWS).min(most)
                }
                Key::Escape | Key::Stats => self.stats_open = false,
                _ => {}
            }
        }
        Effect::None
    }

    /// Strike source and detail navigation inside the stats screen.
    fn strike_page_key(&mut self, key: Key) -> Option<Effect> {
        if key == Key::Tab && !self.strike_pages.sources().is_empty() {
            self.strike_sources_open = true;
            self.strike_detail_open = false;
            self.strike_source_selected = 0;
            self.stats_scroll = 0;
            return Some(Effect::None);
        }
        if key == Key::Escape && (self.strike_sources_open || self.strike_detail_open) {
            if self.busy {
                self.strike_pages.clear();
                self.inspection = None;
                self.strike_sources_open = false;
                self.strike_detail_open = false;
                self.stats_open = false;
                return Some(Effect::None);
            }
            if self.strike_detail_open {
                self.strike_detail_open = false;
                self.strike_sources_open = true;
            } else {
                self.strike_sources_open = false;
            }
            self.stats_scroll = 0;
            return Some(Effect::None);
        }
        if self.strike_sources_open {
            match key {
                Key::Up => {
                    self.strike_source_selected = self.strike_source_selected.saturating_sub(1)
                }
                Key::Down => {
                    self.strike_source_selected = (self.strike_source_selected + 1)
                        .min(self.strike_pages.sources().len().saturating_sub(1))
                }
                Key::Enter if !self.busy => {
                    if let (Some(state), Some(source)) = (
                        &self.state,
                        self.strike_pages.sources().get(self.strike_source_selected),
                    ) {
                        let source = source.source.clone();
                        let context = state.reply_context();
                        match self.strike_pages.select(source, &context) {
                            Ok(command) => return Some(self.command(command)),
                            Err(error) => self.status = error,
                        }
                    }
                }
                _ => {}
            }
            return Some(Effect::None);
        }
        if self.strike_detail_open && matches!(key, Key::Slower | Key::Faster) && !self.busy {
            if let Some(state) = &self.state {
                let context = state.reply_context();
                let query = if key == Key::Slower {
                    self.strike_pages.previous(&context)
                } else {
                    self.strike_pages.next(&context)
                };
                match query {
                    Ok(command) => return Some(self.command(command)),
                    Err(error) => self.status = error,
                }
            }
            return Some(Effect::None);
        }
        None
    }

    fn pace_input(&mut self, input: &Input) -> Option<Effect> {
        let Input::Key {
            key: key @ (Key::Slower | Key::Faster),
        } = *input
        else {
            return None;
        };
        let at = PACES_MS
            .iter()
            .position(|&pace| pace >= self.pace_ms)
            .unwrap_or(PACES_MS.len() - 1);
        let at = if key == Key::Slower {
            (at + 1).min(PACES_MS.len() - 1)
        } else {
            at.saturating_sub(1)
        };
        self.pace_ms = PACES_MS[at];
        self.status = format!("Journey steps {} ms apart.", self.pace_ms);
        Some(Effect::Pace(self.pace_ms))
    }

    /// Only the server ends a journey; a key press shows the rest at once.
    fn journey_skip_input(&mut self, input: &Input) -> Option<Effect> {
        let travelling = self.state.as_ref().is_some_and(|s| {
            s.has_control() && s.travel().is_some_and(|t| t.phase == TravelPhase::Active)
        });
        if !(matches!(input, Input::Key { .. }) && self.connected && travelling) {
            return None;
        }
        self.status = "Skipping ahead.".into();
        Some(Effect::Skip)
    }

    fn stop_running_input(&mut self, input: &Input) -> Option<Effect> {
        if self.running.take().is_none() || matches!(input, Input::Click { .. }) {
            return None;
        }
        self.status = "You stop running.".into();
        Some(Effect::None)
    }

    fn help_input(&mut self, input: &Input) -> Option<Effect> {
        let page = self.help_page?;
        self.help_page = match *input {
            Input::Key {
                key: Key::Escape | Key::Help,
            } => None,
            Input::Key { key: Key::Right } => Some(page.next()),
            Input::Key { key: Key::Left } => Some(page.previous()),
            _ => Some(page),
        };
        Some(Effect::None)
    }

    fn inventory_input(&mut self, input: &Input) -> Option<Effect> {
        if !self.inventory_open {
            return None;
        }
        if let Input::Key { .. } = input {
            self.inventory_open = false;
        }
        Some(Effect::None)
    }

    fn end_dismiss_input(&mut self, input: &Input) -> Option<Effect> {
        let terminal = self.state.as_ref().is_some_and(|s| {
            s.state()
                .observation
                .combat
                .as_ref()
                .is_some_and(|c| c.terminal)
        });
        if !matches!(input, Input::Key { key: Key::Enter }) || self.end_dismissed || !terminal {
            return None;
        }
        self.end_dismissed = true;
        Some(Effect::None)
    }

    fn message_log_input(&mut self, input: &Input) -> Option<Effect> {
        let Some(scroll) = self.message_log else {
            if matches!(
                input,
                Input::Key {
                    key: Key::MessageLog
                }
            ) {
                self.message_log = Some(0);
                return Some(Effect::None);
            }
            return None;
        };
        if let Input::Key { key } = *input {
            let rows = self.messages.history_rows(crate::messages::WIDTH).len();
            let most = rows.saturating_sub(MESSAGE_LOG_ROWS);
            self.message_log = match key {
                Key::Up => Some((scroll + 1).min(most)),
                Key::Down => Some(scroll.saturating_sub(1)),
                Key::OlderHistory => Some((scroll + MESSAGE_LOG_ROWS).min(most)),
                Key::RecentHistory => Some(scroll.saturating_sub(MESSAGE_LOG_ROWS)),
                Key::Escape | Key::MessageLog | Key::Enter => None,
                _ => Some(scroll),
            };
        }
        Some(Effect::None)
    }

    /// Unread messages come first: a key shows the next rows and does nothing
    /// else, so no message scrolls away unseen. Esc skips to the end.
    fn unread_messages_input(&mut self, input: &Input) -> Option<Effect> {
        if self.role == AccessRole::Spectator || !self.messages.more() {
            return None;
        }
        match input {
            Input::Key { key: Key::Escape } => self.messages.skip(),
            Input::Key { .. } => self.messages.page(),
            _ => {}
        }
        Some(Effect::None)
    }

    fn weapon_picker_shortcut_input(&mut self, input: &Input) -> Option<Effect> {
        let choosing = !self.ability_choices.is_empty()
            || !self.attack_targets.is_empty()
            || self.selected_ability.is_some();
        (matches!(input, Input::Key { key: Key::Tab }) && choosing)
            .then(|| self.open_weapon_picker())
    }

    fn ability_choice_input(&mut self, input: &Input) -> Option<Effect> {
        if self.ability_choices.is_empty() {
            return None;
        }
        let Input::Key { key } = *input else {
            return Some(Effect::None);
        };
        match key {
            Key::Escape => {
                self.ability_choices.clear();
                self.selected_weapon = WeaponSelection::Default;
                self.status = "Never mind.".into();
            }
            Key::Up | Key::Down => {
                let next = if key == Key::Up {
                    self.selected.saturating_sub(1)
                } else {
                    self.selected
                        .saturating_add(1)
                        .min(self.ability_choices.len() - 1)
                };
                if next != self.selected && self.selected_weapon != WeaponSelection::Default {
                    self.selected_weapon = WeaponSelection::Default;
                    self.status = "Ability changed; using its default weapon.".into();
                }
                self.selected = next;
            }
            Key::Enter => {
                let ability = self.ability_choices[self.selected].clone();
                self.ability_choices.clear();
                let still_offered = self.state.as_ref().is_some_and(|state| {
                    tor_client_common::abilities::choices(&state.state().observation)
                        .contains(&ability)
                });
                if still_offered {
                    return Some(self.choose_ability(ability));
                }
                self.status = format!("You don't have {}.", ability.name);
            }
            _ => {}
        }
        Some(Effect::None)
    }

    /// A selected self-step or directional-ray ability waits for its direction.
    fn directional_ability_input(&mut self, input: &Input) -> Option<Effect> {
        let kind = self.selected_ability.as_ref()?.target_kind;
        if !matches!(
            kind,
            AbilityTargetKind::SelfStep | AbilityTargetKind::DirectionalRay
        ) {
            return None;
        }
        let ray = kind == AbilityTargetKind::DirectionalRay;
        match *input {
            Input::Click { x, y } if ray => {
                let position = self
                    .state
                    .as_ref()
                    .and_then(|state| crate::render::known_cell_at(state, x, y));
                if let Some(position) = position {
                    let ability = self.selected_ability.take().expect("selected ray");
                    return Some(self.choose_ability_invocation(
                        ability,
                        AbilityInvocation::cell_ray([position.x, position.y, position.z]),
                    ));
                }
            }
            Input::Key { key: Key::Escape } => {
                self.selected_ability = None;
                self.selected_weapon = WeaponSelection::Default;
                self.status = "Never mind.".into();
            }
            Input::Key { key } => {
                if let Some(direction) = key.step_direction() {
                    let ability = self
                        .selected_ability
                        .take()
                        .expect("selected directional ability");
                    let aim = if ray {
                        AbilityAim::DirectionalRay { direction }
                    } else {
                        AbilityAim::SelfStep { direction }
                    };
                    let invocation = AbilityInvocation {
                        aim,
                        weapon: WeaponSelection::Default,
                    };
                    return Some(self.choose_ability_invocation(ability, invocation));
                }
            }
            _ => {}
        }
        Some(Effect::None)
    }

    fn attack_target_input(&mut self, input: &Input) -> Option<Effect> {
        if self.attack_targets.is_empty() {
            return None;
        }
        match *input {
            Input::Key { key: Key::Escape } => {
                self.attack_targets.clear();
                self.selected_ability = None;
                self.supply_selection = None;
                self.weapon_selection = None;
                self.selected_weapon = WeaponSelection::Default;
                return Some(Effect::None);
            }
            Input::Key { key: Key::Up } => self.selected = self.selected.saturating_sub(1),
            Input::Key { key: Key::Down } => {
                self.selected = (self.selected + 1).min(self.attack_targets.len() - 1)
            }
            Input::Key { key: Key::Enter } => return Some(self.attack_selected_target()),
            _ => {}
        }
        self.status = self.target_prompt();
        Some(Effect::None)
    }

    fn attack_selected_target(&mut self) -> Effect {
        let target = self.attack_targets[self.selected].id;
        let ability = self.selected_ability.take();
        self.attack_targets.clear();
        if let Some(ability) = ability {
            return self.choose_ability_target(ability, target);
        }
        let weapon = std::mem::replace(&mut self.selected_weapon, WeaponSelection::Default);
        let action = self.state.as_ref().map(|state| {
            tor_client_common::abilities::basic_action_with_weapon(
                &state.state().observation,
                target,
                weapon,
            )
        });
        match action {
            Some(action) => self.act_or_report(action),
            None => Effect::None,
        }
    }

    /// Inventory letters choose a pickup row; digits build a quantity.
    fn pickup_text_input(&mut self, input: &Input) -> Option<Effect> {
        let Input::Text { text } = input else {
            return None;
        };
        if self.pickup.is_empty() {
            return None;
        }
        if let Some(letter) = text.chars().find(char::is_ascii_alphabetic) {
            if let Some(index) = (0..self.pickup.len()).find(|&i| self.choice_letter(i) == letter) {
                self.selected = index;
                return Some(self.input(Input::Key { key: Key::Enter }));
            }
            self.status = format!("No item is lettered {letter}.");
            return Some(Effect::None);
        }
        if self.item_operation.is_some() || self.applying {
            return Some(Effect::None);
        }
        for ch in text.chars().filter(char::is_ascii_digit) {
            // Keep one overflow digit so an oversized request cannot
            // silently become a smaller valid quantity.
            if self.quantity.len() < 21 {
                self.quantity.push(ch);
            }
        }
        Some(Effect::None)
    }

    /// Whether a prompt that Escape cancels, and that blocks quitting, is open.
    fn has_cancellable_prompt(&self) -> bool {
        self.note.is_some()
            || self.wizard_command.is_some()
            || self.history_page.is_some()
            || !self.pickup.is_empty()
            || self.door_direction.is_some()
    }

    fn escape_input(&mut self, input: &Input) -> Option<Effect> {
        if !matches!(input, Input::Key { key: Key::Escape }) {
            return None;
        }
        if self.place_name.take().is_some() {
            return Some(Effect::None);
        }
        if self.places_open {
            self.places_open = false;
            return Some(Effect::None);
        }
        if self.travel_cursor.take().is_some() || self.look_cursor.take().is_some() {
            self.status = "Never mind.".into();
            return Some(Effect::None);
        }
        // Close only the first open editor or page, then any picker state.
        if self.wizard_command.take().is_some()
            || self.note.take().is_some()
            || self.history_page.take().is_some()
            || !self.pickup.is_empty()
            || self.door_direction.is_some()
        {
            self.pickup.clear();
            self.applying = false;
            self.item_operation = None;
            self.attack_targets.clear();
            self.door_direction = None;
            self.status = "Cancelled.".into();
            return Some(Effect::None);
        }
        self.status = "Nothing to cancel. Press S to save and close, or # for commands.".into();
        Some(Effect::None)
    }

    fn save_quit_input(&mut self, input: &Input) -> Option<Effect> {
        let idle = !self.has_cancellable_prompt()
            && self.place_name.is_none()
            && self.look_cursor.is_none()
            && self.travel_cursor.is_none();
        (matches!(input, Input::Key { key: Key::SaveQuit }) && idle).then_some(Effect::Quit)
    }

    fn offline_input(&mut self, _input: &Input) -> Option<Effect> {
        (!self.connected || self.busy).then_some(Effect::None)
    }

    fn places_input(&mut self, input: &Input) -> Option<Effect> {
        if !self.places_open {
            return None;
        }
        if self.place_name.is_some() {
            return Some(self.place_name_input(input));
        }
        let state = self.state.as_ref().expect("attached");
        let count = state.state().observation.places.len();
        match *input {
            Input::Key { key: Key::Up } => {
                self.place_selected = self.place_selected.saturating_sub(1)
            }
            Input::Key { key: Key::Down } => {
                self.place_selected = (self.place_selected + 1).min(count.saturating_sub(1))
            }
            Input::Key { key: Key::Enter } if count > 0 => {
                if self.role == AccessRole::Spectator || !state.has_control() {
                    self.status = "Naming places requires control.".into();
                } else {
                    self.place_name = Some(String::new());
                }
            }
            _ => {}
        }
        Some(Effect::None)
    }

    fn place_name_input(&mut self, input: &Input) -> Effect {
        let name = self.place_name.as_mut().expect("place name editor");
        match input {
            Input::Text { text } => push_bounded(name, text, 80),
            Input::Key {
                key: Key::Backspace,
            } => {
                name.pop();
            }
            Input::Key { key: Key::Enter } if !name.trim().is_empty() => {
                let name = name.trim().to_owned();
                self.place_name = None;
                let state = self.state.as_ref().expect("attached");
                if self.role == AccessRole::Spectator || !state.has_control() {
                    self.status = "Naming places requires control.".into();
                    return Effect::None;
                }
                let Some(place) = state.state().observation.places.get(self.place_selected) else {
                    return Effect::None;
                };
                return self.command(Command::RenamePlace {
                    expected_revision: state.state().revision,
                    key: place.key.clone(),
                    name,
                });
            }
            _ => {}
        }
        Effect::None
    }

    fn wizard_editor_input(&mut self, input: &Input) -> Option<Effect> {
        let draft = self.wizard_command.as_mut()?;
        match input {
            Input::Text { text } => push_bounded(draft, text, MAX_NOTE_BYTES),
            Input::Key {
                key: Key::Backspace,
            } => {
                draft.pop();
            }
            Input::Key { key: Key::Enter } if !draft.trim().is_empty() => {
                let operation = draft.trim().to_owned();
                self.wizard_command = None;
                if let Some(state) = &self.state {
                    return Some(self.command(Command::Wizard {
                        expected_revision: state.state().revision,
                        operation,
                    }));
                }
            }
            _ => {}
        }
        Some(Effect::None)
    }

    fn note_editor_input(&mut self, input: &Input) -> Option<Effect> {
        let draft = self.note.as_mut()?;
        match input {
            Input::Text { text } => {
                let limit = if draft.report {
                    MAX_PLAY_TEST_REPORT_BYTES
                } else {
                    MAX_NOTE_BYTES
                };
                push_bounded(&mut draft.text, text, limit);
            }
            Input::Key {
                key: Key::Backspace,
            } => {
                draft.text.pop();
            }
            Input::Key { key: Key::Tab } => {
                draft.audience = if draft.audience == Audience::Private {
                    Audience::Actor
                } else {
                    Audience::Private
                }
            }
            Input::Key { key: Key::Enter } if !draft.text.trim().is_empty() => {
                let text = if draft.report {
                    match play_test_report(&draft.text) {
                        Ok(text) => text,
                        Err(error) => {
                            self.status = error;
                            return Some(Effect::None);
                        }
                    }
                } else {
                    draft.text.clone()
                };
                let command = Command::Annotate {
                    anchor: Anchor::State {
                        snapshot: draft.snapshot.clone(),
                    },
                    text,
                    audience: draft.audience,
                    source: ClientSource::User,
                    category: AnnotationCategory::Note,
                };
                self.note = None;
                return Some(self.command(command));
            }
            _ => {}
        }
        Some(Effect::None)
    }

    fn click_travel_input(&mut self, input: &Input) -> Option<Effect> {
        let Input::Click { x, y } = *input else {
            return None;
        };
        if self.history_page.is_some() || !self.pickup.is_empty() || self.door_direction.is_some() {
            return Some(Effect::None);
        }
        let position = self
            .state
            .as_ref()
            .and_then(|s| crate::render::known_cell_at(s, x, y));
        Some(position.map_or(Effect::None, |position| self.travel_to(position)))
    }

    /// An open history page scrolls with Up/Down and keeps only history keys.
    fn history_page_key(&mut self, key: Key) -> Option<Effect> {
        let page = self.history_page.as_ref()?;
        match key {
            Key::Up => self.history_scroll = self.history_scroll.saturating_sub(1),
            Key::Down => {
                let rows = history_lines(&page.entries).len();
                self.history_scroll =
                    (self.history_scroll + 1).min(rows.saturating_sub(HISTORY_ROWS));
            }
            Key::OlderHistory | Key::RecentHistory | Key::History => return None,
            _ => {}
        }
        Some(Effect::None)
    }

    fn spectator_key(&mut self, key: Key) -> Option<Effect> {
        let read_only = matches!(
            key,
            Key::Places
                | Key::History
                | Key::OlderHistory
                | Key::RecentHistory
                | Key::Look
                | Key::Charts
                | Key::Discoveries
                | Key::Inventory
                | Key::Stats
                | Key::Help
                | Key::MessageLog
                | Key::Commands
        );
        if self.role != AccessRole::Spectator || read_only {
            return None;
        }
        self.status = "Spectator access is read-only.".into();
        Some(Effect::None)
    }

    fn door_direction_key(&mut self, key: Key) -> Option<Effect> {
        let open = self.door_direction?;
        let Some(state) = &self.state else {
            return Some(Effect::None);
        };
        let Some(direction) = key.step_direction() else {
            return Some(Effect::None);
        };
        let observation = &state.state().observation;
        let [dx, dy, dz] = step_offset(direction);
        let target = Position {
            x: observation.position.x + dx,
            y: observation.position.y + dy,
            z: observation.position.z + dz,
        };
        let door = observation
            .visible_cells
            .iter()
            .find(|cell| cell.position == target)
            .and_then(|cell| cell.door.as_ref())
            .filter(|door| door.reachable)
            .map(|door| door.id);
        self.door_direction = None;
        if let Some(door) = door {
            return Some(self.act(Action::SetDoor { door, open }));
        }
        self.status = "There is no door in that direction.".into();
        Some(Effect::None)
    }

    fn pickup_key(&mut self, key: Key) -> Option<Effect> {
        if self.pickup.is_empty() {
            return None;
        }
        match key {
            Key::Backspace => {
                self.quantity.pop();
            }
            Key::Up => self.selected = self.selected.saturating_sub(1),
            Key::Down => self.selected = (self.selected + 1).min(self.pickup.len() - 1),
            Key::Enter => return Some(self.choose_pickup_row()),
            _ => {}
        }
        Some(Effect::None)
    }

    /// Clear the item picker and any operation it was choosing for.
    fn close_item_picker(&mut self) {
        self.pickup.clear();
        self.applying = false;
        self.item_operation = None;
    }

    /// Submit an action, or show why it could not be formed.
    fn act_or_report(&mut self, action: Result<Action, String>) -> Effect {
        match action {
            Ok(action) => self.act(action),
            Err(reason) => {
                self.status = reason;
                Effect::None
            }
        }
    }

    fn choose_pickup_row(&mut self) -> Effect {
        let item = self.pickup[self.selected].id;
        if std::mem::take(&mut self.applying) {
            let result = self
                .state
                .as_ref()
                .ok_or_else(|| "No current attachment.".into())
                .and_then(|state| controls::apply_action(&state.state().observation, item));
            self.pickup.clear();
            return self.act_or_report(result);
        }
        if let Some(operation) = self.item_operation.take() {
            return self.choose_item_for(operation, item);
        }
        let quantity = if self.quantity.is_empty() {
            None
        } else {
            match self.quantity.parse::<u64>() {
                Ok(q) if q > 0 => Some(q),
                _ => {
                    self.status = "Quantity must be a positive integer.".into();
                    return Effect::None;
                }
            }
        };
        self.close_item_picker();
        self.attack_targets.clear();
        self.door_direction = None;
        self.act(if self.dropping {
            Action::Drop { item, quantity }
        } else {
            Action::Take { item, quantity }
        })
    }

    /// Finish an item operation picker. `item_operation` is already taken.
    fn choose_item_for(&mut self, operation: ItemOperation, item: ItemTarget) -> Effect {
        if std::mem::take(&mut self.choosing_secondary_weapon) {
            self.pickup.clear();
            let view = &self
                .state
                .as_ref()
                .expect("connected state")
                .state()
                .observation;
            if let Some(chosen) =
                item_choices(view, ItemOperation::Wield).find(|candidate| candidate.id == item)
            {
                self.secondary_weapon = Some(item);
                self.status = format!("Secondary weapon: {}.", chosen.name);
            } else {
                self.status = "That weapon is no longer available.".into();
            }
            return Effect::None;
        }
        match operation {
            ItemOperation::Zap => self.choose_zap_device(item),
            ItemOperation::Wield => {
                self.pickup.clear();
                self.wield_weapon(item, false)
            }
            _ => {
                let observation = &self
                    .state
                    .as_ref()
                    .expect("connected state")
                    .state()
                    .observation;
                let result = item_action(observation, item, operation);
                self.close_item_picker();
                self.act_or_report(result)
            }
        }
    }

    /// Zap a self-targeted device now, or ask for its direction.
    fn choose_zap_device(&mut self, item: ItemTarget) -> Effect {
        let state = self.state.as_ref().expect("connected state");
        let observation = &state.state().observation;
        if !item_choices(observation, ItemOperation::Zap).any(|candidate| candidate.id == item) {
            self.close_item_picker();
            self.status = "That device selection is no longer current.".into();
            return Effect::None;
        }
        if let Ok(action) = item_action(observation, item, ItemOperation::Zap) {
            self.close_item_picker();
            self.zap_selection = None;
            return self.act(action);
        }
        let dual = observation
            .interactions
            .as_ref()
            .and_then(|interactions| interactions.inventory.iter().find(|row| row.item == item))
            .is_some_and(|row| row.device_activation == Some(DeviceActivation::DirectionOrSelf));
        self.zap_selection = Some((item, state.reply_context()));
        self.pickup.clear();
        self.applying = false;
        self.status = if dual {
            "Zap in which direction? Movement keys select; . targets yourself; Esc cancels."
        } else {
            "Zap in which direction? Movement keys select; Esc cancels."
        }
        .into();
        Effect::None
    }

    fn travel_cursor_key(&mut self, key: Key) -> Option<Effect> {
        let mut cursor = self.travel_cursor?;
        match key {
            Key::Ascend | Key::Descend => {
                let up = key == Key::Ascend;
                let wanted = if up {
                    crate::map::Kind::StairsUp
                } else {
                    crate::map::Kind::StairsDown
                };
                let found = self.state.as_ref().and_then(|s| {
                    crate::render::map_tiles(s, &self.palette)
                        .into_iter()
                        .filter(|t| t.kind == wanted)
                        .min_by_key(|t| {
                            (t.position.x - cursor.x).abs() + (t.position.y - cursor.y).abs()
                        })
                        .map(|t| t.position)
                });
                match found {
                    Some(stairs) => cursor = stairs,
                    None => {
                        self.status = format!(
                            "You don't know of any stairs {} on this map.",
                            if up { "up" } else { "down" }
                        );
                    }
                }
            }
            Key::Enter | Key::Wait | Key::Travel => {
                let target = self
                    .state
                    .as_ref()
                    .and_then(|s| crate::render::known_column(s, cursor.x, cursor.y));
                return Some(match target {
                    Some(target) => self.travel_to(target),
                    None => {
                        self.status = "You don't know that spot. Pick a known floor.".into();
                        Effect::None
                    }
                });
            }
            _ => match nudge(cursor, key) {
                Some(moved) => cursor = moved,
                None => return Some(Effect::None),
            },
        }
        if let Some(state) = &self.state {
            cursor = crate::map::clamp(&state.state().observation, cursor);
        }
        self.travel_cursor = Some(cursor);
        Some(Effect::None)
    }

    fn look_cursor_key(&mut self, key: Key) -> Option<Effect> {
        let cursor = self.look_cursor?;
        if matches!(key, Key::Enter | Key::Look | Key::Wait) {
            self.look_cursor = None;
            self.status.clear();
            return Some(self.look_at(cursor));
        }
        let Some(mut cursor) = nudge(cursor, key) else {
            return Some(Effect::None);
        };
        if let Some(state) = &self.state {
            cursor = crate::map::clamp(&state.state().observation, cursor);
        }
        self.look_cursor = Some(cursor);
        Some(Effect::None)
    }

    /// Examine an unidentified reachable trap there, or describe the column.
    fn look_at(&mut self, cursor: Position) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let trap = state
            .state()
            .observation
            .visible_cells
            .iter()
            .filter(|cell| (cell.position.x, cell.position.y) == (cursor.x, cursor.y))
            .filter_map(|cell| cell.trap.as_ref())
            .find(|trap| trap.reachable && trap.family.is_none())
            .map(|trap| trap.id);
        if let Some(trap) = trap {
            return self.act(Action::ExamineTrap { trap });
        }
        let text = crate::describe(state, &self.palette, cursor.x, cursor.y);
        self.messages.push(text);
        Effect::None
    }

    /// Ordinary play: no prompt, picker or cursor owns this key.
    fn play_key(&mut self, key: Key) -> Effect {
        if let Some((operation, filter)) = key.item_picker() {
            return self.open_item_picker(operation, filter);
        }
        if let Some(direction) = key.run_direction() {
            return self.run(direction);
        }
        match key {
            Key::Commands => {
                self.command_line = Some(String::new());
                self.command_selected = 0;
                self.command_selected_explicit = false;
                self.status =
                    "Type or use Up/Down; Tab completes, Enter selects, Esc cancels.".into();
                Effect::None
            }
            Key::Search => self.named_ability("search", AbilityTargetKind::SelfOnly),
            Key::Throw => self.named_ability("throw", AbilityTargetKind::DirectionalProjectile),
            Key::Fire => self.open_projectile_shortcut(ProjectileShortcut::Fire),
            Key::Quiver => self.open_projectile_shortcut(ProjectileShortcut::Quiver),
            Key::Apply => self.open_apply_picker(),
            Key::SetSecondaryWeapon => self.open_secondary_weapon_picker(),
            Key::SwapWeapon => self.swap_weapon(),
            Key::RunPrefix => {
                self.movement_prefix = Some(controls::MovementPrefix::Run);
                self.status =
                    "Run in which direction? TOR stopping rules apply; Esc cancels.".into();
                Effect::None
            }
            Key::MoveOnlyPrefix => {
                self.movement_prefix = Some(controls::MovementPrefix::MoveOnly);
                self.status = "Step in which direction without attacking? Esc cancels.".into();
                Effect::None
            }
            Key::EscapeHold => self.escape_hold(),
            Key::Advancement => self.open_advancement(),
            Key::Look => {
                if let Some(state) = &self.state {
                    self.look_cursor = Some(state.state().observation.position);
                    self.status = "Look at what? hjklyubn moves, . or ; picks, Esc cancels.".into();
                }
                Effect::None
            }
            Key::Discoveries if self.connected && !self.busy && self.state.is_some() => {
                self.request(Request::Discoveries)
            }
            Key::Charts => {
                self.charts_open = true;
                self.chart_selected = 0;
                self.chart_scroll = 0;
                if let Some(state) = &self.state {
                    let _ = self
                        .charts
                        .request(tor_client_common::charts::ChartInput::Overview, state);
                }
                Effect::None
            }
            Key::Inventory => {
                self.inventory_open = true;
                Effect::None
            }
            Key::Stats => {
                self.inspection = None;
                self.strike_pages.clear();
                self.strike_sources_open = false;
                self.strike_detail_open = false;
                self.stats_open = true;
                self.stats_scroll = 0;
                Effect::None
            }
            Key::Help => {
                self.help_page = Some(HelpPage::Play);
                Effect::None
            }
            Key::Travel => self.open_travel_cursor(),
            Key::Attack => {
                self.selected_weapon = WeaponSelection::Default;
                self.select_targets(None);
                Effect::None
            }
            Key::Abilities => self.open_ability_choices(),
            Key::Wait => self.act(Action::Wait),
            Key::Control => self.request(Request::AcquireControl),
            Key::Release => self.request(Request::ReleaseControl),
            Key::ResumeIntention | Key::CancelIntention => self.resume_or_cancel_intention(key),
            Key::OpenDoor | Key::CloseDoor => self.open_door_prompt(key == Key::OpenDoor),
            Key::Pickup | Key::Drop => self.open_pickup(key == Key::Drop),
            Key::Places => {
                self.places_open = true;
                self.place_selected = 0;
                Effect::None
            }
            Key::Wizard => {
                if self.role == AccessRole::Wizard {
                    self.wizard_command = Some(String::new());
                } else {
                    self.status = "Wizard authority is required.".into();
                }
                Effect::None
            }
            Key::Note | Key::PlayTestReport => {
                if let Some(state) = &self.state {
                    self.note = Some(NoteDraft {
                        report: key == Key::PlayTestReport,
                        text: String::new(),
                        audience: Audience::Private,
                        snapshot: state.state().snapshot.clone(),
                    });
                }
                Effect::None
            }
            Key::History => self.request(Request::History {
                before: None,
                limit: 50,
            }),
            Key::OlderHistory => self.older_history(),
            Key::RecentHistory => {
                self.history_page = None;
                Effect::None
            }
            _ => match key.step_direction() {
                Some(direction) => self.act(Action::Move { direction }),
                None => Effect::None,
            },
        }
    }

    fn open_secondary_weapon_picker(&mut self) -> Effect {
        if self.weapon_swap.is_some() {
            self.status = "The previous weapon swap is still pending.".into();
            return Effect::None;
        }
        let effect = self.open_item_picker(ItemOperation::Wield, Some(Key::Wield));
        self.choosing_secondary_weapon =
            self.item_operation == Some(ItemOperation::Wield) && !self.pickup.is_empty();
        if self.choosing_secondary_weapon {
            self.status = "Choose a secondary weapon by inventory letter; Esc cancels.".into();
        }
        effect
    }

    fn open_advancement(&mut self) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() || self.role == AccessRole::Spectator {
            self.status = "Advancement requires control of your character.".into();
            return Effect::None;
        }
        self.advancement_open = true;
        self.advancement_command.clear();
        self.advancement_scroll = 0;
        self.advancement_action(tor_client_common::advancement::AdvancementInput::Overview)
    }

    /// Start a run: one step now, continued while nothing interesting appears.
    fn run(&mut self, direction: Direction) -> Effect {
        if self.creature_in_view() {
            self.status = "You can't run with a creature in view.".into();
            return Effect::None;
        }
        if !self.open_ahead(direction) {
            self.status = "You can't run that way.".into();
            return Effect::None;
        }
        let effect = self.act(Action::Move { direction });
        if matches!(effect, Effect::Request(_)) {
            self.running = Some(direction);
        }
        effect
    }

    fn open_travel_cursor(&mut self) -> Effect {
        if self
            .state
            .as_ref()
            .is_some_and(|s| s.travel().is_some_and(|t| t.phase == TravelPhase::Active))
        {
            self.status = "Press Esc to cancel travel before selecting another destination.".into();
            return Effect::None;
        }
        if let Some(state) = self.state.as_ref().filter(|s| s.has_control()) {
            self.travel_cursor = Some(state.state().observation.position);
            self.status = "Travel where? hjklyubn moves, < > stairs, . goes, Esc cancels.".into();
        } else {
            self.status = "Acquire control before travelling.".into();
        }
        Effect::None
    }

    fn open_ability_choices(&mut self) -> Effect {
        self.selected_weapon = WeaponSelection::Default;
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.status = "You are observing. Press F3 to request control.".into();
            return Effect::None;
        }
        self.ability_choices = tor_client_common::abilities::choices(&state.state().observation);
        self.selected = 0;
        self.status = if self.ability_choices.is_empty() {
            "No abilities are available.".into()
        } else {
            "Choose an ability. Up/Down select, Tab chooses a weapon if permitted, Enter confirms, Esc cancels.".into()
        };
        Effect::None
    }

    fn resume_or_cancel_intention(&mut self, key: Key) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.status = "You are observing. Press F3 to request control.".into();
            return Effect::None;
        }
        let resume = key == Key::ResumeIntention;
        let request = if resume {
            state.resume_intention_request()
        } else {
            state.cancel_intention_request()
        };
        if let Some(request) = request {
            return self.request(request);
        }
        self.status = if resume {
            "No suspended action to resume."
        } else {
            "No queued action to cancel."
        }
        .into();
        Effect::None
    }

    fn open_door_prompt(&mut self, open: bool) -> Effect {
        let Some(state) = &self.state else {
            return Effect::None;
        };
        if !state.has_control() {
            self.attack_targets.clear();
            self.place_name = None;
            self.status = "You are observing. Press F3 to request control.".into();
        } else if !state.can_admit_intention() {
            self.status = "You can't act right now.".into();
        } else if state
            .travel()
            .is_some_and(|t| t.phase == TravelPhase::Active)
        {
            self.status = "Press Esc to cancel travel before using a door.".into();
        } else {
            self.door_direction = Some(open);
            self.status = format!(
                "{} in which direction? HJKL/YUBN; Esc cancels.",
                if open { "Open" } else { "Close" }
            );
        }
        Effect::None
    }

    fn open_pickup(&mut self, dropping: bool) -> Effect {
        self.applying = false;
        self.item_operation = None;
        self.dropping = dropping;
        self.quantity.clear();
        let Some(state) = &self.state else {
            return Effect::None;
        };
        let o = &state.state().observation;
        let mut items: Vec<_> = if dropping {
            o.inventory.clone()
        } else {
            o.ground_items
                .iter()
                .filter(|i| i.reachable)
                .map(|i| i.item.clone())
                .collect()
        };
        // Keep disclosed choice order; opaque identities have no ordinal meaning.
        let mut seen = std::collections::BTreeSet::new();
        items.retain(|item| seen.insert(item.id));
        match items.as_slice() {
            [] => {
                self.status = if dropping {
                    "Your inventory is empty."
                } else {
                    "There is nothing at your feet to pick up."
                }
                .into();
                Effect::None
            }
            // One reachable stack is taken whole; the server checks free units.
            // Dropping and choosing among multiple stacks always ask.
            [item] if !dropping => self.act(Action::Take {
                item: item.id,
                quantity: None,
            }),
            _ => {
                self.pickup = items;
                self.selected = 0;
                self.status = format!(
                    "What do you want to {}? Type its letter; digits first set a count.",
                    if dropping { "drop" } else { "pick up" }
                );
                Effect::None
            }
        }
    }

    fn older_history(&mut self) -> Effect {
        let cursor = self
            .history_page
            .as_ref()
            .map(|p| p.older_before.clone())
            .unwrap_or_else(|| self.state.as_ref().and_then(|s| s.older_before().cloned()));
        if let Some(before) = cursor {
            return self.request(Request::History {
                before: Some(before),
                limit: 50,
            });
        }
        self.status = "No older history entries.".into();
        Effect::None
    }
}

fn is_screenshot(input: &Input) -> bool {
    matches!(
        input,
        Input::Key {
            key: Key::Screenshot
        }
    )
}

/// Append printable text while the UTF-8 byte length stays within `limit`.
fn push_bounded(draft: &mut String, text: &str, limit: usize) {
    for ch in text.chars().filter(|ch| !ch.is_control()) {
        if draft.len() + ch.len_utf8() <= limit {
            draft.push(ch);
        }
    }
}
