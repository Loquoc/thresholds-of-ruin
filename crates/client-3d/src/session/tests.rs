use super::*;
use crate::{CameraRig, Diagnostics};
use std::{collections::BTreeSet, time::Instant};

fn scratch() -> PathBuf {
    let root = crate::launch::repo_root()
        .join(".local/3d-session-tests")
        .join(uuid::Uuid::new_v4().simple().to_string());
    std::fs::create_dir_all(&root).unwrap();
    root
}

fn shipped() -> PathBuf {
    crate::launch::repo_root().join("scenarios")
}

fn app(save_root: PathBuf) -> App {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_event::<AppExit>()
        .add_event::<KeyboardInput>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Presented>()
        .init_resource::<Assets<Mesh>>()
        .insert_resource(Game {
            busy: true,
            ..default()
        })
        .insert_resource(Net(std::sync::Arc::new(Mutex::new(Network::idle()))))
        .insert_resource(CameraRig::default())
        .insert_resource(Diagnostics::default())
        .insert_resource(Session::new(shipped(), save_root))
        .add_plugins(SessionPlugin);
    app.update();
    app
}

fn stage(app: &App) -> &'static str {
    app.world().resource::<Session>().stage.name()
}

fn present(app: &mut App) {
    app.world().resource::<Presented>().record();
    app.update();
}

/// Frames until `done`, presenting each; the private server works off-frame.
fn until(app: &mut App, what: &str, done: impl Fn(&App) -> bool) {
    let started = Instant::now();
    while !done(app) {
        assert!(started.elapsed() < PATIENCE, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(5));
        present(app);
    }
}

fn press(app: &mut App, control: Control) {
    let world = app.world_mut();
    let net = Net(world.resource::<Net>().0.clone());
    world.resource_scope(|world, mut session: Mut<Session>| {
        let mut state = bevy::ecs::system::SystemState::<EventWriter<AppExit>>::new(world);
        let mut exit = state.get_mut(world);
        act(&mut session, &net, control, &mut exit);
    });
    app.update();
}

/// What the scene would do with the first views: the test has no renderer.
fn receive_world(app: &mut App) {
    until(app, "the first view", |app| {
        let net = app.world().resource::<Net>().0.clone();
        let mut net = net.lock().unwrap();
        net.frame_events()
            .into_iter()
            .any(|event| matches!(event, crate::net::Event::View(_)))
    });
    let mut game = app.world_mut().resource_mut::<Game>();
    game.view = Some(crate::test_support::view(1));
    game.view_serial += 1;
    game.busy = false;
}

/// Entities that are neither from before the first game nor the menu's own
/// widgets, which it rebuilds whenever it changes.
fn leftovers(app: &mut App) -> BTreeSet<Entity> {
    let world = app.world_mut();
    let overlay = world
        .query_filtered::<Entity, With<Overlay>>()
        .single(world)
        .unwrap();
    let in_overlay = |world: &World, mut entity: Entity| loop {
        if entity == overlay {
            return true;
        }
        match world.get::<ChildOf>(entity) {
            Some(parent) => entity = parent.parent(),
            None => return false,
        }
    };
    let all: Vec<Entity> = world
        .query_filtered::<Entity, Without<Persistent>>()
        .iter(world)
        .collect();
    all.into_iter()
        .filter(|entity| !in_overlay(world, *entity))
        .collect()
}

#[test]
fn seed_entry_accepts_every_u64_and_nothing_larger() {
    let mut menu = MenuModel::default();
    assert!(menu.digit(0));
    assert!(menu.digit(0));
    assert_eq!(menu.seed, "0", "leading zeros collapse");
    for digit in u64::MAX.to_string().bytes() {
        assert!(menu.digit(digit - b'0'));
    }
    assert_eq!(menu.seed, u64::MAX.to_string());
    assert!(!menu.digit(0), "a 21st digit would overflow");
    assert_eq!(menu.seed, u64::MAX.to_string());
    assert!(!menu.digit(10));
    menu.backspace();
    assert_eq!(menu.seed, "1844674407370955161");
    menu.seed.clear();
    menu.scenarios = local_host::discover_scenarios(&shipped());
    assert_eq!(
        menu.request().err().as_deref(),
        Some("Enter a seed, or choose Random.")
    );
    menu.set_seed(42);
    let Ok(Request::New { seed, scenario }) = menu.request() else {
        panic!("a scenario and seed make a request");
    };
    assert_eq!(seed, 42);
    assert_eq!(scenario, menu.scenarios[0]);
}

#[test]
fn load_requests_name_the_chosen_save_and_pages_stay_in_range() {
    let mut menu = MenuModel {
        tab: Tab::Load,
        ..default()
    };
    assert!(menu.request().is_err(), "nothing to load");
    let saves: Vec<_> = (0..(PAGE * 2 + 1))
        .map(|n| SavedGame {
            save: PathBuf::from(format!("game-{n}/game.db")),
            sidecar: None,
            modified: n as u64,
        })
        .collect();
    menu.set_saves(saves.clone());
    assert_eq!(menu.save, Some(0), "the newest save is chosen at first");
    assert_eq!(menu.pages(), 3);
    for _ in 0..5 {
        menu.turn_page(true);
    }
    assert_eq!(menu.page, 2);
    for _ in 0..5 {
        menu.turn_page(false);
    }
    assert_eq!(menu.page, 0);
    menu.save = Some(3);
    assert_eq!(menu.request(), Ok(Request::Resume(saves[3].clone())));
}

