//! Motion for the Rogue scenario's own monsters: fliers on the wing, the
//! rooted flytrap, beasts' jaws and wings, and each kind's fidgets and fun.
use super::*;

fn roll(a: f32) -> Quat {
    Quat::from_rotation_z(a)
}

fn turn(a: f32) -> Quat {
    Quat::from_rotation_y(a)
}

/// Gather and strike envelopes shared by every offensive move.
pub(super) fn timing(m: Move, t: f32) -> (f32, f32, f32) {
    let hit = m.impact();
    let gather = envelope(t, 0.0, hit * 0.55, hit - 0.1, hit);
    let strike = envelope(t, hit - 0.07, hit, hit + 0.06, 1.0);
    let act = envelope(t, 0.0, 0.12, 0.86, 1.0);
    (gather, strike, act)
}

/// Raise folded wings into a mantle: the leading edge swings out level while
/// the hand keeps the trailing skin or feathers hanging behind it.
fn mantle(p: &mut Pose, k: f32) {
    p.r(J::WingL, roll(-1.15 * k) * fwd(0.25 * k));
    p.r(J::WingR, roll(1.15 * k) * fwd(0.25 * k));
    // Folded tips trail along -Z, so yaw in the opposite direction to
    // each wing's side to swing those tips out rather than into the torso.
    p.r(J::WingTipL, roll(0.96 * k) * turn(-0.66 * k));
    p.r(J::WingTipR, roll(-0.96 * k) * turn(0.66 * k));
}

// ---------------------------------------------------------------------------
// Fliers.

/// Lift the far hand as a distraction while the near hand darts toward
/// the target's belt, then turn the reach inward before settling.
pub(super) fn filch(p: &mut Pose, phase: f32) {
    let feint = envelope(phase, 0.0, 0.20, 0.34, 0.64);
    let reach = envelope(phase, 0.18, 0.40, 0.44, 0.59);
    let pocket = envelope(phase, 0.42, 0.66, 0.76, 1.0);
    p.r(J::ArmL, roll(0.48 * feint) * fwd(0.72 * feint));
    p.r(J::ForeL, fwd(0.86 * feint));
    p.r(J::HandL, fwd(-0.38 * feint));
    p.r(
        J::ArmR,
        fwd(0.18 * feint + 1.29 * reach + 0.39 * pocket) * roll(-0.13 * reach + 0.34 * pocket),
    );
    p.r(J::ForeR, fwd(0.08 * reach + 0.69 * pocket));
    p.r(
        J::HandR,
        fwd(-0.22 * reach + 0.19 * pocket) * turn(-0.50 * pocket),
    );
    p.r(
        J::Torso,
        turn(-0.22 * feint + 0.22 * reach - 0.18 * pocket) * bow(0.20 * reach),
    );
    p.r(
        J::Head,
        turn(0.23 * feint - 0.28 * pocket) * bow(0.08 * reach),
    );
}

pub(super) fn flier_pose(
    p: &mut Pose,
    style: Style,
    clock: f32,
    seed: f32,
    speed: f32,
    action: Option<(Move, f32)>,
) {
    let bat = style == Style::Bat;
    let time = clock + seed;
    let breath = (clock * 1.8 + seed).sin();
    let rate = if bat { 3.4 } else { 2.6 };
    let phase = time * TAU * rate;
    // A kestrel hangs on the wind in bursts of beats, then glides; a bat
    // never stops beating and jinks from side to side.
    let glide = if bat { 0.0 } else { every(time, 6.0, 3.4, 5.8) };
    let amp = (if bat { 0.8 } else { 0.62 }) * (1.0 - 0.8 * glide);
    let beat = phase.sin() * amp;
    let dihedral = 0.18 * glide;
    let jink = if bat { (time * 1.7).sin() * 0.07 } else { 0.0 };
    p.o(
        J::Lift,
        Vec3::new(
            jink,
            -0.05 * phase.cos() * amp + 0.04 * (time * 0.8).sin(),
            0.0,
        ),
    );
    p.r(J::Lift, roll(-jink * 1.5));
    p.r(J::Body, bow(0.3 * speed + 0.06 * beat + breath * 0.01));
    let sweep = 0.22 * phase.cos() * amp;
    p.r(J::WingL, roll(beat + dihedral) * turn(-sweep));
    p.r(J::WingR, roll(-beat - dihedral) * turn(sweep));
    let lag = (phase - 0.7).sin() * amp * 0.55;
    p.r(J::WingTipL, roll(lag));
    p.r(J::WingTipR, roll(-lag));
    // The head stays level while the body rises and falls beneath it.
    p.r(J::Neck, bow(-0.3 * speed - 0.06 * beat));
    p.r(J::Tail, bow(-0.12 * beat) * roll(0.1 * jink));
    for leg in [J::LegL, J::LegR] {
        p.r(leg, fwd(-0.9));
    }
    let Some((m, t)) = action else {
        return;
    };
    let (gather, strike, act) = timing(m, t);
    if m.offensive() {
        // Both climb and stoop. The kestrel strikes with its feet; the
        // bat keeps its feet trailing and snaps its fangs instead.
        p.o(
            J::Lift,
            Vec3::new(
                0.0,
                0.14 * gather - 0.34 * strike,
                -0.12 * gather + 0.42 * strike,
            ),
        );
        // Both dive, but the raptor flares its chest at contact to
        // swing its talons past its beak. The bat still leads with its jaw.
        let contact_pitch = if bat { 0.55 } else { -1.23 };
        p.r(J::Body, bow(-0.35 * gather + contact_pitch * strike));
        p.r(J::WingL, roll(0.9 * gather - 0.3 * strike));
        p.r(J::WingR, roll(-0.9 * gather + 0.3 * strike));
        if bat {
            let contact = m.impact();
            let gape = envelope(t, 0.0, contact * 0.54, contact - 0.07, contact + 0.04);
            p.r(J::Jaw, bow(0.88 * gape));
            p.r(J::Neck, bow(-0.20 * strike));
            p.r(J::Head, bow(0.05 * strike));
        } else {
            for (leg, shin) in [(J::LegL, J::ShinL), (J::LegR, J::ShinR)] {
                p.r(leg, fwd(1.45 * strike));
                p.r(shin, fwd(0.35 * strike));
            }
            p.r(J::Head, bow(1.30 * strike));
        }
    } else {
        // Flare: wings wide, body upright, hanging in the air.
        p.r(J::Body, bow(-0.4 * act));
        p.r(J::WingL, roll(0.35 * act));
        p.r(J::WingR, roll(-0.35 * act));
        p.o(J::Lift, Vec3::Y * 0.1 * act);
    }
}

