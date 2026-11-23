//! Playback timing and straight flights from disclosed cells only.
use tor_client_3d_model::playback::*;
use tor_protocol::Position;

fn at(x: i32, y: i32) -> Position {
    Position { x, y, z: 0 }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn covered_ticks_play_at_the_fixed_rate_and_idle_gaps_are_capped() {
    let mut clock = TickClock::new(0.01, 0.1, 0.05);
    // The first disclosed tick waits for the lookahead.
    assert!(close(clock.place(100, 2.0).at, 2.05));
    // Known activity covers 100..150: 50 ticks at the full rate.
    clock.cover(150);
    assert!(close(clock.place(150, 2.0).at, 2.55));
    // 150..1150 is idle: a thousand ticks compress to the gap cap.
    assert!(close(clock.place(1_150, 2.0).at, 2.65));
    // Partly covered: the covered part plays, the idle remainder is capped.
    clock.cover(1_170);
    assert!(close(clock.place(1_200, 2.0).at, 2.65 + 0.2 + 0.1));
    // A start disclosed late is placed before the anchor without moving it.
    assert!(close(clock.place(1_190, 2.0).at, 2.95 - 0.1));
    assert!(close(clock.span(2.95, 30), 3.25));
}

#[test]
fn the_standard_rate_plays_a_base_step_in_0_325_seconds() {
    // 30% longer than the former quarter-second step, for every action alike.
    let clock = TickClock::standard();
    assert!(close(clock.span(1.0, 100), 1.325));
    assert!(close(STANDARD_STEP_SECONDS, 0.325));
}

#[test]
fn placement_reports_how_late_a_tick_was_and_how_far_presentation_trails() {
    let mut clock = TickClock::new(0.01, 0.1, 0.05);
    let first = clock.place(100, 2.0);
    assert!(close(first.at, 2.05));
    assert!(close(first.late, 0.0), "the first tick is on time");
    assert!(
        close(clock.lead(2.0), 0.05),
        "presentation trails by the lookahead"
    );
    // An earlier tick disclosed after its moment has passed.
    let earlier = clock.place(80, 2.0);
    assert!(close(earlier.at, 1.85));
    assert!(close(earlier.late, 0.15));
    // A later tick whose moment has passed resumes from now and says how late.
    clock.cover(200);
    let resumed = clock.place(200, 4.0);
    assert!(close(resumed.at, 4.05));
    assert!(close(resumed.late, 4.0 - 3.05));
    let next = clock.place(250, 4.0);
    assert!(close(next.at, 4.15), "idle ticks capped at the gap");
    assert!(close(next.late, 0.0), "in time again");
}

#[test]
fn a_late_update_resumes_from_now_and_fast_forward_drains() {
    let mut clock = TickClock::new(0.01, 0.1, 0.05);
    clock.place(0, 0.0);
    // Presentation has moved on past where this tick would have played.
    assert!(close(clock.place(10, 9.0).at, 9.05));
    clock.reserve(10.0);
    assert!(!clock.drained(9.5));
    assert!(clock.drained(10.0));
    clock.reserve(12.0);
    clock.skip(9.5);
    assert!(clock.drained(9.5), "fast-forward makes everything due");
    clock.reset();
    assert!(
        close(clock.place(500, 20.0).at, 20.05),
        "a reset starts timing afresh"
    );
}

#[test]
fn a_stepped_diagonal_is_one_straight_run_and_bounces_split() {
    // A shallow off-axis path from the server's per-axis stepper.
    let shallow = [
        at(0, 0),
        at(1, 0),
        at(2, 1),
        at(3, 1),
        at(4, 2),
        at(5, 2),
        at(6, 3),
    ];
    assert_eq!(straight_runs(&shallow), [(0, 6)]);
    // Straight and pure diagonal paths stay one run.
    assert_eq!(straight_runs(&[at(0, 0), at(1, 0), at(2, 0)]), [(0, 2)]);
    assert_eq!(straight_runs(&[at(0, 0), at(1, 1), at(2, 2)]), [(0, 2)]);
    // A bounce reverses: never one run through itself.
    assert_eq!(
        straight_runs(&[at(0, 0), at(1, 0), at(0, 0)]),
        [(0, 1), (1, 2)]
    );
    // A corner turns: two runs that meet at the disclosed corner cell.
    assert_eq!(
        straight_runs(&[at(0, 0), at(1, 0), at(2, 0), at(2, 1), at(2, 2)]),
        [(0, 2), (2, 4)]
    );
    assert!(straight_runs(&[at(4, 4)]).is_empty());
    assert!(straight_runs::<Position>(&[]).is_empty());
}

#[test]
fn flight_legs_stay_on_disclosed_cells_and_wait_across_hidden_gaps() {
    let segments = vec![
        vec![at(0, 0), at(1, 0), at(2, 1), at(3, 1)],
        vec![at(6, 1), at(7, 1)],
    ];
    let legs = flight_legs(&segments, 10.0);
    assert_eq!(legs.len(), 2);
    // Each leg starts and ends on disclosed cells of its own segment.
    assert_eq!((legs[0].from, legs[0].to), (at(0, 0), at(3, 1)));
    assert_eq!((legs[1].from, legs[1].to), (at(6, 1), at(7, 1)));
    let first = 10f64.sqrt() / 10.0;
    assert!(close(legs[0].duration, first));
    // The 3-cell hidden gap delays the second segment; it is not drawn.
    assert!(close(legs[1].delay, first + 0.3));
    assert!(close(legs[1].duration, 0.1));
    // Arrival only at disclosed cells; a hidden cell or target gets none.
    assert!(close(arrival(&segments, &legs, at(0, 0)).unwrap(), 0.0));
    assert!(close(
        arrival(&segments, &legs, at(7, 1)).unwrap(),
        first + 0.4
    ));
    assert_eq!(
        arrival(&segments, &legs, at(4, 1)),
        None,
        "inside the hidden gap"
    );
    assert_eq!(
        arrival(&segments, &legs, at(9, 9)),
        None,
        "never toward an undisclosed target"
    );
}