#[test]
fn the_title_precedes_the_menu_and_waits_for_presented_frames() {
    let mut app = app(scratch());
    assert_eq!(stage(&app), "title");
    for _ in 0..20 {
        app.update();
    }
    assert_eq!(stage(&app), "title", "no frames presented, no menu");
    until(&mut app, "the menu", |app| stage(app) == "menu");
}

#[test]
fn a_new_game_loads_in_named_steps_then_saves_and_returns_cleanly() {
    let saves = scratch();
    let mut app = app(saves.clone());
    until(&mut app, "the menu", |app| stage(app) == "menu");
    assert_eq!(leftovers(&mut app), BTreeSet::new());
    {
        let mut session = app.world_mut().resource_mut::<Session>();
        let two_room = session
            .menu
            .scenarios
            .iter()
            .position(|choice| choice.id == "two-room")
            .unwrap();
        session.menu.scenario = two_room;
        session.menu.set_seed(9001);
    }
    press(&mut app, Control::Start);
    let step = |app: &App| match &app.world().resource::<Session>().stage {
        Stage::Loading { step, .. } => Some(*step),
        _ => None,
    };
    assert_eq!(step(&app), Some(Step::Opening { resume: false }));
    until(&mut app, "the connection", |app| {
        step(app) == Some(Step::Connecting)
    });
    receive_world(&mut app);
    present(&mut app);
    assert_eq!(
        step(&app),
        Some(Step::Building),
        "a view alone is not a drawn world"
    );
    until(&mut app, "play", |app| stage(app) == "playing");

    // What a game leaves behind: a scene root with a child.
    let root = app.world_mut().spawn(Transform::default()).id();
    let child = app
        .world_mut()
        .spawn((Transform::default(), ChildOf(root)))
        .id();
    assert_eq!(leftovers(&mut app), BTreeSet::from([root, child]));
    let serial = app.world().resource::<Game>().view_serial;

    app.world_mut().resource_mut::<Session>().ask_to_leave();
    app.update();
    assert_eq!(stage(&app), "confirm");
    press(&mut app, Control::ReturnToMenu);
    assert_eq!(stage(&app), "closing");
    until(&mut app, "the menu again", |app| stage(app) == "menu");
    app.update();
    assert_eq!(app.world().resource::<Session>().menu.error, None);
    assert_eq!(
        leftovers(&mut app),
        BTreeSet::new(),
        "leaving a game removes all it made"
    );
    let game = app.world().resource::<Game>();
    assert!(game.view.is_none() && game.busy);
    assert!(
        game.view_serial >= serial,
        "serials never repeat across games"
    );

    // The game just left is listed first and loads again.
    let session = app.world().resource::<Session>();
    assert_eq!(session.menu.saves.len(), 1);
    assert_eq!(session.menu.saves[0].sidecar.as_ref().unwrap().seed, 9001);
    app.world_mut().resource_mut::<Session>().menu.tab = Tab::Load;
    press(&mut app, Control::Start);
    assert_eq!(step(&app), Some(Step::Opening { resume: true }));
    until(&mut app, "the second connection", |app| {
        step(app) == Some(Step::Connecting)
    });
    receive_world(&mut app);
    until(&mut app, "play again", |app| stage(app) == "playing");
    press(&mut app, Control::SaveAndQuit);
    until(&mut app, "the exit", |app| {
        !app.world().resource::<Events<AppExit>>().is_empty()
    });
    assert_eq!(app.world().resource::<Session>().games, 2);
}

/// Android delivers its back key with no physical key code, only `BrowserBack`.
fn press_back(app: &mut App) {
    app.world_mut().send_event(KeyboardInput {
        key_code: KeyCode::Unidentified(bevy::input::keyboard::NativeKeyCode::Unidentified),
        logical_key: Key::BrowserBack,
        state: ButtonState::Pressed,
        text: None,
        repeat: false,
        window: Entity::PLACEHOLDER,
    });
    app.update();
}

#[test]
fn the_android_back_key_asks_before_leaving_and_quits_from_the_menu() {
    let mut app = app(scratch());
    until(&mut app, "the menu", |app| stage(app) == "menu");
    app.world_mut().resource_mut::<Session>().stage = Stage::Playing { confirm: false };
    press_back(&mut app);
    assert_eq!(stage(&app), "confirm", "back while playing asks first");
    press_back(&mut app);
    assert_eq!(stage(&app), "playing", "back again keeps playing");
    assert!(app.world().resource::<Events<AppExit>>().is_empty());
    app.world_mut().resource_mut::<Session>().stage = Stage::Menu;
    press_back(&mut app);
    assert!(
        !app.world().resource::<Events<AppExit>>().is_empty(),
        "back on the menu leaves the app"
    );
}

#[test]
fn a_failed_start_returns_to_the_menu_with_its_reason() {
    let mut app = app(scratch());
    until(&mut app, "the menu", |app| stage(app) == "menu");
    {
        let mut session = app.world_mut().resource_mut::<Session>();
        session.menu.scenarios = vec![ScenarioChoice {
            id: "missing".into(),
            version: "1".into(),
            path: PathBuf::from("no/such/package"),
        }];
        session.menu.scenario = 0;
    }
    press(&mut app, Control::Start);
    until(&mut app, "the menu", |app| stage(app) == "menu");
    let error = app.world().resource::<Session>().menu.error.clone();
    assert!(error.is_some_and(|e| !e.is_empty()));
}