/// An entry blend counts live corridor pre-roll as elapsed time, but an
/// explicit replay seek into the fall cannot resurrect that entry pose.
fn fall_entry(a: &Anim, seconds: f32) -> Option<(DeathEntry, f32)> {
    let (entry, phase) = (a.death_entry?, a.dead?);
    let elapsed = (a.time - entry.started_at).max(phase.max(0.0) * a.death_motion.duration());
    let fall = ease(elapsed / seconds);
    (fall < 1.0).then_some((entry, fall))
}

/// Advance an authored attack until the disclosed impact, then release its
/// impact pose. A replay already in the fall cannot resurrect the entry.
fn impact_fall_entry(a: &Anim, seconds: f32) -> Option<(DeathEntry, Move, f32, f32)> {
    let (entry, death_phase) = (a.death_entry?, a.dead?);
    let (motion, phase) = entry.action?;
    let elapsed = (a.time - entry.started_at).max(0.0);
    let phase = phase + elapsed.min(entry.impact_delay) / motion.duration();
    let release =
        (elapsed - entry.impact_delay).max(death_phase.max(0.0) * a.death_motion.duration());
    let fall = ease(release / seconds);
    (fall < 1.0 && phase < 1.0).then_some((entry, motion, phase, fall))
}

/// Keep a signature touch alive until impact, then fold from the reached pose.
pub(super) fn touch_fall_entry(a: &Anim, accent: Accent) -> Option<(DeathEntry, f32, f32)> {
    let (entry, motion, phase, fall) = impact_fall_entry(a, 0.22)?;
    (motion == Move::Touch(accent)).then_some((entry, phase, fall))
}

/// Carry the last wing stroke and air position into the authored fall.
pub(super) fn release_flier_pose(p: &mut Pose, style: Style, a: &Anim) {
    let Some((entry, fall)) = fall_entry(a, 0.24) else {
        return;
    };
    let mut live = Pose::new();
    flier_pose(
        &mut live,
        style,
        entry.started_at,
        a.seed,
        entry.speed,
        entry.action,
    );
    for j in [
        J::Lift,
        J::Body,
        J::WingL,
        J::WingR,
        J::WingTipL,
        J::WingTipR,
        J::Neck,
        J::Head,
        J::Tail,
        J::Jaw,
        J::LegL,
        J::LegR,
        J::ShinL,
        J::ShinR,
    ] {
        p.rot[j as usize] = live.rot[j as usize].slerp(p.rot[j as usize], fall);
    }
    p.off[J::Lift as usize] = live.off[J::Lift as usize].lerp(p.off[J::Lift as usize], fall);
}

// ---------------------------------------------------------------------------
// Floating ice: lead with a shard fan instead of pushing the whole core.

pub(super) fn ice_touch_fan(side: f32, lag: f32, gather: f32, reach: f32) -> (Vec3, Quat) {
    if side < 0.0 {
        // Cock the hanging shards back, then swing their tips forward. Move
        // inward only partway so the outstretched fan does not cover the eyes.
        (
            Vec3::new(0.10 * reach, 0.12 * reach, -0.095 * gather + 0.24 * reach),
            turn(0.14 * reach) * fwd(-0.25 * gather + 1.43 * reach) * roll(side * lag),
        )
    } else {
        // The opposite fan braces beside the face without also becoming a spear.
        (
            Vec3::new(0.035 * reach, 0.025 * reach, -0.035 * reach),
            roll(lag + 0.22 * reach),
        )
    }
}

// ---------------------------------------------------------------------------
// The flytrap.

pub(super) fn rooted_pose(p: &mut Pose, clock: f32, seed: f32, action: Option<(Move, f32)>) {
    let breath = (clock * 1.8 + seed).sin();
    let time = clock + seed * 3.7;
    let sway = (time * 0.7).sin();
    p.r(J::Neck, roll(sway * 0.07) * bow(0.04 * breath));
    p.r(
        J::Torso,
        roll(-sway * 0.06) * bow(0.12 + 0.04 * (time * 0.9).sin()),
    );
    // At rest the trap hangs a little open, breathing.
    let mut gape = 0.55 + 0.10 * (time * 0.8).sin();
    if let Some((m, t)) = action {
        let (gather, strike, act) = timing(m, t);
        if m.offensive() {
            // Draw back gaping, then bend the stalk forward. Counter
            // pitch at the hinge so both lobes snap at the target instead
            // of burying the mouth in the ground at full extension.
            p.r(J::Neck, bow(-0.4 * gather + 0.45 * strike));
            p.r(J::Torso, bow(-0.25 * gather + 0.40 * strike));
            p.r(J::Head, bow(-0.25 * strike));
            gape = (gape + 0.6 * gather) * (1.0 - strike);
        } else {
            gape = gape.lerp(0.9, act);
        }
    }
    let gape = gape.clamp(0.0, 1.0);
    p.r(J::Head, bow(-0.55 * gape));
    p.r(J::Jaw, bow(1.5 * gape));
}

/// Play the rooted attack up to actual contact, then close the lobes from
/// the reached pose without changing the roots or targeting.
pub(super) fn release_rooted_pose(p: &mut Pose, a: &Anim) {
    let (Some(entry), Some(death_phase)) = (a.death_entry, a.dead) else {
        return;
    };
    let elapsed = (a.time - entry.started_at).max(0.0);
    let alive_for = elapsed.min(entry.impact_delay);
    // Positive death phase also handles explicit seeks past the entry blend.
    let release =
        (elapsed - entry.impact_delay).max(death_phase.max(0.0) * a.death_motion.duration());
    let fall = ease(release / 0.24);
    if fall >= 1.0 {
        return;
    }
    let action = entry.action.and_then(|(motion, phase)| {
        let phase = phase + alive_for / motion.duration();
        (phase < 1.0).then_some((motion, phase))
    });
    let mut live = Pose::new();
    rooted_pose(&mut live, entry.started_at + alive_for, a.seed, action);
    for j in [J::Neck, J::Torso, J::Head, J::Jaw] {
        p.rot[j as usize] = live.rot[j as usize].slerp(p.rot[j as usize], fall);
    }
}

// ---------------------------------------------------------------------------
// Beasts: jaws, wings and their signature moves.

pub(super) fn beast_pose(
    p: &mut Pose,
    rig: &Rig,
    breath: f32,
    m: Option<Move>,
    t: f32,
    walk: f32,
    cycle: f32,
) {
    let style = rig.style;
    let winged = matches!(style, Style::Griffin | Style::Dragon);
    // Jaws rest a fraction open and breathe; folded wings settle as it walks.
    let rest_jaw = match style {
        Style::Xeroc => 0.06,
        Style::Aquator => 0.03,
        Style::Griffin => 0.025,
        _ => 0.08,
    };
    let jaw_breath = if style == Style::Griffin { 0.015 } else { 0.03 };
    p.r(J::Jaw, bow(rest_jaw + jaw_breath * breath));
    if winged {
        let shuffle = 0.04 * breath + 0.06 * walk;
        mantle(p, shuffle);
    }
    if style == Style::Jabberwock {
        // It carries its chest high and its head forward.
        p.r(J::Neck, bow(0.1));
    }
    grasping_forelimbs(p, rig, m, t, walk, cycle);
    let Some(m) = m else {
        return;
    };
    let (gather, strike, act) = timing(m, t);
    match m {
        Move::Breath(_) => {
            // Rear back drawing breath, then thrust the head forward, jaw wide.
            let pour = envelope(t, m.impact() - 0.08, m.impact(), 0.85, 1.0);
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.08 * gather, -0.12 * gather + 0.1 * pour),
            );
            p.r(J::Body, bow(-0.18 * gather + 0.06 * pour));
            p.r(J::Neck, bow(-0.55 * gather + 0.4 * pour));
            p.r(J::Head, bow(0.3 * gather - 0.25 * pour));
            p.r(J::Jaw, bow(0.25 * gather + 0.85 * pour));
            if winged {
                mantle(p, 1.1 * act);
            }
        }
        Move::Fear => {
            // A roar, wings raised.
            p.r(J::Neck, bow(-0.45 * act));
            let gape = if style == Style::Griffin {
                0.64 * envelope(t, 0.18, m.impact(), m.impact() + 0.12, 0.89)
            } else {
                0.85 * act
            };
            p.r(J::Jaw, bow(gape));
            p.o(J::Lift, Vec3::new(0.0, 0.05 * act, -0.06 * act));
            if winged {
                mantle(p, 1.2 * act);
            }
        }
        Move::Shove if style == Style::Xeroc => xeroc_ambush(p, rig, t, m.impact(), gather, strike),
        Move::Shove => {
            // A whole-body shove for other beasts.
            p.o(
                J::Lift,
                Vec3::new(
                    0.0,
                    -0.05 * gather * rig.legs,
                    -0.1 * gather + 0.42 * strike,
                ),
            );
            p.r(J::Body, bow(0.1 * gather - 0.08 * strike));
            p.r(J::Head, bow(-0.25 * gather + 0.15 * strike));
            p.r(J::Jaw, bow(0.95 * gather * (1.0 - strike)));
        }
        Move::Touch(_) => {
            // The aquator's contact sweep: low and forward, tendrils leading.
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.04 * gather, -0.06 * gather + 0.22 * strike),
            );
            p.r(J::Neck, bow(0.25 * strike));
            p.r(J::Body, roll(0.12 * strike));
        }
        _ if m.offensive() => {
            // A bite: the neck coils back, then strikes, jaws snapping shut.
            p.r(J::Neck, bow(-0.25 * gather + 0.5 * strike));
            p.r(J::Head, bow(0.15 * gather - 0.15 * strike));
            p.r(J::Jaw, bow(0.75 * gather * (1.0 - strike)));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.02 * gather, -0.07 * gather + 0.2 * strike),
            );
            if winged {
                mantle(p, 0.5 * act);
            }
        }
        _ => {
            p.r(J::Neck, bow(-0.15 * act));
            p.r(J::Body, bow(-0.04 * act));
            if m == Move::Step {
                p.o(J::Lift, Vec3::new(0.0, 0.0, -0.14 * act));
            }
            if winged {
                mantle(p, 0.4 * act);
            }
        }
    }
}

/// Let an interrupted display fold into the fall without replacing the
/// quadruped's foot support or replaying the action.
pub(super) fn release_winged_display(p: &mut Pose, rig: &Rig, a: &Anim) {
    if !matches!(rig.style, Style::Dragon | Style::Griffin) {
        return;
    }
    let Some((_, motion, phase, fall)) = impact_fall_entry(a, 0.24) else {
        return;
    };
    if !matches!(
        (rig.style, motion),
        (Style::Dragon, Move::Breath(_)) | (Style::Griffin, Move::Fear)
    ) {
        return;
    }
    let mut neutral = Pose::new();
    let mut display = Pose::new();
    beast_pose(&mut neutral, rig, 0.0, None, 0.0, 0.0, 0.0);
    beast_pose(&mut display, rig, 0.0, Some(motion), phase, 0.0, 0.0);
    for joint in [
        J::Neck,
        J::Head,
        J::Jaw,
        J::WingL,
        J::WingR,
        J::WingTipL,
        J::WingTipR,
    ] {
        let delta = neutral.rot[joint as usize].inverse() * display.rot[joint as usize];
        p.r(joint, Quat::IDENTITY.slerp(delta, 1.0 - fall));
    }
}

/// Reach out of the shell, expose the fangs on contact, then snap shut.
fn xeroc_ambush(p: &mut Pose, rig: &Rig, phase: f32, impact: f32, gather: f32, strike: f32) {
    let gape = envelope(phase, 0.0, impact * 0.52, impact, impact + 0.14);
    // Short supporting legs cannot carry the whole shell far forward without
    // sliding. Put the rest of the reach into the neck instead.
    p.o(
        J::Lift,
        Vec3::new(
            0.0,
            -0.07 * gather * rig.legs,
            -0.065 * gather + 0.13 * strike,
        ),
    );
    p.r(J::Body, bow(0.07 * gather - 0.025 * strike));
    p.o(
        J::Neck,
        Vec3::new(0.0, 0.035 * strike, -0.035 * gather + 0.17 * strike),
    );
    p.r(J::Head, bow(-0.33 * gape + 0.055 * strike));
    p.r(J::Jaw, bow(0.92 * gape - 0.055 * strike));
}

/// Free forelimbs balance the stride and reach into a two-handed rake. This
/// follows limb roles, so another reared body can use the same motion without
/// inheriting Jabberwock's neck, jaw, or tail recipe.
fn grasping_forelimbs(p: &mut Pose, rig: &Rig, m: Option<Move>, t: f32, walk: f32, cycle: f32) {
    let (gather, strike, act) = m.map_or((0.0, 0.0, 0.0), |m| timing(m, t));
    let attacking = m.is_some_and(Move::offensive);
    for (upper, fore, hand, side, phase) in [
        (J::FL, J::FLs, J::HoofFL, 1.0, 0.0),
        (J::FR, J::FRs, J::HoofFR, -1.0, PI),
    ] {
        if rig.supports(hand) {
            continue;
        }
        let swing = (cycle + phase).sin() * 0.09 * walk;
        p.r(upper, fwd(swing) * roll(side * 0.055));
        p.r(fore, fwd(-swing * 0.6));
        if attacking {
            p.r(
                upper,
                fwd(0.55 * gather + 1.0 * strike) * roll(side * (0.24 * gather - 0.08 * strike)),
            );
            // Turn the hooked claws in and down as they reach. Share the
            // twist with the forearm, around its own bone, so the wrists keep
            // the same reach instead of orbiting away from the target.
            let fore_axis = rig.rest[hand as usize].normalize_or(Vec3::NEG_Y);
            p.r(
                fore,
                fwd(-0.35 * gather + 0.35 * strike)
                    * Quat::from_axis_angle(fore_axis, side * 1.24 * strike),
            );
            p.r(hand, fwd(-0.22 * strike) * turn(-side * 0.98 * strike));
        } else if m.is_some() {
            p.r(upper, fwd(0.35 * act) * roll(side * 0.18 * act));
            p.r(fore, fwd(0.2 * act));
        }
    }
}

/// The dead: slack jaws and wings, fallen fliers, a wilted flytrap.
pub(super) fn death(p: &mut Pose, rig: &Rig, d: f32, side: f32) {
    match rig.plan {
        Plan::Bird => {
            // Settle on the flank, not on the end of a vertical wing.
            let height = rig.rest[J::Body as usize].y - rig.girth;
            p.rot[J::Lift as usize] =
                Quat::from_rotation_z(side * 1.48 * d) * Quat::from_rotation_x(0.2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * (height + 0.1) * d, -height * 0.02 * d, 0.0);
            let (lower, upper) = if side > 0.0 {
                (J::WingR, J::WingL)
            } else {
                (J::WingL, J::WingR)
            };
            // Both wings yield into the fall, with the lower wing already
            // lying across the ground when the body bears its own weight.
            p.r(lower, roll(-side * 1.54 * d));
            p.r(upper, roll(-side * 1.03 * d));
            p.r(J::Neck, bow(0.6 * d) * turn(side * 0.5 * d));
            p.r(J::Head, bow(0.3 * d));
            for leg in [J::LegL, J::LegR] {
                p.r(leg, fwd(0.6 * d));
            }
        }
        Plan::Rooted => {
            // Fold the stem partway and let the long lobes finish the drop.
            // Their rim reaches the floor before the stalk does; folding the
            // whole stem flat would bury most of the trap under its roots.
            p.r(J::Neck, bow(0.47 * d) * roll(side * 0.25 * d));
            p.r(J::Torso, bow(0.33 * d));
            p.r(J::Head, bow(0.28 * d) * turn(side * 0.4 * d));
            p.r(J::Jaw, bow(0.52 * d));
        }
        _ => {
            if rig.style == Style::Aquator {
                aquator_fall_contacts(p, rig, side, d);
            }
            let slack = if rig.style == Style::Griffin {
                0.16
            } else {
                0.4
            };
            p.r(J::Jaw, bow(slack * d));
            if matches!(rig.style, Style::Griffin | Style::Dragon) {
                // The upper wing falls open across the flank.
                let (upper, tip) = if side > 0.0 {
                    (J::WingR, J::WingTipR)
                } else {
                    (J::WingL, J::WingTipL)
                };
                p.r(upper, roll(side * 0.6 * d));
                p.r(tip, roll(-side * 0.4 * d));
            }
        }
    }
}

/// Fold the two legs under the fallen flank instead of leaving their webbed
/// feet hanging through the floor. The original roll bears the body's weight.
fn aquator_fall_contacts(p: &mut Pose, rig: &Rig, side: f32, fall: f32) {
    // The storybook-scaled outer haunch carries a lateral roll, rather
    // than the narrower ribcage clearance in the generic fall. Let it take
    // the weight late in the roll as the body approaches its flank.
    use crate::creatures::anatomy::{aquator_haunch, storybook_part};
    let haunch = aquator_haunch(side);
    let flank =
        (haunch.center.x.abs() + haunch.radii.x) * storybook_part(rig.plan, J::Body).x + 0.003;
    p.o(J::Lift, Vec3::Y * ((flank - rig.girth) * fall * fall));
    let weight = ease((fall - 0.18) / 0.40);
    if weight <= 0.0 {
        return;
    }
    let legs = if side > 0.0 {
        [(J::FR, J::FRs, J::HoofFR), (J::BR, J::BRs, J::HoofBR)]
    } else {
        [(J::FL, J::FLs, J::HoofFL), (J::BL, J::BLs, J::HoofBL)]
    };
    let body_rest = rig.rest[J::Body as usize];
    let root = p.rot[J::Lift as usize];
    let body = p.rot[J::Body as usize];
    for (i, (thigh, shin, foot)) in legs.into_iter().enumerate() {
        let [hip, upper, lower] = [thigh, shin, foot].map(|j| rig.rest[j as usize]);
        let ankle = hip
            + p.off[thigh as usize]
            + p.rot[thigh as usize] * (upper + p.rot[shin as usize] * lower);
        let at =
            root * (body_rest + p.off[J::Body as usize] + body * ankle) + p.off[J::Lift as usize];
        // The heavier rear calf has a deeper bulge than the front one.
        // Leave room for it while seating the fore pad close to the floor.
        let ankle_y = rig.ankle_floor + if i == 0 { 0.0 } else { 0.022 };
        beast_limb(
            p,
            thigh,
            shin,
            foot,
            body_rest,
            hip,
            upper,
            lower,
            at.with_y(ankle_y),
            rig.support_bend(shin, foot, if i == 0 { 1.0 } else { -1.0 }),
            weight,
        );
    }
}

/// A long neck needs to yield sideways at the end of the fall: bowing
/// forward alone leaves the head suspended once the body is on its flank.
pub(super) fn settle_winged_beast_head(p: &mut Pose, style: Style, fall: f32, side: f32) {
    let drop = match style {
        Style::Dragon => 0.42,
        Style::Griffin => 0.34,
        _ => return,
    };
    p.r(J::Neck, roll(side * drop * ease((fall - 0.25) / 0.75)));
}

/// The long winged-beast tails trail onto the floor instead of sticking up
/// like levers when the body rolls. The dragon's tail also curves to its own
/// right in the mesh, so counter that bias on both sides of the fall.
pub(super) fn fallen_quadruped_tail(p: &mut Pose, style: Style, d: f32, side: f32) {
    match style {
        Style::Dragon => p.r(J::Tail, turn((side * 0.295 + 0.085) * d)),
        Style::Griffin => p.r(J::Tail, turn(side * 0.24 * d)),
        _ => {
            p.r(J::Tail, turn(-side * 0.5 * d));
            p.r(J::Tail, fwd(0.6 * d) * roll(-side * 0.6 * d));
        }
    }
}

// ---------------------------------------------------------------------------
// Humanoid moves that only Rogue monsters use.

/// The vampire and wraith have an open left hand and a right equipment grip.
/// Reach with the open hand and draw it back toward the chest without
/// asking the grip's fixed fist to mime an open-palmed grasp.
pub(super) fn drain_with_free_hand(p: &mut Pose, reach: f32, draw: f32) {
    p.r(
        J::ArmL,
        fwd(1.48 * reach + 0.65 * draw) * roll(0.16 * reach - 0.10 * draw),
    );
    p.r(J::ForeL, fwd(-0.16 * reach + 1.65 * draw));
    p.r(J::HandL, bow(0.20 * reach - 0.55 * draw));
    p.r(
        J::ArmR,
        fwd(0.32 * reach + 0.22 * draw) * roll(-0.15 * reach),
    );
    p.r(J::ForeR, fwd(0.46 * draw));
}

pub(super) fn biped_move(p: &mut Pose, style: Style, m: Move, t: f32) {
    let (gather, strike, act) = timing(m, t);
    match m {
        Move::Gaze => {
            // A level, unblinking stare, clawed hands spread low.
            let k = act;
            p.r(J::Head, bow(-0.12 * k) * roll((t * 30.0).sin() * 0.015 * k));
            p.r(J::Torso, bow(0.12 * k));
            p.r(J::ArmL, fwd(0.5 * k) * roll(0.65 * k));
            p.r(J::ArmR, fwd(0.5 * k) * roll(-0.65 * k));
            p.r(J::ForeL, fwd(0.7 * k));
            p.r(J::ForeR, fwd(0.7 * k));
            if style != Style::Medusa {
                p.o(J::Lift, Vec3::new(0.0, -0.03 * k, 0.06 * k));
            }
        }
        Move::Bite(_) => {
            p.r(J::Torso, bow(-0.1 * gather + 0.4 * strike));
            p.r(J::Head, bow(0.2 * strike));
            p.r(J::ArmL, fwd(0.4 * gather + 1.1 * strike));
            p.r(J::ArmR, fwd(0.4 * gather + 1.1 * strike));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.04 * gather, -0.06 * gather + 0.3 * strike),
            );
        }
        _ => {
            // A breath from a person: chest swelling, then a forward exhalation.
            p.r(J::Torso, bow(-0.2 * gather + 0.3 * strike));
            p.r(J::Head, bow(-0.25 * gather + 0.2 * strike));
            p.r(J::ArmL, roll(0.5 * gather));
            p.r(J::ArmR, roll(-0.5 * gather));
            p.o(J::Lift, Vec3::new(0.0, 0.0, 0.15 * strike));
        }
    }
}

// ---------------------------------------------------------------------------
// Idle fidgets: true when this replaces the shared idle; people add their
// fidget on top of it.

/// Reach the crown with the open, non-equipment hand. Solve in the torso
/// frame so the claws follow the actual tilted head and each build's arm
/// lengths rather than describing a fixed arc below the chin.
pub(super) fn troll_scratch(p: &mut Pose, rig: &Rig, time: f32, calm: f32) {
    let t = time.rem_euclid(9.0);
    let weight = envelope(t, 5.0, 5.86, 6.64, 7.5) * calm;
    if weight <= 0.0 {
        return;
    }
    let Some(body) = rig.humanoid else {
        return;
    };
    p.r(J::Head, roll(-0.14 * weight));
    let head = p.rot[J::Head as usize];
    let scratch = ((t - 6.25) * 18.0).sin();
    let nail = rig.rest[J::Head as usize]
        + p.off[J::Head as usize]
        + head * Vec3::new(0.135, 0.282 + 0.018 * scratch, 0.023 - 0.012 * scratch);
    let fingers = head * Vec3::new(-0.76, -0.64, -0.10).normalize();
    let wrist = nail - fingers * (0.123 * body.build);
    let upper = rig.rest[J::ForeL as usize];
    let lower = rig.rest[J::HandL as usize];
    let shoulder = rig.rest[J::ArmL as usize] + p.off[J::ArmL as usize];
    let reach = wrist - shoulder;
    let axis = reach.normalize();
    let a = upper.length();
    let b = lower.length();
    let distance = reach.length().clamp((a - b).abs() + 0.001, a + b - 0.001);
    let along = (a * a - b * b + distance * distance) / (2.0 * distance);
    let outward = Vec3::new(1.0, 0.0, 0.38);
    let bend = (outward - axis * outward.dot(axis)).normalize();
    let elbow = axis * along + bend * (a * a - along * along).max(0.0).sqrt();
    let arm = Quat::from_rotation_arc(upper.normalize(), elbow.normalize());
    let fore = arm.inverse()
        * Quat::from_rotation_arc(lower.normalize(), (axis * distance - elbow).normalize());

    let up = -fingers;
    let right = (Vec3::X - up * up.x).normalize();
    let hand = Quat::from_mat3(&Mat3::from_cols(right, up, right.cross(up)));
    for (joint, target) in [
        (J::ArmL, arm),
        (J::ForeL, fore),
        (J::HandL, (arm * fore).inverse() * hand),
    ] {
        p.rot[joint as usize] = p.rot[joint as usize].slerp(target, weight);
    }
}

pub(super) fn idle(p: &mut Pose, rig: &Rig, time: f32, seed: f32, k: f32) -> bool {
    let look = (time * 0.37).sin() * 0.3 + (time * 0.13).sin() * 0.2;
    match rig.style {
        Style::Orc => {
            // Cracks its knuckles.
            let f = every(time, 7.0, 4.0, 6.0) * k;
            let crack = (time * 11.0).sin() * 0.06 * f;
            p.r(J::ArmL, fwd(0.95 * f) * roll(-0.35 * f));
            p.r(J::ArmR, fwd(0.95 * f) * roll(0.35 * f));
            p.r(J::ForeL, fwd((1.4 + crack) * f));
            p.r(J::ForeR, fwd((1.4 - crack) * f));
            p.r(J::Head, bow(0.12 * f));
            false
        }
        Style::Troll => {
            p.r(J::Lift, roll((time * 0.5).sin() * 0.05 * k));
            false
        }
        Style::Leprechaun => {
            // Bounces on its toes and flips a coin.
            p.o(J::Lift, Vec3::Y * (time * 3.0).sin().abs() * 0.018 * k);
            let f = every(time, 6.0, 3.0, 4.6) * k;
            p.r(J::ArmR, fwd(0.6 * f));
            p.r(J::ForeR, fwd(0.9 * f));
            p.r(J::HandR, Quat::from_rotation_x(f * TAU * 2.0));
            p.r(J::Head, bow(-0.3 * f));
            false
        }
        Style::Nymph => {
            // Sways gracefully and opens a hand in welcome.
            p.r(J::Torso, roll((time * 0.8).sin() * 0.05 * k));
            let f = every(time, 8.0, 4.0, 7.0) * k;
            p.r(J::ArmR, roll(-0.9 * f) * fwd(0.45 * f));
            p.r(J::ForeR, fwd(0.25 * f));
            p.r(J::Head, roll(0.15 * f) * turn(-0.2 * f));
            false
        }
        Style::Medusa => {
            // Hips sway; now and then she flexes her claws.
            p.r(J::Hips, roll((time * 0.7).sin() * 0.06 * k));
            p.r(J::Head, roll((time * 0.4).sin() * 0.1 * k));
            let f = every(time, 7.5, 4.0, 6.0) * k;
            p.r(J::ArmL, fwd(0.7 * f) * roll(0.3 * f));
            p.r(J::ForeL, fwd((1.0 + (time * 8.0).sin() * 0.12) * f));
            false
        }
        Style::Phantom => {
            // Hangs just off the floor, arms drifting.
            p.o(J::Lift, Vec3::Y * (0.03 + (time * 1.1).sin() * 0.02) * k);
            p.r(J::ArmL, fwd((0.2 + (time * 0.9).sin() * 0.1) * k));
            p.r(J::ArmR, fwd((0.2 + (time * 0.9 + 1.0).sin() * 0.1) * k));
            false
        }
        Style::Wraith => {
            // Hunched and watchful; a slow reach of one claw.
            p.r(J::Torso, bow(0.08 * k));
            let f = every(time, 9.0, 5.0, 8.0) * k;
            p.r(J::ArmL, fwd(1.2 * f) * roll(0.2 * f));
            p.r(J::ForeL, fwd(0.15 * f));
            p.r(J::Head, bow(-0.15 * f));
            false
        }
        Style::Bat | Style::Kestrel => {
            // Glances about; a kestrel cocks its head.
            p.r(J::Neck, turn(look * 1.2 * k));
            if rig.style == Style::Kestrel {
                let f = every(time, 5.0, 2.0, 3.0) * k;
                p.r(J::Head, roll(0.4 * f));
            }
            true
        }
        Style::Flytrap => {
            p.r(J::Torso, turn(look * 0.6 * k));
            true
        }
        Style::Griffin | Style::Dragon | Style::Jabberwock | Style::Aquator | Style::Xeroc => {
            let sway = (time * 0.6).sin();
            p.r(J::Tail, turn((time * 1.4).sin() * 0.3 * k));
            p.r(J::Body, roll(sway * 0.02 * k));
            p.r(J::Neck, turn(look * 0.5 * k));
            match rig.style {
                Style::Dragon => {
                    // Lifts its head to snort smoke and resettles its wings.
                    let f = every(time, 7.0, 3.5, 5.5) * k;
                    p.r(J::Neck, bow(-0.25 * f));
                    p.r(J::Jaw, bow(0.25 * f));
                    mantle(p, 0.15 * every(time + seed, 9.0, 1.0, 2.2) * k);
                }
                Style::Griffin => {
                    // Preens under one wing.
                    let f = every(time, 9.0, 4.5, 7.5) * k;
                    p.r(J::Neck, turn(1.1 * f) * bow(0.55 * f));
                    p.r(J::Head, bow(0.35 * f) * roll((time * 9.0).sin() * 0.1 * f));
                    p.r(J::WingL, roll(-0.3 * f));
                }
                Style::Jabberwock => {
                    // Jerks its head about and chatters its jaw.
                    let jerk = ((time * 1.1).sin() * 4.0).round() * 0.1;
                    p.r(J::Neck, turn(jerk * k));
                    let f = every(time, 5.0, 2.5, 3.6) * k;
                    p.r(J::Jaw, bow((0.35 + (time * 22.0).sin() * 0.25) * f));
                }
                Style::Aquator => {
                    // Gulps, its throat working.
                    let f = every(time, 6.0, 3.0, 3.8) * k;
                    p.r(J::Jaw, bow(0.35 * f));
                    p.r(J::Head, bow(-0.12 * f));
                }
                _ => {
                    // Lies in wait, its mouth slowly widening.
                    let f = every(time, 8.0, 3.0, 6.5) * k;
                    p.r(J::Jaw, bow(0.45 * f));
                }
            }
            true
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Antics while the player is away: true when handled here.

pub(super) fn antics(p: &mut Pose, rig: &Rig, time: f32, gag: u32, progress: f32, k: f32) -> bool {
    let beat = time * TAU * 1.6;
    match (rig.style, gag % 2) {
        (Style::Orc, 0) => {
            // Flexes both arms, grinning at the room.
            let pump = (time * 3.0).sin().abs();
            p.r(J::ArmL, roll((1.5 + 0.1 * pump) * k));
            p.r(J::ArmR, roll(-(1.5 + 0.1 * pump) * k));
            p.r(J::ForeL, fwd((1.7 + 0.3 * pump) * k));
            p.r(J::ForeR, fwd((1.7 + 0.3 * pump) * k));
            p.r(J::Torso, bow(-0.15 * k));
            p.o(J::Lift, Vec3::Y * 0.02 * pump * k);
        }
        (Style::Troll, 0) => {
            // Drums on its belly.
            let drum = (time * 10.0).sin();
            p.r(J::ArmL, fwd((0.6 + 0.2 * drum) * k));
            p.r(J::ArmR, fwd((0.6 - 0.2 * drum) * k));
            p.r(J::ForeL, fwd(1.3 * k));
            p.r(J::ForeR, fwd(1.3 * k));
            p.r(J::Head, bow(-0.35 * k));
            p.r(J::Torso, bow(-0.15 * k));
        }
        (Style::Leprechaun, 0) => {
            // A quick jig with heel clicks in the air.
            let b = beat.sin();
            let hop = (beat * 2.0).sin().abs();
            p.o(J::Lift, Vec3::Y * 0.12 * hop * k);
            p.r(J::LegL, fwd(0.6 * b.max(0.0) * k) * roll(-0.25 * hop * k));
            p.r(J::LegR, fwd(0.6 * (-b).max(0.0) * k) * roll(0.25 * hop * k));
            p.r(J::ShinL, fwd(-0.9 * hop * k));
            p.r(J::ShinR, fwd(-0.9 * hop * k));
            p.r(J::ArmL, roll(0.3 * k) * fwd(0.4 * k));
            p.r(J::ArmR, roll(-0.3 * k) * fwd(0.4 * k));
            p.r(J::ForeL, fwd(1.5 * k));
            p.r(J::ForeR, fwd(1.5 * k));
        }
        (Style::Leprechaun, _) => {
            // Spins so fast it all but vanishes, then bows.
            p.r(J::Lift, turn(spins(progress, 8.0)));
            p.r(J::ArmL, roll(1.2 * k));
            p.r(J::ArmR, roll(-1.2 * k));
            let bow_out = ease((progress - 0.85) / 0.15) * k;
            p.r(J::Torso, bow(0.6 * bow_out));
        }
        (Style::Nymph, 0) => {
            // A turning dance, arms raised.
            p.r(J::Lift, turn(spins(progress, 3.0)));
            p.o(J::Lift, Vec3::Y * 0.04 * beat.sin().abs() * k);
            p.r(J::ArmL, roll((2.4 + 0.2 * beat.sin()) * k));
            p.r(J::ArmR, roll(-(2.0 - 0.2 * beat.sin()) * k));
            p.r(J::ForeL, roll(-0.4 * k));
            p.r(J::ForeR, roll(0.4 * k));
            p.r(J::LegL, fwd(0.3 * k));
            p.r(J::Head, bow(-0.25 * k));
        }
        (Style::Medusa, 0) => {
            // A charmer's sway, arms undulating like her serpents.
            let w = (time * 2.0).sin();
            p.r(J::Hips, roll(0.12 * w * k));
            p.r(J::Torso, roll(-0.14 * w * k));
            p.r(J::ArmL, roll((1.3 + 0.3 * w) * k));
            p.r(J::ArmR, roll(-(1.3 - 0.3 * w) * k));
            p.r(J::ForeL, roll(-0.6 * w * k));
            p.r(J::ForeR, roll(-0.6 * w * k));
            p.r(J::Head, roll(0.15 * w * k));
        }
        (Style::Phantom, 0) | (Style::Wraith, 0) => {
            // Rises and drifts round in a slow circle.
            p.r(J::Lift, turn(spins(progress, 1.0)));
            p.o(J::Lift, Vec3::Y * (0.32 + (time * 1.3).sin() * 0.05) * k);
            p.r(J::ArmL, roll(0.9 * k) * fwd(0.3 * k));
            p.r(J::ArmR, roll(-0.9 * k) * fwd(0.3 * k));
            for (leg, shin) in [(J::LegL, J::ShinL), (J::LegR, J::ShinR)] {
                p.r(leg, fwd(0.3 * k));
                p.r(shin, fwd(-0.5 * k));
            }
        }
        (Style::Phantom, _) | (Style::Wraith, _) => {
            // Boo: arms up, looming forward.
            let loom = (time * 1.5).sin().abs();
            p.r(J::ArmL, fwd((2.4 + 0.2 * loom) * k));
            p.r(J::ArmR, fwd((2.4 + 0.2 * loom) * k));
            p.r(J::ForeL, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.6 * k));
            p.r(J::Torso, bow(0.3 * loom * k));
            p.o(J::Lift, Vec3::new(0.0, 0.08 * k, 0.15 * loom * k));
        }
        (Style::Orc | Style::Troll | Style::Nymph | Style::Medusa, _) => {
            if rig.style == Style::Medusa {
                // Tosses her serpent hair, hand on hip.
                p.r(J::Head, roll((time * 3.0).sin() * 0.35 * k) * bow(-0.2 * k));
                p.r(J::ArmL, roll(0.6 * k) * fwd(-0.3 * k));
                p.r(J::ForeL, fwd(1.6 * k));
            } else {
                stretch(p, time, k);
            }
        }
        (Style::Bat | Style::Kestrel, 0) => {
            // Loops the loop.
            let a = spins(progress, 1.0);
            p.o(
                J::Lift,
                Vec3::new(0.0, (1.0 - a.cos()) * 0.4, a.sin() * 0.4),
            );
            p.r(J::Lift, Quat::from_rotation_x(-a));
        }
        (Style::Bat | Style::Kestrel, _) => {
            // A barrel roll, then another.
            p.r(J::Lift, Quat::from_rotation_z(spins(progress, 2.0)));
            p.o(J::Lift, Vec3::Y * 0.2 * k);
        }
        (Style::Flytrap, 0) => {
            // Dances on its stem.
            let w = beat.sin();
            p.r(J::Neck, roll(0.35 * w * k));
            p.r(J::Torso, roll(-0.5 * w * k) * bow(0.15 * k));
            p.r(J::Head, roll(0.3 * w * k));
        }
        (Style::Flytrap, _) => {
            // Snaps at a fly only it can see.
            let snap = (time * 6.0).sin().max(0.0);
            p.r(
                J::Torso,
                turn((time * 1.7).sin() * 0.6 * k) * bow(0.3 * snap * k),
            );
            p.r(J::Head, bow(-0.5 * (1.0 - snap) * k));
            p.r(J::Jaw, bow(0.7 * (1.0 - snap) * k));
        }
        (Style::Dragon | Style::Griffin, 0) => {
            // Rears, beating its wings.
            p.r(J::Lift, Quat::from_rotation_x(-0.55 * k));
            p.o(J::Lift, Vec3::new(0.0, 0.18, -0.3) * k * rig.legs);
            let flap = (time * 4.0).sin();
            mantle(p, (1.0 + 0.35 * flap) * k);
            p.r(J::FL, fwd(1.0 * k));
            p.r(J::FR, fwd(1.0 * k));
            p.r(J::Neck, bow(-0.4 * k));
            p.r(J::Jaw, bow(0.5 * k));
        }
        (Style::Jabberwock, 0) => {
            // Jabbers: arms flailing, jaw going, bouncing on its haunches.
            let b = beat.sin();
            p.o(J::Lift, Vec3::Y * 0.06 * b.abs() * k);
            p.r(J::FL, fwd((1.2 + 0.6 * b) * k));
            p.r(J::FR, fwd((1.2 - 0.6 * b) * k));
            p.r(J::FLs, fwd(-0.8 * k));
            p.r(J::FRs, fwd(-0.8 * k));
            p.r(J::Neck, roll(0.3 * b * k) * bow(-0.3 * k));
            p.r(J::Jaw, bow((0.4 + 0.3 * (time * 18.0).sin()) * k));
        }
        (Style::Aquator, 0) => {
            // Frog hops on the spot.
            let hop = (time * 4.0).sin().max(0.0);
            p.o(J::Lift, Vec3::Y * 0.22 * hop * k);
            p.r(J::Body, bow(-0.25 * hop * k));
            for leg in [J::BL, J::BR] {
                p.r(leg, fwd(-0.7 * hop * k));
            }
        }
        (Style::Xeroc, 0) => {
            // Spins on its shell.
            p.r(J::Lift, turn(spins(progress, 3.0)));
            p.o(J::Lift, Vec3::Y * 0.05 * beat.sin().abs() * k);
            p.r(J::Jaw, bow(0.5 * k));
        }
        (Style::Xeroc, _) => {
            // Chomps at the air.
            p.r(J::Jaw, bow(0.9 * (time * 6.0).sin().abs() * k));
            p.r(J::Head, bow(-0.15 * k));
        }
        (Style::Dragon | Style::Griffin | Style::Jabberwock | Style::Aquator, _) => {
            zoomies(p, progress, 0.6, k);
            if rig.style == Style::Aquator {
                p.r(J::Lift, turn(spins(progress, 1.0)));
            }
        }
        _ => return false,
    }
    true
}

// ---------------------------------------------------------------------------
// Segmented growths: a medusa's serpents and an aquator's tendrils.

pub(super) fn place(
    rig: &Rig,
    a: &Anim,
    fun: f32,
    joints: &mut Query<&mut Transform, Without<Rig>>,
) -> bool {
    let (m, t) = a
        .action
        .filter(|_| a.dead.is_none())
        .map_or((None, 0.0), |(m, t)| (Some(m), t));
    let d = ease(a.death_fall());
    let alive = 1.0 - d;
    let (gather, strike) = m.map_or((0.0, 0.0), |m| {
        let (g, s, _) = timing(m, t);
        (g, if m.offensive() { s } else { 0.0 })
    });
    let time = a.time + a.seed * 2.3;
    let (w, gag, _) = antic(a);
    let fun = w * fun * f32::from(u8::from(m.is_none() && a.dead.is_none()));
    match rig.style {
        Style::Medusa => {
            let (gather, strike, aiming) =
                match (a.dead, a.death_entry.and_then(|entry| entry.action)) {
                    (Some(phase), Some((Move::Gaze, gaze_phase))) => {
                        // Preserve just the crown articulation, not the cancelled
                        // gaze action or its gameplay/whole-body presentation.
                        let elapsed = phase.max(0.0) * a.death_motion.duration();
                        let release = 1.0 - ease(elapsed / 0.18);
                        let (g, s, _) = timing(Move::Gaze, gaze_phase);
                        (g * release, s * release, true)
                    }
                    _ => (gather, strike, m == Some(Move::Gaze)),
                };
            for (i, e) in rig.segments.iter().enumerate() {
                let f = i as f32;
                if let Ok(mut transform) = joints.get_mut(*e) {
                    // The front rank turns to the victim at contact. Keep
                    // each head on its own side and leave the rear rank fanned
                    // out, rather than pulling all eleven across the crown.
                    let at = transform.translation;
                    let aim = if aiming && at.z > 0.01 { strike } else { 0.0 };
                    let focus = turn(-at.x.signum() * 0.92 * aim);
                    // Let the rest of the crown keep its individual weave
                    // while the four front heads momentarily hold their aim.
                    let sway = alive * (1.0 - 0.65 * aim);
                    let weave = Quat::from_rotation_x((time * 1.3 + f * 0.9).sin() * 0.22 * sway)
                        * Quat::from_rotation_z((time * 1.1 + f * 1.7).cos() * 0.25 * sway)
                        * Quat::from_rotation_y((time * 2.1 + f).sin() * 0.3 * fun);
                    let rear = Quat::from_rotation_x(-0.35 * gather + 0.5 * strike - 0.32 * aim);
                    let slump = Quat::from_rotation_x(1.1 * d);
                    transform.rotation = slump * focus * rear * weave;
                }
            }
            true
        }
        Style::Aquator => {
            let (gather, strike) =
                touch_fall_entry(a, Accent::Amber).map_or((gather, strike), |(_, phase, fall)| {
                    let (g, s, _) = timing(Move::Touch(Accent::Amber), phase);
                    (g * (1.0 - fall), s * (1.0 - fall))
                });
            for (i, e) in rig.segments.iter().enumerate() {
                let (chain, k) = (i / 4, (i % 4) as f32);
                let side = if chain == 0 { -1.0 } else { 1.0 };
                let sway = (time * 1.4 - k * 0.7 + side).sin() * 0.25;
                // Lead with the rust tips: draw the roots down and back
                // on the gather, then lift the sweep forward at contact.
                // Keep a little trailing curl in the smaller segments.
                let sweep = if k == 0.0 {
                    0.32 * gather - 0.82 * strike
                } else {
                    -0.05 * gather + 0.12 * strike
                };
                let twirl = if gag == 1 {
                    (time * 3.0 + k).sin() * 0.6 * fun
                } else {
                    0.0
                };
                let droop = 0.45 * d;
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.rotation = Quat::from_rotation_x(sweep + droop + 0.08 * k * alive)
                        * Quat::from_rotation_z(side * (sway * alive + twirl));
                }
            }
            true
        }
        _ => false,
    }
}
