use super::anatomy::{BirdBody, FOOT_SCALE, HIP_SCALE};
use super::geometry::{floater_mote, FLOATER_HOVER};
use super::geometry::{
    snake_fang_rotation, snake_ground_height, snake_link, SnakeLivePath, BIRD_ANKLE, SNAKE,
    SNAKE_WARNING_LIFT,
};
use super::*;

mod carriage;
mod curves;
mod foot_trajectory;
pub use carriage::WalkingCarriage;
pub(super) use curves::ease;
use curves::envelope;
use foot_trajectory::beast_step;
mod death;
pub use death::DeathMotion;
mod publication;
mod rogue;

// ---------------------------------------------------------------------------
// Posing.

fn fwd(a: f32) -> Quat {
    Quat::from_rotation_x(-a)
}

/// Tip a joint that points up or ahead (a torso, a head, a neck, a beast's
/// body) forward and down by `a`; negative rears it back. `fwd` is the same
/// motion for limbs, which hang down from their joints.
fn bow(a: f32) -> Quat {
    Quat::from_rotation_x(a)
}

/// Rotations and offsets for one frame, keyed by joint.
struct Pose {
    rot: [Quat; JOINTS],
    off: [Vec3; JOINTS],
}

impl Pose {
    fn new() -> Self {
        Self {
            rot: [Quat::IDENTITY; JOINTS],
            off: [Vec3::ZERO; JOINTS],
        }
    }
    fn r(&mut self, j: J, q: Quat) {
        self.rot[j as usize] *= q;
    }
    fn o(&mut self, j: J, v: Vec3) {
        self.off[j as usize] += v;
    }
}

/// When cloaks are fitted against their wearers. Without this resource, or with
/// an interval of zero, every cloak is fitted every frame. Otherwise one wearer
/// is fitted every `interval` frames, each taking its turn, so the cost of a
/// frame does not grow with the crowd; between fits a cloak keeps its last
/// fitted shape on its wearer.
#[derive(Resource, Debug, Default)]
pub(crate) struct CloakPacing {
    interval: u32,
    frame: u32,
    /// Cloak wearers posed in the previous frame.
    wearers: u32,
}

impl CloakPacing {
    pub(crate) fn every(interval: u32) -> Self {
        Self {
            interval,
            ..default()
        }
    }

    /// Whether the cloak wearer posed `ordinal`-th this frame is fitted now.
    fn fits(&self, ordinal: u32) -> bool {
        let wearers = self.wearers.max(1);
        self.interval == 0
            || (self.frame.is_multiple_of(self.interval)
                // Match one ordinal, not its residue: newly added wearers must not
                // repeat slots from the previous frame's smaller population.
                && (self.frame / self.interval) % wearers == ordinal)
    }

    fn finish_frame(&mut self, wearers: u32) {
        self.frame = self.frame.wrapping_add(1);
        self.wearers = wearers;
    }
}

/// Opt-in CPU attribution for the latest animation step; no history is retained.
/// Phases are pose/joints, robe fitting, drapes/segments, and hair/contact fitting.
#[derive(Resource, Default, Debug, PartialEq)]
pub(crate) struct MotionProfile {
    pub rigs: usize,
    pub phases: [std::time::Duration; 4],
    pub hair_phases: [std::time::Duration; 4],
}

fn record_phase(
    profile: &mut Option<&mut MotionProfile>,
    phase: usize,
    started: Option<std::time::Instant>,
) {
    if let (Some(profile), Some(started)) = (profile.as_deref_mut(), started) {
        profile.phases[phase] += started.elapsed();
    }
}

/// Where hair solves run, and the camera that orders them.
#[derive(bevy::ecs::system::SystemParam)]
pub struct HairScheduling<'w, 's> {
    camera: Query<'w, 's, &'static GlobalTransform, crate::portrait::ExplorationCamera>,
    solving: Option<Res<'w, super::hair_budget::HairSolving>>,
    cloaks: Option<ResMut<'w, CloakPacing>>,
}

/// Advance timers and pose every creature.
pub fn animate(
    (time, pace): (Res<Time>, Option<Res<crate::playback::Pace>>),
    bored: Option<Res<Boredom>>,
    mut profile: Option<ResMut<MotionProfile>>,
    mut rigs: Query<(&mut Rig, &mut Anim, &Transform)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut joints: Query<&mut Transform, Without<Rig>>,
    mut scheduling: HairScheduling,
) {
    let background = scheduling
        .solving
        .is_some_and(|s| *s == super::hair_budget::HairSolving::Background);
    if let Some(profile) = profile.as_deref_mut() {
        *profile = MotionProfile::default();
    }
    // Authored poses and effects share elapsed presentation time. A slow frame
    // must not let a projectile land before the matching hurt or death pose.
    let elapsed = crate::playback::Pace::of(pace, time.delta_secs());
    // A held presentation frame must retain contact history and cloth transforms,
    // not re-evaluate the pose with a zero step.
    if elapsed <= 0.0 {
        return;
    }
    // Only numerical smoothing and the contact/cloth solve need a bounded step.
    let dt = elapsed.min(0.05);

    // Left alone for a minute, everyone starts to amuse themselves.
    let fun = bored.map_or(0.0, |b| ease((b.0 - BORED_AFTER) / 3.0));
    let eye = scheduling
        .camera
        .iter()
        .next()
        .map(GlobalTransform::translation);
    // Wearers whose hair awaits a contact solve, with their head frames and
    // distance from the camera; solved together once every pose is set.
    let mut wearers = Vec::new();
    let mut cloaked = 0;
    for (mut rig, mut anim, root) in &mut rigs {
        // Actor roots are unparented. Their current Transform already includes
        // this Update's interpolation; GlobalTransform still describes last frame.
        let at = root.translation;
        let first = anim.time == 0.0;
        let delta = at - anim.last;
        let moved = delta.with_y(0.0);
        anim.last = at;
        // The dead lie still, and a body just placed hasn't moved at all.
        // A jump of more than half a cell in one frame is a placement, not a walk.
        let placed = first || moved.length() > 0.5;
        let step = if placed || anim.dead.is_some() {
            0.0
        } else {
            moved.length()
        };
        let target_speed = (step / dt.max(1e-4) / (4.0 * rig.scale.max(0.2))).clamp(0.0, 1.0);
        anim.speed += (target_speed - anim.speed) * (1.0 - (-dt * 10.0).exp());
        // Fliers never touch the floor; a rooted plant never steps.
        let supported = matches!(
            rig.plan,
            Plan::Biped | Plan::Bird | Plan::Quadruped | Plan::Centaur
        ) && !rig.style.flies()
            && !placed
            && delta.y.abs() <= 0.001
            && !anim.quiet
            && anim.action.is_none()
            && anim.dead.is_none();
        // A flinch or a sidestep keeps the feet where they stand: dropping
        // their support would snap them from the floor in a single frame.
        if !supported || anim.contacts.rig != rig.joint(J::Lift) {
            anim.contacts = WalkContacts::default();
        }
        // Shorter slow steps need a correspondingly shorter distance cycle.
        // The support solve itself is not weakened a second time by speed.
        let stride = if supported {
            walk_stride(anim.speed)
        } else {
            1.0
        };
        anim.walk += step * TAU * 0.9 * rig.stride / (rig.scale.max(0.2) * stride);
        if step > 0.002 && anim.action.is_none() {
            anim.target_yaw = Some(moved.x.atan2(moved.z));
        }
        // Scene death chooses a corridor-aware heading during the negative
        // pre-roll. Consume that target before the fall, then freeze facing;
        // zero/settled death must still discard stale locomotion targets.
        if anim.dead.is_some_and(|phase| phase >= 0.0) {
            anim.target_yaw = None;
        }
        if let Some(goal) = anim.target_yaw {
            let mut d = (goal - anim.yaw) % TAU;
            if d > PI {
                d -= TAU;
            } else if d < -PI {
                d += TAU;
            }
            anim.yaw += d * (1.0 - (-dt * 12.0).exp());
        }
        anim.time += elapsed;
        if let Some((m, t)) = anim.action {
            let t = t + elapsed / m.duration();
            anim.action = (t < 1.0).then_some((m, t));
        }
        anim.hurt = anim.hurt.map(|t| t + elapsed).filter(|t| *t < 0.4);
        anim.dodge = anim.dodge.map(|t| t + elapsed).filter(|t| *t < 0.45);
        let death_motion = anim.death_motion;
        if let Some(d) = anim.dead.as_mut() {
            *d = death_motion.advance(*d, elapsed);
            anim.fallen = death_motion.fall(*d);
        } else {
            // A revived creature gets up over most of a second.
            anim.fallen = (anim.fallen - elapsed / 0.8).max(0.0);
        }
        let moving = step > dt * 0.04 * rig.scale.max(0.2);
        let fit_cloaks = rig.cloaks.is_empty()
            || scheduling.cloaks.as_ref().is_none_or(|pacing| {
                cloaked += 1;
                pacing.fits(cloaked - 1)
            });
        let hair = present_pose(
            &mut rig,
            &mut anim,
            fun,
            dt,
            supported,
            moving,
            &mut meshes,
            &mut joints,
            profile.as_deref_mut(),
            fit_cloaks,
            if background {
                HairWork::Defer
            } else {
                HairWork::Now
            },
        );
        if let Some(frame) = hair {
            let distance = eye.map_or(0.0, |eye| eye.distance(root.translation));
            wearers.push((rig, frame, distance));
        }
    }
    if let Some(pacing) = scheduling.cloaks.as_deref_mut() {
        pacing.finish_frame(cloaked);
    }
    solve_hair(wearers, dt, &mut meshes, profile.as_deref_mut());
}

/// Whether a pose fits its hair at once or leaves it to the frame's budget.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HairWork {
    Now,
    Defer,
}

/// Background hair: write the solves that have finished, then send the
/// wearers that waited longest to the free workers (see `hair_budget`).
fn solve_hair(
    mut wearers: Vec<(Mut<Rig>, Affine3A, f32)>,
    dt: f32,
    meshes: &mut Assets<Mesh>,
    mut profile: Option<&mut MotionProfile>,
) {
    if wearers.is_empty() {
        return;
    }
    let started = profile.as_ref().map(|_| std::time::Instant::now());
    let mut busy = 0;
    let waiting: Vec<_> = wearers
        .iter_mut()
        .map(|(rig, _, distance)| {
            let hair = rig.hair.as_mut().expect("only wearers of hair wait");
            if let Some(Some(solved)) = hair.receive(false) {
                if let Some(profile) = profile.as_deref_mut() {
                    for (total, stage) in profile.hair_phases.iter_mut().zip(solved.stages()) {
                        *total += stage;
                    }
                }
                super::hair_contact::apply(solved, meshes);
            }
            hair.wait += dt;
            busy += usize::from(hair.is_away());
            let wait = if hair.is_away() { 0.0 } else { hair.wait };
            super::hair_budget::Waiting {
                wait,
                distance: *distance,
            }
        })
        .collect();
    let workers =
        bevy::tasks::AsyncComputeTaskPool::get_or_init(bevy::tasks::TaskPool::default).thread_num();
    let timing = profile.is_some();
    for index in super::hair_budget::choose(&waiting, workers.saturating_sub(busy)) {
        let (rig, frame, _) = &mut wearers[index];
        let hair = rig.hair.as_mut().expect("chosen wearers have hair");
        // A solve covers all the motion since the last one; the chain's
        // recovery is exponential in time, so it catches up.
        let step = hair.wait.min(super::hair_budget::MAX_STEP);
        hair.send(*frame, step, timing);
    }
    record_phase(&mut profile, 3, started);
}

/// An explicit state/composition change needs a pose at the current timestamp.
/// Unlike a simulation step, this request never advances animation time.
#[derive(Component)]
pub(crate) struct PoseRefresh;

pub(crate) fn refresh_pose(
    mut commands: Commands,
    bored: Option<Res<Boredom>>,
    mut rigs: Query<(Entity, &mut Rig, &mut Anim), With<PoseRefresh>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut joints: Query<&mut Transform, Without<Rig>>,
) {
    let fun = bored.map_or(0.0, |b| ease((b.0 - BORED_AFTER) / 3.0));
    for (entity, mut rig, mut anim) in &mut rigs {
        // A replacement rig cannot inherit another rig's foot ownership.
        if anim.contacts.rig != rig.joint(J::Lift) {
            anim.contacts = WalkContacts::default();
        }
        let supported = anim.contacts.rig.is_some() && anim.action.is_none() && anim.dead.is_none();
        present_pose(
            &mut rig,
            &mut anim,
            fun,
            0.0,
            supported,
            false,
            &mut meshes,
            &mut joints,
            None,
            true,
            HairWork::Now,
        );
        commands.entity(entity).remove::<PoseRefresh>();
    }
}

#[allow(clippy::too_many_arguments)]
fn present_pose(
    rig: &mut Rig,
    anim: &mut Anim,
    fun: f32,
    dt: f32,
    supported: bool,
    moving: bool,
    meshes: &mut Assets<Mesh>,
    joints: &mut Query<&mut Transform, Without<Rig>>,
    mut profile: Option<&mut MotionProfile>,
    fit_cloaks: bool,
    hair_work: HairWork,
) -> Option<Affine3A> {
    if let Some(profile) = profile.as_deref_mut() {
        profile.rigs += 1;
    }
    let started = profile.as_ref().map(|_| std::time::Instant::now());
    let pose = pose(rig, anim, fun, dt, supported, moving);
    if dt > 0.0 {
        anim.contacts.strike_entry = if supported && moving {
            strike_entry_from_walk(&pose, rig, anim)
        } else {
            None
        };
    }
    for j in 0..JOINTS {
        if let Some(e) = rig.joints[j] {
            if let Ok(mut transform) = joints.get_mut(e) {
                let (translation, rotation) = if j == J::Lift as usize {
                    // Lunges and falls are in the creature's own frame.
                    let facing = Quat::from_rotation_y(anim.yaw);
                    (facing * pose.off[j] * rig.scale, facing * pose.rot[j])
                } else {
                    (rig.rest[j] + pose.off[j], pose.rot[j])
                };
                publication::joint(&mut transform, translation, rotation);
            }
        }
    }
    if let Some(veil) = &rig.veil {
        let frame = |j: J| {
            Affine3A::from_rotation_translation(
                pose.rot[j as usize],
                rig.rest[j as usize] + pose.off[j as usize],
            )
        };
        // Up and the floor height are unchanged by the actor's facing yaw.
        // These matrices keep the animated sheet in actor-local floor space.
        let lift = Affine3A::from_rotation_translation(
            pose.rot[J::Lift as usize],
            pose.off[J::Lift as usize] * rig.scale,
        ) * Affine3A::from_scale(Vec3::splat(rig.scale));
        let torso = lift * frame(J::Hips) * frame(J::Torso);
        let scale = Affine3A::from_scale(super::anatomy::storybook_part(Plan::Biped, J::Torso));
        let head = torso
            * frame(J::Head)
            * Affine3A::from_translation(-rig.rest[J::Head as usize])
            * scale;
        if let Ok(mut transform) = joints.get_mut(veil.train) {
            let train = veil.pose(torso * scale, head, anim.death_fall());
            publication::joint(&mut transform, train.translation, train.rotation);
        }
    }
    record_phase(&mut profile, 0, started);
    // Retained skirt layers must expose their current synchronized transform
    // before the robe/outer-panel contact solver consumes them.
    let started = profile.as_ref().map(|_| std::time::Instant::now());
    place_segments(rig, anim, fun, joints);
    swing_drapes(rig, anim, dt, joints);
    record_phase(&mut profile, 2, started);
    if !rig.hip_garments.is_empty() {
        let roots = [rig.rest[J::LegL as usize], rig.rest[J::LegR as usize]];
        let legs = [J::LegL, J::LegR].map(|j| {
            Affine3A::from_rotation_translation(
                pose.rot[j as usize],
                rig.rest[j as usize] + pose.off[j as usize],
            )
        });
        let pivot = rig.rest[J::Torso as usize];
        let torso = Affine3A::from_rotation_translation(
            pose.rot[J::Torso as usize],
            pivot + pose.off[J::Torso as usize],
        ) * Affine3A::from_translation(-pivot);
        // Ease out with the last supported step, including when an action
        // interrupts it. An action cue alone must not snap the skirt shut.
        let cloth_travel = ease((anim.speed - 0.006) * 10.0).max(ease(anim.contacts.blend));
        let relief = (anim.dead.is_none()
            && cloth_travel > 0.0
            && rig
                .hip_garments
                .iter()
                .any(|panel| panel.leg_relief.is_some()))
        .then(|| {
            let leg_points = |i: usize| {
                let (_, shin, foot) = BIPED_LEGS[i];
                let sk = rig.rest[shin as usize];
                let fk = rig.rest[foot as usize];
                let knee = legs[i].transform_point3(sk + pose.off[shin as usize]);
                let ankle = knee
                    + legs[i].transform_vector3(
                        pose.rot[shin as usize] * (fk + pose.off[foot as usize]),
                    );
                (knee - roots[i] - sk, ankle - roots[i] - sk - fk)
            };
            let [left, right] = [0, 1].map(leg_points);
            let knee_y = roots[0].y + rig.rest[J::ShinL as usize].y;
            super::skirt_motion::ReliefLegs {
                knee_y,
                ankle_y: knee_y + rig.rest[J::FootL as usize].y,
                knee: [left.0, right.0].map(|d| d * cloth_travel),
                ankle: [left.1, right.1].map(|d| d * cloth_travel),
            }
        });
        let relief = if anim.dead.is_some() {
            rig.hip_garments
                .iter()
                .find_map(skirt_motion::Panel::last_relief)
                .and_then(|last| last.settling(dt))
        } else {
            relief
        };
        super::skirt_motion::place(
            &mut rig.hip_garments,
            roots,
            legs,
            torso,
            relief,
            |entity| joints.get(entity).ok().map(Transform::compute_affine),
            meshes,
        );
    }
    let has_apron = !rig.aprons.is_empty();
    let has_lower_cloth = rig.robe.is_some() || has_apron;
    let Rig { rest, robe, .. } = rig;
    let started = (profile.is_some() && has_lower_cloth).then(std::time::Instant::now);
    if let Some(robe) = robe {
        robe.update_layer_transforms(&|entity| joints.get(entity).ok().map(|t| t.compute_affine()));
        super::robe::place(robe, rest, &pose.rot, &pose.off, meshes);
        if has_apron {
            robe.publish_overlay_baselines(meshes);
        }
    }
    if !rig.aprons.is_empty() {
        let support = super::visual_contact::posed_triangles(&rig.apron_contacts, |joint| {
            rig.joint(joint)
                .and_then(|e| joints.get(e).ok())
                .map(Transform::compute_affine)
        });
        super::apron::place(
            &mut rig.aprons,
            &support,
            |entity| joints.get(entity).ok().map(Transform::compute_affine),
            meshes,
        );
    }
    record_phase(&mut profile, 1, started);
    let started = profile.as_ref().map(|_| std::time::Instant::now());
    let needs_cloth_contacts = fit_cloaks && rig.cloaks.iter().any(|cloak| cloak.needs_contacts());
    let contacts = if rig.hair.is_some() || needs_cloth_contacts {
        super::visual_contact::posed(&rig.visual_contacts, |joint| {
            rig.joint(joint)
                .and_then(|entity| joints.get(entity).ok())
                .map(Transform::compute_affine)
        })
    } else {
        Vec::new()
    };
    let cloth_surface = needs_cloth_contacts.then(|| {
        std::sync::Arc::new(super::cloak::ContactSurface::new(
            super::visual_contact::posed_triangles(&rig.visual_contacts, |joint| {
                rig.joint(joint)
                    .and_then(|entity| joints.get(entity).ok())
                    .map(Transform::compute_affine)
            }),
        ))
    });
    let head_surface = (needs_cloth_contacts && !rig.cloth_head_contacts.is_empty()).then(|| {
        std::sync::Arc::new(super::cloak::ContactSurface::new(
            super::visual_contact::posed_triangles(&rig.cloth_head_contacts, |joint| {
                rig.joint(joint)
                    .and_then(|entity| joints.get(entity).ok())
                    .map(Transform::compute_affine)
            }),
        ))
    });
    for cloak in rig.cloaks.iter_mut().filter(|_| fit_cloaks) {
        if cloak.needs_contacts() {
            cloak.set_contacts(contacts.clone());
            if let Some(surface) = &cloth_surface {
                cloak.set_contact_surface(surface.clone());
            }
            if let Some(surface) = &head_surface {
                cloak.set_head_surface(surface.clone());
            }
        }
    }
    if fit_cloaks {
        super::cloak::place_layers(
            &mut rig.cloaks,
            |entity| joints.get(entity).ok().map(|transform| transform.rotation),
            meshes,
        );
    }
    record_phase(&mut profile, 2, started);
    let started = (profile.is_some() && rig.hair.is_some()).then(std::time::Instant::now);
    let head = rig
        .joint(J::Head)
        .and_then(|entity| joints.get(entity).ok())
        .map(Transform::compute_affine);
    let mut deferred = None;
    if let (Some(head), Some(hair)) = (head, &mut rig.hair) {
        hair.set_contacts(contacts);
        if let Ok(drape) = joints.get(hair.entity) {
            let frame = head * drape.compute_affine();
            match hair_work {
                HairWork::Now => super::hair_contact::place_profiled(
                    hair,
                    frame,
                    dt,
                    meshes,
                    profile.as_deref_mut().map(|p| &mut p.hair_phases),
                ),
                HairWork::Defer => deferred = Some(frame),
            }
        }
    }
    record_phase(&mut profile, 3, started);
    deferred
}

/// Hanging cloth and hair follow their wearer with a little lag: a cloak
/// flares back as they travel and trails through a turn; skirts and rags
/// sway with the stride, longer cloth less far; long hair lifts with each
/// step. At rest they barely stir; the dead's lie still. Each eases toward
/// its swing rather than taking it at once, so a sudden turn or stop never
/// snaps it.
fn swing_drapes(rig: &Rig, a: &Anim, dt: f32, joints: &mut Query<&mut Transform, Without<Rig>>) {
    if rig.drapes.is_empty() {
        return;
    }
    let alive = if a.dead.is_none() { 1.0 } else { 0.0 };
    let walk = ease(a.speed * 4.0) * alive;
    let turning = a.target_yaw.map_or(0.0, |goal| {
        ((goal - a.yaw + PI).rem_euclid(TAU) - PI).clamp(-1.0, 1.0)
    }) * alive;
    let stir = (a.time * 1.3 + a.seed).sin() * alive;
    let stride = a.walk.sin();
    // Layers fitted around the same waist retain their relative placement.
    // The longest authored layer sets the response of that garment stack;
    // independent peplums, cloaks and hair keep their own motion.
    let layered_length = rig
        .drapes
        .iter()
        .filter_map(|(entity, kind)| match kind {
            Drape::Skirt(length) if rig.skirt_layers.contains(entity) => Some(*length),
            _ => None,
        })
        .reduce(f32::max);
    for &(e, kind) in &rig.drapes {
        let rotation = match kind {
            Drape::Cloak => {
                Quat::from_rotation_x(0.3 * walk + 0.05 * stride.abs() * walk + 0.012 * stir)
                    * Quat::from_rotation_z(0.3 * turning + 0.04 * stride * walk)
            }
            Drape::Skirt(length) => {
                let length = if rig.skirt_layers.contains(&e) {
                    layered_length.unwrap_or(length)
                } else {
                    length
                };
                let give = 1.0 / (1.0 + 2.5 * length);
                Quat::from_rotation_z((0.09 * stride * walk + 0.2 * turning) * give)
                    * Quat::from_rotation_x((0.1 * walk + 0.008 * stir) * give)
            }
            Drape::Hair => {
                // Back from the crown as she goes, a lift on every step
                // (twice a stride), swinging out against a turn.
                let step = (a.walk * 2.0).cos();
                Quat::from_rotation_x(0.1 * walk + 0.025 * step * walk + 0.012 * stir)
                    * Quat::from_rotation_z(0.16 * turning + 0.035 * stride * walk)
            }
        };
        if let Ok(mut transform) = joints.get_mut(e) {
            // Its own inertia: a seventh of a second or so to follow.
            let follow = 1.0 - (-dt * 7.0).exp();
            let rotation = transform.rotation.slerp(rotation, follow);
            publication::rotation(&mut transform, rotation);
        }
    }
}

fn pose(rig: &Rig, a: &mut Anim, fun: f32, dt: f32, supported: bool, moving: bool) -> Pose {
    let mut p = Pose::new();
    // Once a fall starts, ordinary breathing and gait cannot move the body
    // underneath its death pose. The fall itself is the only moving layer.
    let alive = if a.dead.is_none() { 1.0 } else { 0.0 };
    let s = a.speed * alive;
    let w = a.walk * alive;
    let breath = (a.time * 1.8 + a.seed).sin() * alive;
    let hurt = a
        .hurt
        .filter(|t| *t >= 0.0)
        .map_or(0.0, |t| (1.0 - t / 0.4) * (t / 0.06).min(1.0));
    let dodge = a
        .dodge
        .filter(|t| *t >= 0.0)
        .map_or(0.0, |t| (t / 0.45 * PI).sin());
    let (m, t) = a
        .action
        .filter(|_| a.dead.is_none())
        .map_or((None, 0.0), |(m, t)| (Some(m), t));
    if m.is_none() || !valid_strike_entry(rig, a, t) {
        a.strike_entry = None;
    }
    // At rest each creature idles in its own way, fading out as it moves or acts.
    // Fidgets belong to rest. Normal travelling speeds already have a full
    // gait even though speed is normalized against a much faster run.
    let settled = f32::from(u8::from(m.is_none() && a.dead.is_none()));
    // Idle fidgets fade out as an action begins and back in after it ends,
    // rather than switching off and on in a single frame mid-gesture.
    a.rest += (settled - a.rest) * (1.0 - (-dt * 7.0).exp());
    // Retain the blend for revival, but living layers stop when death starts.
    // Real deaths can retain rest = 1, unlike discovered corpse construction.
    let calm = (1.0 - ease(s * 4.0).max(a.contacts.blend)) * a.rest * alive;
    let mut bird_plant = 0.0;
    if calm > 0.0 && !a.quiet {
        let time = a.time + a.seed * 3.7;
        if !rogue::idle(&mut p, rig, time, a.seed, calm) {
            if rig.plan == Plan::Bird {
                bird_plant = idle_ground_bird(&mut p, time, calm);
            } else {
                idle(&mut p, rig, time, a.seed, calm);
            }
        }
        if fun > 0.0 {
            let (w, gag, progress) = antic(a);
            let k = w * calm * fun;
            // Every woman has one more antic beside her kind's own: she
            // flirts with whoever is watching.
            let flirts = gag == 2
                && rig.plan == Plan::Biped
                && rig.humanoid.is_some_and(|body| body.bust.is_some())
                && rig.style != Style::Zombie;
            if w > 0.0 && flirts {
                flirt(&mut p, a.time, progress, k);
            } else if w > 0.0 && !rogue::antics(&mut p, rig, a.time, gag, progress, k) {
                antics(&mut p, rig, a.time, gag, progress, k);
            }
        }
    }
    let world_walk = supported && (moving || a.contacts.rig.is_some());
    match rig.plan {
        Plan::Biped => biped_pose(&mut p, rig, s, w, breath, m.map(|m| (m, t)), a.carriage),
        Plan::Bird if rig.style.flies() => {
            if a.dead.is_none() {
                rogue::flier_pose(&mut p, rig.style, a.time, a.seed, s, a.action);
            }
        }
        Plan::Bird => {
            bird_pose(&mut p, s, w, breath, m, t, !world_walk, bird_plant);
            if let Some(transfer) = a.strike_entry {
                beast_strike_entry_contacts(&mut p, rig, a, transfer.seed, t);
            }
        }
        Plan::Quadruped | Plan::Centaur => {
            quad_pose(&mut p, rig, s, w, breath, m, t, !world_walk);
            if let Some(transfer) = a.strike_entry {
                beast_strike_entry_contacts(&mut p, rig, a, transfer.seed, t);
            }
        }
        Plan::Rooted => {
            if a.dead.is_none() {
                rogue::rooted_pose(&mut p, a.time, a.seed, a.action);
            }
        }
        Plan::Serpent | Plan::Floater | Plan::Marker => {}
    }
    if world_walk {
        grounded_world_walk(&mut p, rig, a, dt, moving);
    } else if rig.plan == Plan::Biped && a.dead.is_none() {
        if let Some(m) = m {
            grounded_action(&mut p, rig, a, m, t);
        } else if a.hurt.is_none() && a.dodge.is_none() && s > 0.0 {
            grounded_walk(&mut p, rig, a);
        }
    }
    // Reactions, shared by every plan.
    p.r(J::Lift, Quat::from_rotation_x(-0.28 * hurt));
    p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * hurt));
    p.r(J::Head, bow(-0.35 * hurt));
    p.r(J::Neck, bow(-0.3 * hurt));
    let side = if a.seed > 5.0 { 1.0 } else { -1.0 };
    p.o(J::Lift, Vec3::new(side * 0.28 * dodge, 0.0, -0.05 * dodge));
    p.r(J::Lift, Quat::from_rotation_z(-side * 0.25 * dodge));
    if rig.style == Style::Troll && rig.scalp_exposed && !a.quiet {
        let uninterrupted = 1.0 - hurt.max(dodge).max(m.map_or(0.0, |_| ease(t / 0.12)));
        rogue::troll_scratch(&mut p, rig, a.time + a.seed * 3.7, calm * uninterrupted);
    }
    if a.dead.is_some() || a.fallen > 0.0 {
        rogue::release_winged_display(&mut p, rig, a);
        death_pose(&mut p, rig, ease(a.death_fall()), a.seed);
        if let Some(phase) = a.dead {
            death::perform(&mut p, rig, a, phase);
            match rig.plan {
                Plan::Rooted => rogue::release_rooted_pose(&mut p, a),
                Plan::Bird if rig.style.flies() => rogue::release_flier_pose(&mut p, rig.style, a),
                Plan::Quadruped if rig.style == Style::Aquator => {
                    release_aquator_lunge(&mut p, rig, a)
                }
                _ => {}
            }
        }
    }
    p
}

/// Shared contact timing for legs, pelvis and arms. Family differences change
/// the support duration and pickup, without changing distance-driven foot speed.
struct BipedGait {
    cadence: f32,
    stance: f32,
    clearance: f32,
    crouch: f32,
    arm: f32,
    bounce: f32,
}

fn biped_gait(style: Style) -> BipedGait {
    let (cadence, stance, clearance, crouch, arm, bounce) = match style {
        Style::Guardian => (1.4, 0.67, 0.06, 0.085, 0.12, 0.008),
        Style::Yeti => (1.25, 0.65, 0.075, 0.1, 0.18, 0.012),
        Style::Zombie => (1.5, 0.67, 0.055, 0.11, 0.0, 0.009),
        Style::Hobgoblin => (1.15, 0.6, 0.095, 0.095, 0.27, 0.014),
        Style::Vampire => (1.18, 0.63, 0.065, 0.07, 0.18, 0.006),
        Style::Caster => (1.0, 0.57, 0.08, 0.075, 0.18, 0.011),
        Style::Sentinel => (1.0, 0.57, 0.09, 0.075, 0.2, 0.011),
        Style::Scout => (1.05, 0.55, 0.11, 0.08, 0.3, 0.013),
        Style::Orc => (1.12, 0.6, 0.09, 0.09, 0.26, 0.014),
        Style::Troll => (1.3, 0.64, 0.07, 0.1, 0.15, 0.012),
        Style::Leprechaun => (0.92, 0.52, 0.12, 0.07, 0.32, 0.018),
        Style::Nymph => (0.96, 0.53, 0.12, 0.07, 0.3, 0.016),
        Style::Medusa => (1.1, 0.6, 0.07, 0.07, 0.15, 0.008),
        // The dead glide, barely lifting their feet.
        Style::Phantom => (1.2, 0.62, 0.04, 0.05, 0.05, 0.004),
        Style::Wraith => (1.25, 0.63, 0.04, 0.05, 0.04, 0.004),
        _ => (1.0, 0.55, 0.11, 0.075, 0.28, 0.014),
    };
    BipedGait {
        cadence,
        stance,
        clearance,
        crouch,
        arm,
        bounce,
    }
}

const BIPED_LEGS: [(J, J, J); 2] = [(J::LegL, J::ShinL, J::FootL), (J::LegR, J::ShinR, J::FootR)];

fn biped_rest_contacts(rig: &Rig) -> [Vec3; 2] {
    BIPED_LEGS.map(|(leg, shin, foot)| {
        rig.rest[J::Hips as usize]
            + rig.rest[leg as usize]
            + rig.rest[shin as usize]
            + rig.rest[foot as usize]
    })
}

fn biped_walk_contacts(rig: &Rig, a: &Anim) -> [Vec3; 2] {
    let gait = biped_gait(rig.style);
    let blend = ease(a.speed * 4.0);
    let mut contacts = biped_rest_contacts(rig);
    for (i, (leg, _, _)) in BIPED_LEGS.into_iter().enumerate() {
        let drag = rig.style == Style::Zombie && leg == J::LegR;
        let stance = gait.stance + if drag { 0.06 } else { 0.0 };
        let clearance = if drag { 0.018 } else { gait.clearance };
        let span = stance / (0.9 * rig.stride.max(0.1) * gait.cadence);
        let phase = a.walk * gait.cadence / TAU + i as f32 * 0.5;
        let (z, lift) = beast_step(phase, stance, span, clearance);
        contacts[i] += Vec3::new(0.0, lift, z) * blend;
    }
    let selection = if a.action.is_some() {
        WalkingCarriage::Native
    } else {
        a.carriage
    };
    let carriage = carriage::resolve(rig, selection);
    if carriage.narrowing > 0.0 {
        for contact in &mut contacts {
            contact.x *= 1.0 - carriage.narrowing * blend;
        }
    }
    contacts
}

/// Ankles follow a distance-driven support/return path. The same small solve
/// also supports action contacts; there is no separate set of attack leg lengths.
fn grounded_walk(p: &mut Pose, rig: &Rig, a: &Anim) {
    let gait = biped_gait(rig.style);
    let blend = ease(a.speed * 4.0);
    p.off[J::Lift as usize].y -= gait.crouch * blend;
    solve_biped_contacts(p, rig, &biped_walk_contacts(rig, a), blend);
}

fn walk_stride(speed: f32) -> f32 {
    ease(speed * 4.0).max(0.2)
}

#[derive(Clone, Copy)]
struct WalkLeg {
    body: J,
    thigh: J,
    shin: J,
    foot: J,
    rest: Vec3,
    floor: f32,
    phase: f32,
    stance: f32,
    span: f32,
    clearance: f32,
    bend: f32,
}

fn quadruped_cadence(style: Style) -> f32 {
    // The compact Quagga needs shorter steps so the hind hoof does not
    // overtrack into the fore hoof. Let it complete steps more frequently at
    // the same travelling speed, rather than moving an already planted foot.
    if style == Style::Quagga {
        1.62
    } else {
        1.35
    }
}

/// The existing family timing, expressed once for the shared support memory.
fn walk_legs(rig: &Rig, a: &Anim) -> [Option<WalkLeg>; 4] {
    let mut legs = [None; 4];
    let mut add = |i: usize,
                   body: J,
                   thigh: J,
                   shin: J,
                   foot: J,
                   phase: f32,
                   stance: f32,
                   cadence: f32,
                   clearance: f32,
                   bend: f32,
                   floor: Option<f32>| {
        if rig.joint(foot).is_some() && rig.supports(foot) {
            let rest = rig.rest[body as usize]
                + rig.rest[thigh as usize]
                + rig.rest[shin as usize]
                + rig.rest[foot as usize];
            legs[i] = Some(WalkLeg {
                body,
                thigh,
                shin,
                foot,
                rest,
                floor: floor.unwrap_or(rest.y),
                phase,
                stance,
                span: stance / (0.9 * rig.stride.max(0.1) * cadence),
                clearance,
                bend,
            });
        }
    };
    match rig.plan {
        Plan::Biped => {
            let gait = biped_gait(rig.style);
            for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
                let drag = rig.style == Style::Zombie && i == 1;
                add(
                    i,
                    J::Hips,
                    leg,
                    shin,
                    foot,
                    a.walk * gait.cadence / TAU + i as f32 * 0.5,
                    gait.stance + if drag { 0.06 } else { 0.0 },
                    gait.cadence,
                    if drag { 0.018 } else { gait.clearance },
                    1.0,
                    None,
                );
            }
        }
        Plan::Bird => {
            for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
                add(
                    i,
                    J::Body,
                    leg,
                    shin,
                    foot,
                    a.walk * 1.6 / TAU + i as f32 * 0.5,
                    0.6,
                    1.6,
                    0.1,
                    1.0,
                    Some(BirdBody::ANKLE_HEIGHT + 0.002),
                );
            }
        }
        Plan::Quadruped | Plan::Centaur => {
            let trot = ease((a.speed - 0.38) / 0.45);
            let legs = rig.legs;
            let cadence = quadruped_cadence(rig.style);
            for (i, (leg, shin, foot, offset)) in [
                (J::FL, J::FLs, J::HoofFL, 0.0),
                (J::FR, J::FRs, J::HoofFR, 0.5),
                (J::BL, J::BLs, J::HoofBL, 0.25 + 0.25 * trot),
                (J::BR, J::BRs, J::HoofBR, -0.25 + 0.25 * trot),
            ]
            .into_iter()
            .enumerate()
            {
                add(
                    i,
                    J::Body,
                    leg,
                    shin,
                    foot,
                    a.walk * cadence / TAU + offset,
                    0.69 - 0.12 * trot,
                    cadence,
                    if rig.style == Style::Unicorn {
                        0.11
                    } else {
                        0.075 * legs
                    },
                    rig.support_bend(shin, foot, if i < 2 { 1.0 } else { -1.0 }),
                    Some(rig.ankle_floor),
                );
            }
        }
        _ => {}
    }
    if rig.plan == Plan::Biped
        && !matches!(rig.style, Style::Zombie | Style::Guardian | Style::Yeti)
        && a.carriage != WalkingCarriage::Native
    {
        // Explicit profiles guide future landings; already planted feet keep
        // their world-space ownership. Native retains the historical path.
        let narrowing = carriage::resolve(rig, a.carriage).narrowing * ease(a.speed * 4.0);
        for leg in legs.iter_mut().flatten() {
            leg.rest.x *= 1.0 - narrowing;
        }
    }
    legs
}

fn heading_between(from: f32, to: f32, weight: f32) -> f32 {
    from + ((to - from + PI).rem_euclid(TAU) - PI) * weight
}

fn walk_hip(p: &Pose, rig: &Rig, leg: WalkLeg) -> Vec3 {
    p.rot[J::Lift as usize]
        * (rig.rest[leg.body as usize]
            + p.off[leg.body as usize]
            + p.rot[leg.body as usize] * (rig.rest[leg.thigh as usize] + p.off[leg.thigh as usize]))
        + p.off[J::Lift as usize]
}

fn posed_ankle(p: &Pose, rig: &Rig, leg: WalkLeg) -> Vec3 {
    let thigh = p.rot[J::Lift as usize] * p.rot[leg.body as usize] * p.rot[leg.thigh as usize];
    walk_hip(p, rig, leg)
        + thigh
            * (rig.rest[leg.shin as usize]
                + p.off[leg.shin as usize]
                + p.rot[leg.shin as usize]
                    * (rig.rest[leg.foot as usize] + p.off[leg.foot as usize]))
}

const ACTION_SUPPORT_IN: f32 = 0.08;
const STRIKE_ENTRY_RELEASE_SECS: f32 = 0.16;

fn strike_entry_release(elapsed: f32, planted: bool) -> f32 {
    // Let the free foot reach the action path first. Delay the old stance
    // crossover a little, but retain an interior sample even at 50 ms.
    let (start, end) = if planted {
        (0.015, STRIKE_ENTRY_RELEASE_SECS)
    } else {
        (0.0, 0.115)
    };
    ease((elapsed - start) / (end - start))
}

fn strike_entry_action(rig: &Rig) -> Option<Move> {
    match (rig.plan, rig.style) {
        (Plan::Biped, Style::Hero) | (Plan::Bird, Style::Emu) => Some(Move::Strike),
        (Plan::Quadruped, Style::Aquator) => Some(Move::Touch(Accent::Amber)),
        _ => None,
    }
}

fn strike_entry_from_walk(p: &Pose, rig: &Rig, a: &Anim) -> Option<StrikeEntry> {
    let action = strike_entry_action(rig)?;
    if a.action.is_some()
        || a.contacts.blend != 1.0
        || a.contacts.settle.is_some()
        || a.hurt.is_some()
        || a.dodge.is_some()
        || a.fallen > 0.0
    {
        return None;
    }
    let lift = rig.joint(J::Lift)?;
    if a.contacts.rig != Some(lift) {
        return None;
    }
    let legs = walk_legs(rig, a);
    let facing = Quat::from_rotation_y(a.yaw);
    let snapshot = |i: usize| {
        let (leg, contact) = (legs[i]?, a.contacts.feet[i]?);
        let chain = sole_parent(p, leg.body, leg.thigh, leg.shin);
        Some(StrikeEntryFoot {
            // Save the actual presented swing pose, not its last ground anchor.
            ankle: a.last + facing * posed_ankle(p, rig, leg) * rig.scale,
            sole: facing * chain * p.rot[leg.foot as usize],
            planted: contact.phase.rem_euclid(1.0) < leg.stance,
        })
    };
    let feet = [snapshot(0)?, snapshot(1)?];
    let rear_feet = if rig.plan == Plan::Quadruped {
        Some([snapshot(2)?, snapshot(3)?])
    } else {
        None
    };
    feet.iter()
        .chain(rear_feet.iter().flatten())
        .any(|foot| foot.planted)
        .then_some(StrikeEntry {
            rig: lift,
            time: a.time,
            root: a.last,
            action,
            feet,
            rear_feet,
        })
}

fn valid_strike_entry(rig: &Rig, a: &Anim, phase: f32) -> bool {
    a.strike_entry.is_some_and(|transfer| {
        let elapsed = phase * transfer.seed.action.duration();
        let since_cue = a.time - transfer.cue_time;
        let moved = a.last - transfer.seed.root;
        (0.0..STRIKE_ENTRY_RELEASE_SECS).contains(&elapsed)
            && a.action.is_some_and(|(action, _)| action == transfer.seed.action)
            && strike_entry_action(rig) == Some(transfer.seed.action)
            && rig.joint(J::Lift) == Some(transfer.seed.rig)
            // The cue may be applied after this presentation step. The
            // phase owns easing; the clock only rejects a pinned stale cue.
            && (0.0..=STRIKE_ENTRY_RELEASE_SECS + 0.05).contains(&since_cue)
            && moved.y.abs() <= 0.001
            && moved.with_y(0.0).length_squared() <= 0.25
            && a.hurt.is_none()
            && a.dodge.is_none()
            && a.fallen == 0.0
    })
}

fn sole_parent(p: &Pose, body: J, thigh: J, shin: J) -> Quat {
    p.rot[J::Lift as usize] * p.rot[body as usize] * p.rot[thigh as usize] * p.rot[shin as usize]
}

/// Hand off the solved feet, not live walking. The ordinary action still
/// owns the destinations and their clearance; only the initial world-pose
/// contribution has a longer release, converging before impact.
fn strike_entry_contacts(
    p: &mut Pose,
    rig: &Rig,
    a: &Anim,
    seed: StrikeEntry,
    planned: &[Vec3; 2],
    elapsed: f32,
) {
    let facing = Quat::from_rotation_y(a.yaw).inverse();
    let legs = walk_legs(rig, a);
    let releases = seed
        .feet
        .map(|foot| strike_entry_release(elapsed, foot.planted));
    let mut contacts = *planned;
    for (i, foot) in seed.feet.into_iter().enumerate() {
        let carried = facing * (foot.ankle - a.last) / rig.scale;
        let carried = if foot.planted {
            legs[i].map_or(carried, |leg| reachable_walk_contact(p, rig, leg, carried))
        } else {
            carried
        };
        contacts[i] = carried.lerp(planned[i], releases[i]);
    }
    fit_biped_support(p, rig, &contacts, 1.0);
    solve_biped_contacts(p, rig, &contacts, 1.0);
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        let parent = sole_parent(p, J::Hips, leg, shin);
        let carried = parent.inverse() * facing * seed.feet[i].sole;
        p.rot[foot as usize] = carried.slerp(p.rot[foot as usize], releases[i]);
    }
}

/// Ground-bird pecks and Aquator corrosion both keep their authored local
/// action contacts. Only their starting positions come from the solved walk.
fn beast_strike_entry_contacts(p: &mut Pose, rig: &Rig, a: &Anim, seed: StrikeEntry, phase: f32) {
    let elapsed = phase * seed.action.duration();
    let facing = Quat::from_rotation_y(a.yaw).inverse();
    let feet = seed
        .feet
        .into_iter()
        .chain(seed.rear_feet.into_iter().flatten());
    for (leg, foot) in walk_legs(rig, a).into_iter().zip(feet) {
        let Some(leg) = leg else { continue };
        let release = strike_entry_release(elapsed, foot.planted);
        if release >= 1.0 {
            continue;
        }
        let carried = facing * (foot.ankle - a.last) / rig.scale;
        let target = carried.lerp(posed_ankle(p, rig, leg), release);
        let planned_sole = sole_parent(p, leg.body, leg.thigh, leg.shin) * p.rot[leg.foot as usize];
        beast_limb(
            p,
            leg.thigh,
            leg.shin,
            leg.foot,
            rig.rest[leg.body as usize],
            rig.rest[leg.thigh as usize],
            rig.rest[leg.shin as usize],
            rig.rest[leg.foot as usize],
            target,
            leg.bend,
            1.0,
        );
        let parent = sole_parent(p, leg.body, leg.thigh, leg.shin);
        let sole = parent.inverse() * facing * foot.sole;
        let planned = if seed.rear_feet.is_some() {
            parent.inverse() * planned_sole
        } else {
            p.rot[leg.foot as usize]
        };
        p.rot[leg.foot as usize] = sole.slerp(planned, release);
    }
}

/// Support holds a world point. Swing releases it along a clear arc and
/// approaches the next landing with the nominal backwards contact velocity.
fn walking_target(a: &Anim, leg: WalkLeg, foot: WalkFoot, scale: f32) -> (Vec3, f32) {
    let facing = Quat::from_rotation_y(a.yaw);
    let from = facing.inverse() * (foot.world - a.last) / scale;
    let phase = leg.phase.rem_euclid(1.0);
    if phase < leg.stance {
        return (from, foot.yaw);
    }
    let cycle = leg.phase.floor();
    let start = foot.lifted.max(cycle + leg.stance).min(leg.phase);
    let duration = (cycle + 1.0 - start).max(0.001);
    let t = ((leg.phase - start) / duration).clamp(0.0, 1.0);
    let stride = walk_stride(a.speed);
    let to = leg.rest.with_y(leg.floor) + Vec3::Z * (leg.span * stride * 0.5);
    let mut target = from.lerp(to, ease(t));
    target.y += leg.clearance * stride * (PI * t).sin().powi(2);
    target.z -= leg.span * stride * duration / leg.stance * t * t * (1.0 - t) * (1.0 - 2.0 * t);
    (target, heading_between(foot.yaw, a.yaw, ease(t)))
}

fn settle_progress(foot: WalkFoot, elapsed: f32, count: usize) -> f32 {
    let duration = if count == 2 { 0.16 } else { 0.12 };
    ((elapsed - f32::from(foot.settle_order) * duration) / duration).clamp(0.0, 1.0)
}

fn settling_target(
    a: &Anim,
    leg: WalkLeg,
    foot: WalkFoot,
    elapsed: f32,
    count: usize,
    scale: f32,
) -> (Vec3, f32) {
    let from = Quat::from_rotation_y(-a.yaw) * (foot.world - a.last) / scale;
    if foot.settled {
        return (from, foot.yaw);
    }
    let t = settle_progress(foot, elapsed, count);
    let to = leg.rest.with_y(leg.floor);
    let mut target = from.lerp(to, ease(t));
    let pickup = (from - to).with_y(0.0).length().min(leg.clearance * 0.6);
    target.y += pickup * (PI * t).sin().powi(2);
    (target, heading_between(foot.yaw, a.yaw, ease(t)))
}

/// Confine a contact before IK, retaining its floor height. Extreme turns may
/// yield a little instead of crossing legs, stretching, or retaining a far anchor.
fn reachable_walk_contact(p: &Pose, rig: &Rig, leg: WalkLeg, mut target: Vec3) -> Vec3 {
    let hip = walk_hip(p, rig, leg);
    let upper = rig.rest[leg.shin as usize].length();
    let reach = upper + rig.rest[leg.foot as usize].length() - 0.002;
    target.x = target
        .x
        .clamp(leg.rest.x - upper * 0.32, leg.rest.x + upper * 0.32);
    let across = (target - hip).with_y(0.0);
    let radius = (reach * reach - (target.y - hip.y).powi(2)).max(0.0).sqrt();
    if across.length_squared() > radius * radius {
        let bounded = hip + across.normalize_or(Vec3::Z) * radius;
        target.x = bounded.x;
        target.z = bounded.z;
    }
    target
}

fn grounded_world_walk(p: &mut Pose, rig: &Rig, a: &mut Anim, dt: f32, moving: bool) {
    let mut legs = walk_legs(rig, a);
    if a.contacts.settle.is_none() {
        // Changing the equine walk/trot offset during braking may otherwise
        // run a hind foot's phase backwards across a touchdown.
        for (leg, foot) in legs.iter_mut().zip(a.contacts.feet) {
            if let (Some(leg), Some(foot)) = (leg, foot) {
                leg.phase = leg.phase.max(foot.phase);
            }
        }
    }
    let count = legs.iter().flatten().count();
    if count == 0 {
        a.contacts = WalkContacts::default();
        return;
    }
    let facing = Quat::from_rotation_y(a.yaw);
    let scale = rig.scale.max(0.2);
    let seeded = a.contacts.rig.is_some();
    if moving && (!seeded || a.contacts.settle.is_some()) {
        for (i, leg) in legs
            .into_iter()
            .enumerate()
            .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
        {
            let (at, yaw) = if let (Some(t), Some(foot)) = (a.contacts.settle, a.contacts.feet[i]) {
                settling_target(a, leg, foot, t, count, scale)
            } else {
                (posed_ankle(p, rig, leg).with_y(leg.floor), a.yaw)
            };
            a.contacts.feet[i] = Some(WalkFoot {
                world: a.last + facing * at * scale,
                yaw,
                phase: leg.phase,
                lifted: leg.phase,
                settle_order: 0,
                settled: false,
            });
        }
        a.contacts.rig = rig.joint(J::Lift);
        a.contacts.settle = None;
    } else if !moving && a.contacts.settle.is_none() {
        for (i, leg) in legs
            .into_iter()
            .enumerate()
            .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
        {
            if let Some(mut foot) = a.contacts.feet[i] {
                let (at, yaw) = walking_target(a, leg, foot, scale);
                foot.world = a.last + facing * at * scale;
                foot.yaw = yaw;
                // Land the feet already in swing before moving any support.
                let airborne = leg.phase.rem_euclid(1.0) >= leg.stance;
                foot.settle_order = legs
                    .iter()
                    .enumerate()
                    .filter(|(j, other)| {
                        other.is_some_and(|other| {
                            let other_airborne = other.phase.rem_euclid(1.0) >= other.stance;
                            (other_airborne && !airborne) || (other_airborne == airborne && *j < i)
                        })
                    })
                    .count() as u8;
                foot.settled = false;
                a.contacts.feet[i] = Some(foot);
            }
        }
        a.contacts.settle = Some(0.0);
    }
    if moving {
        a.contacts.blend = (a.contacts.blend + dt / 0.12).min(1.0);
    } else if let Some(t) = a.contacts.settle.as_mut() {
        *t += dt;
        let end = if count == 2 { 0.32 } else { 0.48 };
        if *t > end {
            a.contacts.blend = (a.contacts.blend - dt / 0.14).max(0.0);
        }
    }
    let weight = ease(a.contacts.blend);
    if weight == 0.0 {
        a.contacts = WalkContacts::default();
        return;
    }
    let stride = walk_stride(a.speed);
    let procedural = ease(a.speed * 4.0);
    match rig.plan {
        Plan::Biped => p.off[J::Lift as usize].y -= biped_gait(rig.style).crouch * stride * weight,
        Plan::Bird => p.off[J::Lift as usize].y -= 0.09 * (stride - procedural) * weight,
        _ => p.off[J::Lift as usize].y -= 0.113 * rig.legs * (stride - procedural) * weight,
    }
    let mut targets = [Vec3::ZERO; 4];
    let mut headings = [a.yaw; 4];
    for (i, leg) in legs
        .into_iter()
        .enumerate()
        .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
    {
        let Some(mut foot) = a.contacts.feet[i] else {
            continue;
        };
        if a.contacts.settle.is_none() {
            let crossed = leg.phase.floor() != foot.phase.floor();
            if leg.phase.rem_euclid(1.0) < leg.stance {
                if crossed || foot.phase.rem_euclid(1.0) >= leg.stance {
                    let phase = leg.phase.rem_euclid(1.0);
                    let at = leg.rest.with_y(leg.floor)
                        + Vec3::Z * (leg.span * stride * (0.5 - phase / leg.stance));
                    foot.world = a.last + facing * at * scale;
                    foot.yaw = a.yaw;
                }
            } else if crossed || foot.phase.rem_euclid(1.0) < leg.stance {
                foot.lifted = leg.phase.floor() + leg.stance;
            }
        }
        let (target, yaw) = if let Some(t) = a.contacts.settle {
            settling_target(a, leg, foot, t, count, scale)
        } else {
            walking_target(a, leg, foot, scale)
        };
        targets[i] = target;
        headings[i] = a.yaw + ((yaw - a.yaw + PI).rem_euclid(TAU) - PI).clamp(-0.85, 0.85);
        if let Some(t) = a.contacts.settle {
            if !foot.settled && settle_progress(foot, t, count) >= 1.0 {
                foot.world = a.last + facing * target * scale;
                foot.yaw = headings[i];
                foot.settled = true;
            }
        }
        foot.phase = leg.phase;
        a.contacts.feet[i] = Some(foot);
    }
    if rig.plan == Plan::Biped {
        // Bound a displaced anchor before fitting the pelvis. A tight turn
        // must not demand a deep squat merely to retain an unreachable point.
        for i in 0..2 {
            if let Some(leg) = legs[i] {
                targets[i] = reachable_walk_contact(p, rig, leg, targets[i]);
            }
        }
        fit_biped_support(p, rig, &[targets[0], targets[1]], weight);
    }
    for (i, leg) in legs
        .into_iter()
        .enumerate()
        .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
    {
        let target = reachable_walk_contact(p, rig, leg, targets[i]);
        let planted = a.contacts.settle.is_none() && leg.phase.rem_euclid(1.0) < leg.stance;
        if planted || a.contacts.feet[i].is_some_and(|foot| foot.settled) {
            if let Some(foot) = a.contacts.feet[i].as_mut() {
                foot.world = a.last + facing * target * scale;
                foot.yaw = headings[i];
            }
        }
        if rig.plan == Plan::Biped {
            targets[i] = target;
        } else {
            beast_limb(
                p,
                leg.thigh,
                leg.shin,
                leg.foot,
                rig.rest[leg.body as usize],
                rig.rest[leg.thigh as usize],
                rig.rest[leg.shin as usize],
                rig.rest[leg.foot as usize],
                target,
                leg.bend,
                weight,
            );
        }
    }
    if rig.plan == Plan::Biped {
        solve_biped_contacts(p, rig, &[targets[0], targets[1]], weight);
    }
    for (i, leg) in legs
        .into_iter()
        .enumerate()
        .filter_map(|(i, leg)| leg.map(|leg| (i, leg)))
    {
        p.r(
            leg.foot,
            Quat::from_rotation_y((headings[i] - a.yaw) * weight),
        );
    }
}

fn solve_biped_contacts(p: &mut Pose, rig: &Rig, contacts: &[Vec3; 2], blend: f32) {
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        if rig.joint(foot).is_none() {
            continue;
        }
        let rest_hip = rig.rest[J::Hips as usize];
        let rest_leg = rig.rest[leg as usize];
        let upper = rig.rest[shin as usize].length();
        let lower = rig.rest[foot as usize].length();
        let root = p.rot[J::Lift as usize];
        let hips = p.rot[J::Hips as usize];
        let target = hips.inverse()
            * (root.inverse() * (contacts[i] - p.off[J::Lift as usize])
                - rest_hip
                - p.off[J::Hips as usize])
            - rest_leg
            - p.off[leg as usize];
        let distance = target
            .length()
            .clamp((upper - lower).abs() + 0.001, upper + lower - 0.001);
        let direction = target.normalize_or(Vec3::NEG_Y);
        let along = (upper * upper - lower * lower + distance * distance) / (2.0 * distance);
        let bend = (upper * upper - along * along).max(0.0).sqrt();
        let forward = (Vec3::Z - direction * direction.dot(Vec3::Z)).normalize_or(Vec3::X);
        let knee = direction * along + forward * bend;
        let thigh = Quat::from_rotation_arc(Vec3::NEG_Y, knee.normalize_or(Vec3::NEG_Y));
        let calf = Quat::from_rotation_arc(
            Vec3::NEG_Y,
            (direction * distance - knee).normalize_or(Vec3::NEG_Y),
        );
        // Fade right to zero so both travel and actions can give the individual
        // idle pose back without snapping a knee at a speed/timer threshold.
        p.rot[leg as usize] = p.rot[leg as usize].slerp(thigh, blend);
        p.rot[shin as usize] = p.rot[shin as usize].slerp(thigh.inverse() * calf, blend);
        let actual = root * hips * p.rot[leg as usize] * p.rot[shin as usize];
        p.rot[foot as usize] = p.rot[foot as usize].slerp(actual.inverse(), blend);
    }
}

/// Keep both requested contacts reachable before solving the legs. A visual
/// forward drive can otherwise put the rear ankle outside the leg's sphere,
/// and clamping the IK alone would leave its sole floating above the floor.
fn fit_biped_support(p: &mut Pose, rig: &Rig, contacts: &[Vec3; 2], weight: f32) {
    let root = p.rot[J::Lift as usize];
    let hips = p.rot[J::Hips as usize];
    let mut drop = 0.0f32;
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        if rig.joint(foot).is_none() {
            continue;
        }
        let at = root
            * (rig.rest[J::Hips as usize]
                + p.off[J::Hips as usize]
                + hips * (rig.rest[leg as usize] + p.off[leg as usize]))
            + p.off[J::Lift as usize];
        let reach = rig.rest[shin as usize].length() + rig.rest[foot as usize].length() - 0.006;
        let across = (contacts[i] - at).with_y(0.0).length_squared();
        let height = (reach * reach - across).max(0.0).sqrt();
        drop = drop.max(at.y - contacts[i].y - height);
    }
    p.off[J::Lift as usize].y -= drop * weight;
    if rig.robe.is_some() {
        limit_caster_support(p, rig, contacts);
    }
}

/// Retain the caster's conservative floor-length rest-hem support limit.
/// Account for the whole hip chain, then shorten only an unreachable body
/// drive. Articulated cloth follows the resulting final pose independently.
fn limit_caster_support(p: &mut Pose, rig: &Rig, contacts: &[Vec3; 2]) {
    let root = p.rot[J::Lift as usize];
    let hips = p.rot[J::Hips as usize];
    let up = (root * hips).inverse() * Vec3::Y;
    let body = rig.humanoid.expect("biped support has resolved anatomy");
    let build = body.build;
    let hip_width = body.build * body.hips_width;
    let flare = 0.208 + 0.84 * 0.045;
    let hem_x = (flare * hip_width + 0.009) * HIP_SCALE.x;
    let hem_z = (flare * 0.74 * build + 0.009) * HIP_SCALE.z;
    let origin =
        root * (rig.rest[J::Hips as usize] + p.off[J::Hips as usize]) + p.off[J::Lift as usize];
    let hem_y = origin.y - 0.84 * up.y - hem_x * up.x.abs() - hem_z * up.z.abs();
    let raise = (0.025 - hem_y).max(0.0);
    if raise == 0.0 {
        return;
    }
    p.off[J::Lift as usize].y += raise;
    let mut near = f32::NEG_INFINITY;
    let mut far = f32::INFINITY;
    for (i, (leg, shin, foot)) in BIPED_LEGS.into_iter().enumerate() {
        let at = root
            * (rig.rest[J::Hips as usize]
                + p.off[J::Hips as usize]
                + hips * (rig.rest[leg as usize] + p.off[leg as usize]))
            + p.off[J::Lift as usize];
        let reach = rig.rest[shin as usize].length() + rig.rest[foot as usize].length() - 0.006;
        let across = (at.x - contacts[i].x).powi(2) + (at.y - contacts[i].y).powi(2);
        let span = (reach * reach - across).max(0.0).sqrt();
        let center = contacts[i].z - (at.z - p.off[J::Lift as usize].z);
        near = near.max(center - span);
        far = far.min(center + span);
    }
    if near <= far {
        p.off[J::Lift as usize].z = p.off[J::Lift as usize].z.clamp(near, far);
    }
}

const HERO_STRIKE_LEAD_RETURN: (f32, f32) = (0.715, 0.895);
const HERO_STRIKE_REAR_PICKUP: f32 = 0.92;

/// A lead step has an explicit landing and a separate recovery pickup. Its
/// planted middle never follows the moving pelvis, and it returns exactly home.
fn action_step(t: f32, impact: f32, staged_recovery: bool) -> (f32, f32) {
    let land = (impact - 0.08).max(0.14);
    let (recover, end) = if staged_recovery {
        HERO_STRIKE_LEAD_RETURN
    } else {
        ((impact + 0.38).clamp(0.78, 0.86), 0.985)
    };
    contact_step(t, 0.035, land, recover, end)
}

fn hero_strike_rear_phase(t: f32) -> Option<f32> {
    (HERO_STRIKE_REAR_PICKUP..1.0)
        .contains(&t)
        .then(|| (t - HERO_STRIKE_REAR_PICKUP) / (1.0 - HERO_STRIKE_REAR_PICKUP))
}

fn hero_strike_rear_lift(phase: f32) -> f32 {
    // A short relief arc over the existing horizontal return.
    0.065 * (PI * phase).sin().powi(2)
}

fn level_strike_rear_sole(p: &mut Pose, phase: f32) {
    let (leg, shin, foot) = BIPED_LEGS[1];
    // Use the same solved-chain counter-rotation as grounded support. Keep
    // the sole level through the middle of pickup, then ease back to the
    // authored free pose instead of letting the blend tip the toes down.
    let chain = p.rot[J::Lift as usize]
        * p.rot[J::Hips as usize]
        * p.rot[leg as usize]
        * p.rot[shin as usize];
    let level = envelope(phase, 0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0);
    p.rot[foot as usize] = p.rot[foot as usize].slerp(chain.inverse(), level);
}

fn contact_step(t: f32, start: f32, land: f32, recover: f32, end: f32) -> (f32, f32) {
    if t < start || t >= end {
        (0.0, 0.0)
    } else if t < land {
        let phase = (t - start) / (land - start);
        (ease(phase), (PI * phase).sin().powi(2))
    } else if t < recover {
        (1.0, 0.0)
    } else {
        let phase = (t - recover) / (end - recover);
        (1.0 - ease(phase), (PI * phase).sin().powi(2))
    }
}

/// Downward extent of the existing terminal foot after a toe-down turn.
/// The ellipse support formula bounds its sampled blobs; the stone foot uses
/// its box envelope. This includes the mesh-only 1.22 toe/sole depth scale.
fn biped_foot_depth(rig: &Rig, angle: f32) -> f32 {
    let (s, c) = angle.sin_cos();
    if rig.style == Style::Guardian {
        return 0.075 * c + 0.21 * FOOT_SCALE.z * s;
    }
    let body = rig.humanoid.expect("biped support has resolved anatomy");
    let mut depth = body
        .feet
        .into_iter()
        .fold(f32::NEG_INFINITY, |depth, (center, radii)| {
            let z = center.z * FOOT_SCALE.z;
            let rz = radii.z * FOOT_SCALE.z;
            depth.max(-center.y * c + z * s + Vec2::new(radii.y * c, rz * s).length())
        });
    if rig.style == Style::Yeti {
        depth = depth
            .max(0.04 * c + 0.17 * FOOT_SCALE.z * s + 0.012 * FOOT_SCALE.z)
            .max(0.06 * c + 0.22 * FOOT_SCALE.z * s);
    }
    depth
}

/// Plant the front foot before carrying the rear ankle back, then reverse
/// that order to rise. At full depth the rear knee and tucked toe support the
/// body; a two-flat-sole solve alone would replace the kneel with a squat.
fn mend_contacts(p: &mut Pose, rig: &Rig, t: f32, weight: f32) -> ([Vec3; 2], f32) {
    let mut contacts = biped_rest_contacts(rig);
    let kneel = envelope(t, 0.0, 0.25, 0.8, 1.0);
    if rig.style == Style::Caster {
        // Upright channeling retains the caster's restrained lower-body pose.
        // The articulated garment then follows the final supported legs.
        p.off[J::Lift as usize].y =
            p.off[J::Lift as usize].y * (1.0 - weight) - 0.025 * kneel * weight;
        return (contacts, 0.0);
    }
    let build = rig
        .humanoid
        .expect("biped support has resolved anatomy")
        .build;
    let knee_height = match rig.style {
        Style::Guardian => 0.104,
        Style::Sentinel => (0.076 * build + 0.003).max(0.104),
        Style::Yeti => 0.08 * build + 0.003,
        _ => 0.076 * build + 0.003,
    };
    let upper = rig.rest[J::ShinR as usize].length();
    let lower = rig.rest[J::FootR as usize].length();
    let knee_z = -0.06f32;
    let tuck = 1.05;
    let neutral_depth = biped_foot_depth(rig, 0.0);
    let tucked_y = contacts[1].y + biped_foot_depth(rig, tuck) - neutral_depth + 0.003;
    let rear_z = knee_z
        - (lower * lower - (tucked_y - knee_height).powi(2))
            .max(0.0)
            .sqrt();
    let (front, front_lift) = contact_step(t, 0.015, 0.14, 0.9, 0.985);
    let (rear, rear_lift) = contact_step(t, 0.145, 0.25, 0.8, 0.9);
    let family = if rig.style == Style::Zombie { 0.8 } else { 1.0 };
    contacts[0] += Vec3::new(0.0, 0.045 * front_lift, 0.3 * family * front);
    contacts[1].z += rear_z * rear;
    let foot_angle = tuck * rear;
    contacts[1].y +=
        biped_foot_depth(rig, foot_angle) - neutral_depth + 0.003 * rear + 0.065 * rear_lift;

    // Keep the authored .4-unit descent, adapting its settled depth only for
    // the knee's actual mass. Remove residual travel roll while it bears weight.
    p.rot[J::Lift as usize] = p.rot[J::Lift as usize].slerp(Quat::IDENTITY, kneel * weight);
    let root = p.rot[J::Lift as usize];
    let hips = p.rot[J::Hips as usize];
    let rest_hip = rig.rest[J::Hips as usize] + rig.rest[J::LegR as usize];
    let at = root
        * (rig.rest[J::Hips as usize]
            + p.off[J::Hips as usize]
            + hips * (rig.rest[J::LegR as usize] + p.off[J::LegR as usize]))
        + p.off[J::Lift as usize];
    let across = (Vec3::new(contacts[1].x, knee_height, knee_z) - at)
        .with_y(0.0)
        .length_squared();
    let supported_hip = knee_height + (upper * upper - across).max(0.0).sqrt();
    let wanted = rest_hip.y + (supported_hip - rest_hip.y) * kneel;
    p.off[J::Lift as usize].y += (wanted - at.y) * weight;
    (contacts, foot_angle)
}

fn grounded_action(p: &mut Pose, rig: &Rig, a: &Anim, m: Move, t: f32) {
    let weight = envelope(t, 0.0, ACTION_SUPPORT_IN, 0.92, 1.0);
    if a.speed > 0.0 {
        grounded_walk(p, rig, a);
    }
    if m == Move::Gaze && rig.style == Style::Medusa {
        // Her gaze starts at the shoulders; nothing drives the pelvis.
        // Preserve the standing legs (or the last fading walk) instead of
        // solving her already planted ankles into a new knee bend.
        return;
    }
    let walking = biped_walk_contacts(rig, a);
    if m == Move::Jump {
        // The body drops into the crouch, springs and flies; the feet stay
        // planted until takeoff and tuck beneath it in the air.
        let (gather, rise, tuck) = jump_profile(t);
        let scale = rig.humanoid.map_or(1.0, |body| body.build);
        p.off[J::Lift as usize].y += (rise - 0.16 * gather) * scale;
        let mut contacts = biped_rest_contacts(rig);
        for contact in &mut contacts {
            contact.y += (rise + 0.09 * tuck) * scale;
        }
        fit_biped_support(p, rig, &contacts, 1.0);
        solve_biped_contacts(p, rig, &contacts, 1.0);
        p.r(J::FootL, bow(-0.5 * tuck));
        p.r(J::FootR, bow(-0.5 * tuck));
        return;
    }
    if matches!(m, Move::Step | Move::Mend) {
        let (mut planned, foot_angle) = if m == Move::Mend {
            mend_contacts(p, rig, t, weight)
        } else {
            let mut contacts = biped_rest_contacts(rig);
            let (back, pickup) = contact_step(t, 0.035, 0.35, 0.72, 0.985);
            let hop = (PI * (t / 0.35).clamp(0.0, 1.0)).sin().max(0.0) * 0.05;
            contacts[0].y += hop;
            contacts[1] += Vec3::new(0.0, hop + 0.055 * pickup, -0.38 * back);
            (contacts, 0.0)
        };
        for i in 0..2 {
            planned[i] = walking[i].lerp(planned[i], weight);
        }
        fit_biped_support(p, rig, &planned, weight);
        solve_biped_contacts(p, rig, &planned, weight);
        p.r(J::FootR, bow(foot_angle * weight));
        return;
    }
    let (lead, stride): (usize, f32) = match m {
        Move::Strike => (0, 0.22),
        Move::Thrust => (0, 0.34),
        Move::Bash => (0, 0.32),
        Move::Shove => (0, 0.4),
        Move::Heavy => (0, 0.18),
        Move::Hook => (0, 0.14),
        Move::Touch(_) => (0, 0.25),
        Move::Bite(_) => (0, 0.2),
        Move::Combo(other) => (usize::from(other), 0.14),
        _ => (0, 0.0),
    };
    let mut planned = biped_rest_contacts(rig);
    // Stage only the Hero's normal strike. Finish the lead return after
    // impact, before allowing the rear foot to leave its support.
    let staged_recovery = m == Move::Strike && rig.style == Style::Hero;
    let (advance, pickup) = action_step(t, m.impact(), staged_recovery);
    let (leg, shin, foot) = BIPED_LEGS[lead];
    let length = rig.rest[shin as usize].length() + rig.rest[foot as usize].length();
    let family = if rig.style == Style::Zombie { 0.8 } else { 1.0 };
    let stride = (stride * family).min(length * 0.48);
    if stride > 0.0 {
        let clearance = if matches!(rig.style, Style::Guardian | Style::Zombie) {
            0.035
        } else {
            0.055
        };
        let side = rig.rest[leg as usize].x.signum();
        planned[lead] += Vec3::new(side * 0.016 * advance, clearance * pickup, stride * advance);
    }
    if let Some(transfer) = a.strike_entry.filter(|_| m == Move::Strike) {
        strike_entry_contacts(p, rig, a, transfer.seed, &planned, t * m.duration());
        return;
    }
    let mut contacts = [
        walking[0].lerp(planned[0], weight),
        walking[1].lerp(planned[1], weight),
    ];
    let rear_pickup = if staged_recovery {
        hero_strike_rear_phase(t)
    } else {
        None
    };
    if let Some(phase) = rear_pickup {
        contacts[1].y += hero_strike_rear_lift(phase);
    }
    fit_biped_support(p, rig, &contacts, weight);
    solve_biped_contacts(p, rig, &contacts, weight);
    if let Some(phase) = rear_pickup {
        level_strike_rear_sole(p, phase);
    }
}

/// Seconds without the player's touch before the cast starts to play.
pub const BORED_AFTER: f32 = 60.0;

/// Seconds since the player last touched keyboard or mouse.
#[derive(Resource, Default)]
pub struct Boredom(pub f32);

/// Where a creature is in its round of antics: how far into one (a weight
/// that ramps in and out), which gag, and its progress from 0 to 1.
fn antic(a: &Anim) -> (f32, u32, f32) {
    const PERIOD: f32 = 12.0;
    const START: f32 = 1.0;
    const END: f32 = 8.0;
    let local = match a.antic {
        Some(_) => a.time,
        None => a.time + a.seed * 3.7,
    };
    let t = local.rem_euclid(PERIOD);
    let round = (local / PERIOD).floor() as i64;
    let gag = a
        .antic
        .unwrap_or((round + a.seed as i64).rem_euclid(3) as u32);
    let w = ease((t - START) / 0.7) * ease((END - t) / 0.7);
    (w, gag, ((t - START) / (END - START)).clamp(0.0, 1.0))
}

/// A woman's flirt, over `progress` 0 to 1: a hip popped with her hand
/// on it while she twirls a lock of hair, a kiss blown to whoever is
/// watching, then a shimmy of the shoulders and a swish of the hips.
fn flirt(p: &mut Pose, time: f32, progress: f32, k: f32) {
    let t = progress;
    let pose = envelope(t, 0.0, 0.08, 0.92, 1.0) * k;
    let twirl = envelope(t, 0.02, 0.1, 0.34, 0.42) * k;
    let kiss = envelope(t, 0.38, 0.46, 0.56, 0.6) * k;
    let blow = envelope(t, 0.54, 0.6, 0.66, 0.74) * k;
    let shimmy = envelope(t, 0.7, 0.76, 0.92, 0.98) * k;
    // The hip popped out over her right leg, her right fist resting on it,
    // her head tilted and turned toward the watcher. Her open left hand
    // does the flirting.
    p.r(J::Hips, Quat::from_rotation_z(-0.12 * pose));
    p.r(J::Torso, Quat::from_rotation_z(0.1 * pose));
    p.r(
        J::Head,
        Quat::from_rotation_z(-0.14 * pose) * Quat::from_rotation_y(0.18 * pose),
    );
    p.r(J::LegL, fwd(0.12 * pose));
    p.r(J::ShinL, fwd(-0.2 * pose));
    // Her arms are posed outright for each beat, over whatever her idle
    // or her kind's fidget was doing with them.
    let reach = |p: &mut Pose, j: J, target: Quat, w: f32| {
        p.rot[j as usize] = p.rot[j as usize].slerp(target, w.clamp(0.0, 1.0));
    };
    reach(
        p,
        J::ArmR,
        Quat::from_rotation_z(-0.52) * fwd(-0.32) * Quat::from_rotation_y(1.25),
        pose,
    );
    reach(p, J::ForeR, fwd(1.75), pose);
    // Her left hand up at the side of her head, winding a lock of hair.
    let wind = (time * 7.0).sin();
    // Raised forward first, then out to her side, so it rises beside
    // her head rather than across her face.
    reach(p, J::ArmL, fwd(2.1) * Quat::from_rotation_z(0.65), twirl);
    reach(p, J::ForeL, fwd(2.0), twirl);
    reach(p, J::HandL, Quat::from_rotation_x(0.45 * wind), twirl);
    // The kiss: fingers to her lips, then swept out toward the watcher.
    reach(p, J::ArmL, fwd(1.05) * Quat::from_rotation_z(-0.5), kiss);
    reach(p, J::ForeL, fwd(2.2), kiss);
    reach(p, J::ArmL, fwd(1.5) * Quat::from_rotation_z(0.4), blow);
    reach(p, J::ForeL, fwd(0.25), blow);
    p.r(J::Head, bow(0.08 * kiss - 0.06 * blow));
    // A shimmy of the shoulders over a swish of the hips.
    p.r(
        J::Torso,
        Quat::from_rotation_y((time * 15.0).sin() * 0.12 * shimmy),
    );
    p.r(
        J::Hips,
        Quat::from_rotation_z((time * 5.0).sin() * 0.1 * shimmy),
    );
}

/// A whole number of turns over a gag's progress, so spins start and end square.
fn spins(progress: f32, turns: f32) -> f32 {
    ease(progress) * TAU * turns
}

/// Little bits of fun while the player is away. `k` fades the gag in and
/// out; `progress` runs 0 to 1 over it, for spins and laps that must close.
fn antics(p: &mut Pose, rig: &Rig, time: f32, gag: u32, progress: f32, k: f32) {
    let beat = time * TAU * 1.6;
    match rig.plan {
        Plan::Biped => match (rig.style, gag % 2) {
            (Style::Caster, 0) => {
                // Twirls the staff like a baton.
                p.r(J::ArmR, fwd(0.9 * k) * Quat::from_rotation_z(-0.3 * k));
                p.r(J::ForeR, fwd(0.8 * k));
                p.r(
                    J::HandR,
                    Quat::from_rotation_z(spins(progress, 6.0) * k.signum()),
                );
                p.r(J::Head, Quat::from_rotation_y(-0.3 * k) * bow(0.2 * k));
            }
            (Style::Caster, _) => {
                // Floats cross-legged in meditation.
                p.o(J::Lift, Vec3::Y * (0.28 + (time * 1.7).sin() * 0.04) * k);
                for (leg, shin, side) in [(J::LegL, J::ShinL, 1.0f32), (J::LegR, J::ShinR, -1.0)] {
                    p.r(leg, Quat::from_rotation_y(side * 0.7 * k) * fwd(1.5 * k));
                    p.r(shin, fwd(-2.3 * k));
                }
                for (arm, fore, side) in [(J::ArmL, J::ForeL, 1.0f32), (J::ArmR, J::ForeR, -1.0)] {
                    p.r(arm, Quat::from_rotation_z(side * 0.35 * k) * fwd(0.4 * k));
                    p.r(fore, fwd(1.0 * k));
                }
                p.r(J::Head, bow(0.15 * k));
            }
            (Style::Sentinel, 0) => {
                // Marches on the spot and salutes.
                let step = beat.sin();
                p.r(J::LegL, fwd(1.1 * step.max(0.0) * k));
                p.r(J::ShinL, fwd(-1.3 * step.max(0.0) * k));
                p.r(J::LegR, fwd(1.1 * (-step).max(0.0) * k));
                p.r(J::ShinR, fwd(-1.3 * (-step).max(0.0) * k));
                p.o(J::Lift, Vec3::Y * 0.03 * step.abs() * k);
                p.r(J::ArmR, Quat::from_rotation_z(-1.3 * k) * fwd(0.5 * k));
                p.r(J::ForeR, fwd(2.0 * k));
                p.r(J::Head, bow(-0.1 * k));
            }
            (Style::Sentinel, _) => {
                // Polishes the shield with a gauntlet.
                p.r(J::ArmL, fwd(0.9 * k) * Quat::from_rotation_z(-0.3 * k));
                p.r(J::ForeL, fwd(0.9 * k));
                let rub = time * 9.0;
                p.r(
                    J::ArmR,
                    fwd((0.9 + 0.2 * rub.cos()) * k)
                        * Quat::from_rotation_z((0.25 + 0.25 * rub.sin()) * k),
                );
                p.r(J::ForeR, fwd(1.1 * k));
                p.r(J::Head, bow(0.35 * k) * Quat::from_rotation_y(0.3 * k));
            }
            (Style::Scout, 0) => {
                // Juggles the dagger.
                let toss = (time * 9.0).sin();
                p.r(J::ArmR, fwd(0.7 * k));
                p.r(J::ForeR, fwd((1.0 + 0.6 * toss) * k));
                p.r(
                    J::HandR,
                    Quat::from_rotation_x(spins(progress, 9.0) * k.signum()),
                );
                p.r(J::ArmL, fwd(0.6 * k));
                p.r(J::ForeL, fwd((1.0 - 0.6 * toss) * k));
                p.r(J::Head, bow(-0.35 * k + 0.15 * toss * k));
            }
            (Style::Scout, _) => {
                // Tiptoes in place, looking about with exaggerated stealth.
                let step = (time * 2.6).sin();
                p.o(J::Lift, Vec3::Y * -0.08 * k);
                p.r(J::LegL, fwd(0.9 * step.max(0.0) * k));
                p.r(J::ShinL, fwd(-1.4 * step.max(0.0) * k));
                p.r(J::LegR, fwd(0.9 * (-step).max(0.0) * k));
                p.r(J::ShinR, fwd(-1.4 * (-step).max(0.0) * k));
                p.r(J::Torso, bow(0.35 * k));
                for (arm, fore) in [(J::ArmL, J::ForeL), (J::ArmR, J::ForeR)] {
                    p.r(arm, fwd(0.7 * k));
                    p.r(fore, fwd(1.3 * k));
                }
                p.r(J::Head, Quat::from_rotation_y((time * 1.3).sin() * 0.8 * k));
            }
            (Style::Guardian, _) => {
                // The robot.
                let snap = |x: f32| x.sin().round();
                p.r(J::ArmL, fwd((0.8 + 0.7 * snap(time * 2.0)) * k));
                p.r(
                    J::ForeL,
                    fwd(1.57 * (0.5 + 0.5 * snap(time * 2.0 + 1.0)) * k),
                );
                p.r(J::ArmR, fwd((0.8 - 0.7 * snap(time * 2.0)) * k));
                p.r(
                    J::ForeR,
                    fwd(1.57 * (0.5 - 0.5 * snap(time * 2.0 + 1.0)) * k),
                );
                p.r(J::Head, Quat::from_rotation_y(0.8 * snap(time * 1.5) * k));
                p.r(J::Lift, Quat::from_rotation_y(0.35 * snap(time) * k));
            }
            (Style::Hobgoblin, 1) => {
                // Scratches its head with the club.
                p.r(J::ArmR, Quat::from_rotation_z(-1.7 * k) * fwd(0.5 * k));
                p.r(J::ForeR, fwd((2.1 + (time * 14.0).sin() * 0.15) * k));
                p.r(J::Head, Quat::from_rotation_z(0.3 * k));
                p.r(J::LegL, fwd(0.3 * k) * Quat::from_rotation_z(0.25 * k));
            }
            (Style::Zombie, 0) => {
                // Its head drops off; it catches it and puts it back.
                let drop = ease(progress * 3.0) * ease((1.0 - progress) * 3.0) * k;
                p.o(J::Head, Vec3::new(0.0, -0.62, 0.36) * drop);
                p.r(
                    J::Head,
                    Quat::from_rotation_x(1.2 * drop) * Quat::from_rotation_z(0.6 * drop),
                );
                for arm in [J::ArmL, J::ArmR] {
                    p.r(arm, fwd(1.0 * k));
                }
                p.r(J::ForeL, fwd(0.5 * k));
                p.r(J::ForeR, fwd(0.5 * k));
            }
            (Style::Zombie, _) => {
                // A shuffling graveyard dance, claws up.
                let side = (time * 2.2).sin();
                p.o(J::Lift, Vec3::X * 0.12 * side * k);
                p.r(J::Lift, Quat::from_rotation_z(-0.12 * side * k));
                p.r(J::ArmL, Quat::from_rotation_z(1.0 * k) * fwd(0.6 * k));
                p.r(J::ArmR, Quat::from_rotation_z(-0.4 * k) * fwd(1.2 * k));
                p.r(J::ForeL, fwd(1.2 * k));
                p.r(J::ForeR, fwd(0.9 * k));
                p.r(J::Head, Quat::from_rotation_z(0.35 * side * k));
            }
            (Style::Vampire, 0) => {
                // A swirling pirouette, cloak flared.
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 3.0)));
                p.o(J::Lift, Vec3::Y * 0.06 * k);
                p.r(J::ArmL, Quat::from_rotation_z(1.3 * k));
                p.r(J::ArmR, Quat::from_rotation_z(-1.3 * k));
            }
            (Style::Vampire, _) => {
                // Hides behind the cloak and rises, very dramatically.
                p.o(J::Lift, Vec3::Y * 0.18 * k);
                p.r(J::ArmL, fwd(1.4 * k) * Quat::from_rotation_z(-0.7 * k));
                p.r(J::ForeL, fwd(1.5 * k));
                p.r(J::Head, bow(0.25 * k) * Quat::from_rotation_y(-0.3 * k));
            }
            (_, 0) => jig(p, time, k),
            _ => stretch(p, time, k),
        },
        Plan::Bird => match gag {
            0 => zoomies(p, progress, 0.55, k),
            _ => {
                // Head-bob dance with side steps.
                let b = beat.sin();
                p.r(J::Neck, bow((0.5 + 0.5 * b) * k));
                p.o(
                    J::Lift,
                    Vec3::new(0.08 * (beat * 0.5).sin(), 0.04 * b.abs(), 0.0) * k,
                );
                p.r(J::LegL, fwd(0.4 * b.max(0.0) * k));
                p.r(J::LegR, fwd(0.4 * (-b).max(0.0) * k));
            }
        },
        Plan::Quadruped => match (rig.style, gag) {
            (_, 1) => {
                // Bucks, kicking out behind.
                let kick = (time * 5.0).sin().max(0.0) * k;
                p.r(J::Lift, Quat::from_rotation_x(0.3 * kick));
                p.o(J::Lift, Vec3::Y * 0.05 * kick);
                p.r(J::BL, fwd(-kick));
                p.r(J::BR, fwd(-1.1 * kick));
                p.r(J::Neck, bow(0.6 * kick));
            }
            (Style::Unicorn, _) | (_, 0) => {
                // Rears up, pawing the air.
                p.r(J::Lift, Quat::from_rotation_x(-0.65 * k));
                p.o(J::Lift, Vec3::new(0.0, 0.18, -0.3) * k);
                let paw = (time * 8.0).sin();
                p.r(J::FL, fwd((1.1 + 0.4 * paw) * k));
                p.r(J::FLs, fwd(-1.5 * k));
                p.r(J::FR, fwd((1.1 - 0.4 * paw) * k));
                p.r(J::FRs, fwd(-1.5 * k));
                p.r(J::Neck, bow(-0.4 * k));
            }
            _ => zoomies(p, progress, 0.7, k),
        },
        Plan::Centaur => {
            // A hoof-stamping dance, arms high.
            let b = beat.sin();
            p.r(J::FL, fwd(0.8 * b.max(0.0) * k));
            p.r(J::FR, fwd(0.8 * (-b).max(0.0) * k));
            p.r(J::Torso, Quat::from_rotation_y(0.4 * b * k));
            p.r(J::ArmL, Quat::from_rotation_z(2.4 * k));
            p.r(J::ArmR, Quat::from_rotation_z(-2.4 * k));
            p.r(J::Head, Quat::from_rotation_z(0.2 * b * k));
        }
        Plan::Serpent => {
            if gag != 0 {
                // Chases its own tail.
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 3.0)));
            }
        }
        Plan::Marker | Plan::Rooted => {}
        Plan::Floater => match gag {
            0 => {
                // Loop the loop.
                let a = spins(progress, 2.0);
                p.o(
                    J::Lift,
                    Vec3::new(0.0, (1.0 - a.cos()) * 0.45, a.sin() * 0.45),
                );
            }
            _ => {
                p.r(J::Lift, Quat::from_rotation_y(spins(progress, 5.0)));
                p.o(J::Lift, Vec3::Y * 0.4 * k);
            }
        },
    }
}

/// A jig: hops, kicks and waving arms.
fn jig(p: &mut Pose, time: f32, k: f32) {
    let b = (time * TAU * 1.6).sin();
    p.o(J::Lift, Vec3::Y * 0.07 * b.abs() * k);
    p.r(J::LegL, fwd(0.7 * b.max(0.0) * k));
    p.r(J::ShinL, fwd(-0.9 * b.max(0.0) * k));
    p.r(J::LegR, fwd(0.7 * (-b).max(0.0) * k));
    p.r(J::ShinR, fwd(-0.9 * (-b).max(0.0) * k));
    p.r(J::ArmL, Quat::from_rotation_z((1.6 + 0.5 * b) * k));
    p.r(J::ArmR, Quat::from_rotation_z(-(1.6 - 0.5 * b) * k));
    p.r(J::ForeL, fwd(0.6 * k));
    p.r(J::ForeR, fwd(0.6 * k));
    p.r(J::Torso, Quat::from_rotation_z(0.12 * b * k));
    p.r(J::Head, Quat::from_rotation_z(-0.2 * b * k));
}

/// A long yawn and a stretch, arms over the head.
fn stretch(p: &mut Pose, time: f32, k: f32) {
    let wave = (time * 1.5).sin() * 0.1;
    p.r(J::ArmL, Quat::from_rotation_z((2.7 + wave) * k));
    p.r(J::ArmR, Quat::from_rotation_z(-(2.7 - wave) * k));
    p.r(J::ForeL, Quat::from_rotation_z(0.3 * k));
    p.r(J::ForeR, Quat::from_rotation_z(-0.3 * k));
    p.r(J::Torso, bow(-0.25 * k) * Quat::from_rotation_z(wave * k));
    p.r(J::Head, bow(-0.45 * k));
    p.o(J::Lift, Vec3::Y * 0.03 * k);
}

/// Racing round in a little circle, legs a blur.
fn zoomies(p: &mut Pose, progress: f32, r: f32, k: f32) {
    let a = spins(progress, 2.0);
    p.o(J::Lift, Vec3::new((1.0 - a.cos()) * r, 0.0, a.sin() * r));
    p.r(J::Lift, Quat::from_rotation_y(a));
    let run = progress * 120.0;
    for (j, phase) in [
        (J::LegL, 0.0),
        (J::LegR, PI),
        (J::FL, 0.0),
        (J::FR, PI),
        (J::BL, PI),
        (J::BR, 0.0),
    ] {
        p.r(j, fwd((run + phase).sin() * 0.8 * k));
    }
    p.r(J::Neck, bow(0.4 * k));
}

/// A bell over part of a repeating cycle: 0 outside [start, end] of each
/// `period` seconds, rising smoothly to 1 in the middle.
fn every(time: f32, period: f32, start: f32, end: f32) -> f32 {
    let t = time.rem_euclid(period);
    if t < start || t > end {
        0.0
    } else {
        ((t - start) / (end - start) * PI).sin().powi(2)
    }
}

/// Idle behaviour: breathing, weight shifts and looking about, with a
/// signature fidget every few seconds that fits the creature.
fn idle(p: &mut Pose, rig: &Rig, time: f32, seed: f32, k: f32) {
    let look = (time * 0.37).sin() * 0.3 + (time * 0.13).sin() * 0.2;
    let sway = (time * 0.6).sin();
    match rig.plan {
        Plan::Biped => {
            p.r(J::Lift, Quat::from_rotation_z(sway * 0.025 * k));
            p.r(J::Hips, Quat::from_rotation_z(-sway * 0.04 * k));
            if rig.humanoid.is_some_and(|body| body.woman)
                && !matches!(rig.style, Style::Zombie | Style::Guardian)
            {
                // She rests her weight on one leg, as the women stand on the
                // concept sheets: the hip rises over it, the shoulders and
                // head counter it, and the free knee softens. The side
                // changes now and then, easing across.
                let side = (time * 0.07 + seed).sin().clamp(-0.4, 0.4) / 0.4;
                p.r(J::Hips, Quat::from_rotation_z(0.07 * side * k));
                p.r(J::Torso, Quat::from_rotation_z(-0.08 * side * k));
                p.r(J::Head, Quat::from_rotation_z(0.05 * side * k));
                let (free, shin) = if side > 0.0 {
                    (J::LegR, J::ShinR)
                } else {
                    (J::LegL, J::ShinL)
                };
                p.r(free, fwd(0.1 * side.abs() * k));
                p.r(shin, fwd(-0.16 * side.abs() * k));
                // Now and then, with her weight on her left leg, her free
                // left hand comes to rest on that hip.
                let mood = ease(((time * 0.045 + seed * 1.7).sin() - 0.1) / 0.4);
                let hand = side.max(0.0) * mood * k;
                p.r(
                    J::ArmL,
                    Quat::from_rotation_z(0.52 * hand)
                        * fwd(-0.32 * hand)
                        * Quat::from_rotation_y(-1.25 * hand),
                );
                p.r(J::ForeL, fwd(1.75 * hand));
                // Every so often she tosses her head back and aside; long
                // hair, hung as its own drape, swings with it.
                let beat = (time / 11.0 + seed * 0.37).fract();
                let toss = if beat < 0.07 {
                    (std::f32::consts::PI * beat / 0.07).sin() * k
                } else {
                    0.0
                };
                let aside = if seed.fract() < 0.5 { 1.0 } else { -1.0 };
                p.r(
                    J::Head,
                    Quat::from_rotation_y(0.4 * aside * toss) * bow(-0.22 * toss),
                );
                p.r(J::Torso, Quat::from_rotation_y(-0.08 * aside * toss));
                p.r(J::HandL, Quat::from_rotation_z(-0.4 * hand));
            }
            p.r(J::LegL, Quat::from_rotation_z(sway * 0.03 * k));
            p.r(J::LegR, Quat::from_rotation_z(sway * 0.03 * k));
            p.r(J::Head, Quat::from_rotation_y(look * k));
            match rig.style {
                Style::Hero => {
                    // Lifts the blade to look along its edge.
                    let f = every(time, 8.0, 4.5, 7.0) * k;
                    p.r(J::ArmR, fwd(0.9 * f) * Quat::from_rotation_z(0.25 * f));
                    p.r(J::ForeR, fwd(0.9 * f));
                    p.r(J::HandR, fwd(0.9 * f) * Quat::from_rotation_z(0.6 * f));
                    p.r(J::Head, Quat::from_rotation_y(-0.35 * f) * bow(0.15 * f));
                }
                Style::Caster => {
                    // Raises a palm to the orb, which it studies.
                    let f = every(time, 7.0, 3.5, 6.3) * k;
                    p.r(J::ArmL, fwd(1.1 * f) * Quat::from_rotation_z(0.2 * f));
                    p.r(J::ForeL, fwd(0.8 * f + (time * 5.0).sin() * 0.08 * f));
                    p.r(J::ArmR, fwd(0.25 * f));
                    p.r(J::Head, Quat::from_rotation_y(0.25 * f) * bow(-0.1 * f));
                }
                Style::Sentinel => {
                    // Settles the shield and rolls a shoulder.
                    let f = every(time, 6.5, 3.8, 5.8) * k;
                    p.r(J::ArmL, fwd(0.55 * f));
                    p.r(J::ForeL, fwd(0.7 * f));
                    p.r(J::Torso, Quat::from_rotation_z(0.06 * f));
                }
                Style::Scout => {
                    // Quick glances and a compact grip check. Bring up the arm
                    // and turn the forearm with the hand, rather than spinning
                    // the hand on its cuff while keeping the elbow straight.
                    let dart = ((time * 0.9).sin() * 3.0).clamp(-1.0, 1.0) * 0.45;
                    p.r(J::Head, Quat::from_rotation_y(dart * k));
                    p.o(J::Lift, Vec3::Y * -0.03 * k);
                    let lift = every(time, 5.5, 3.0, 4.2) * k;
                    let turn = every(time, 5.5, 3.14, 4.06) * k;
                    p.r(
                        J::ArmR,
                        fwd(0.36 * lift) * Quat::from_rotation_z(-0.1 * lift),
                    );
                    p.r(
                        J::ForeR,
                        fwd(0.66 * lift) * Quat::from_rotation_y(0.28 * turn),
                    );
                    p.r(
                        J::HandR,
                        Quat::from_rotation_y(0.56 * turn) * fwd(0.14 * turn),
                    );
                }
                Style::Guardian => {
                    // A slow, grinding scan of the room.
                    p.r(
                        J::Head,
                        Quat::from_rotation_y((time * 0.22).sin() * 0.6 * k),
                    );
                    p.r(
                        J::Torso,
                        Quat::from_rotation_y((time * 0.22).sin() * 0.12 * k),
                    );
                }
                Style::Hobgoblin => {
                    // Twitchy, and taps the club on its shoulder.
                    let twitch = ((time * 4.3).sin() * (time * 1.7).sin()).max(0.0) * 0.25;
                    p.r(J::Head, Quat::from_rotation_z(twitch * k));
                    let f = every(time, 6.0, 2.5, 5.0) * k;
                    let tap = (time * 9.0).sin().max(0.0) * 0.15;
                    p.r(J::ArmR, fwd(-0.4 * f) * Quat::from_rotation_z(-0.3 * f));
                    p.r(J::ForeR, fwd((1.9 + tap) * f));
                }
                Style::Zombie => {
                    // Sways and lolls.
                    p.r(
                        J::Lift,
                        Quat::from_rotation_z((time * 0.8).sin() * 0.08 * k),
                    );
                    p.r(
                        J::Head,
                        Quat::from_rotation_z((time * 0.55).sin() * 0.35 * k) * bow(0.15 * k),
                    );
                    p.r(J::ArmL, fwd((time * 0.7).sin() * 0.15 * k));
                    p.r(J::ArmR, fwd((time * 0.7 + 1.5).sin() * 0.15 * k));
                }
                Style::Vampire => {
                    // Still and watchful, with a slow tilt of the head.
                    p.r(
                        J::Head,
                        Quat::from_rotation_z((time * 0.3).sin() * 0.12 * k),
                    );
                    p.o(J::Lift, Vec3::Y * ((time * 0.9).sin() * 0.012 * k));
                    let f = every(time, 9.0, 5.0, 8.0) * k;
                    p.r(J::ArmL, fwd(0.9 * f) * Quat::from_rotation_z(-0.5 * f));
                    p.r(J::ForeL, fwd(1.3 * f));
                }
                Style::Yeti => {
                    // Heavy breaths, and now and then beats its chest.
                    p.r(J::Torso, bow((time * 1.2).sin() * 0.05 * k));
                    let f = every(time, 9.0, 5.5, 7.5) * k;
                    let beat = (time * 12.0).sin();
                    for (side, arm, fore, hand) in [
                        (1.0, J::ArmL, J::ForeL, J::HandL),
                        (-1.0, J::ArmR, J::ForeR, J::HandR),
                    ] {
                        // Keep the elbow out and fold the forearm across
                        // the chest. Each fist comes back as the other winds
                        // up, instead of both fists pushing into empty air.
                        let tap = (1.0 + side * beat) * 0.5;
                        p.r(
                            arm,
                            Quat::from_rotation_y(-side * (0.24 + 0.51 * tap) * f)
                                * fwd((0.13 + 0.52 * (1.0 - tap)) * f),
                        );
                        p.r(fore, fwd((1.05 + 1.23 * tap) * f));
                        p.r(hand, fwd(0.50 * tap * f));
                    }
                    p.r(J::Head, bow(-0.3 * f));
                }
                _ => {}
            }
        }
        Plan::Quadruped | Plan::Centaur => {
            p.r(
                J::Tail,
                Quat::from_rotation_y((time * 2.1).sin() * 0.35 * k),
            );
            p.r(J::Body, Quat::from_rotation_z(sway * 0.02 * k));
            if rig.plan == Plan::Centaur {
                p.r(J::Head, Quat::from_rotation_y(look * k));
                return;
            }
            // Grazes, then lifts its head; stamps a forefoot now and then.
            let graze = every(time, 10.0, 4.0, 8.5) * k;
            let chew = (time * 6.0).sin() * 0.05 * graze;
            p.r(
                J::Neck,
                bow((1.15 + chew) * graze) * Quat::from_rotation_y(look * 0.4 * (k - graze)),
            );
            p.r(J::Snout, bow(0.35 * graze));
            let stamp = every(time + seed, 7.0, 1.0, 1.6) * k;
            p.r(J::FL, fwd(0.5 * stamp));
            p.r(J::FLs, fwd(-0.9 * stamp));
        }
        Plan::Bird | Plan::Serpent | Plan::Floater | Plan::Rooted | Plan::Marker => {}
    }
}

/// Peck with the existing neck reach, but plant the toes before pitching the
/// breast. Ease the support in and out while the load is still small.
fn idle_ground_bird(p: &mut Pose, time: f32, weight: f32) -> f32 {
    let jerk = ((time * 1.3).sin() * 4.0).round() * 0.08;
    p.r(J::Neck, Quat::from_rotation_y(jerk * weight));
    let f = every(time, 5.0, 2.8, 3.8) * weight;
    let peck = (time * 14.0).sin().max(0.0);
    p.r(J::Neck, bow((1.6 + 0.2 * peck) * f));
    p.r(J::Head, bow(0.4 * f));
    p.r(J::Body, bow(0.25 * f));
    envelope(time.rem_euclid(5.0), 2.65, 2.8, 3.8, 3.95) * weight
}

#[allow(clippy::too_many_arguments)]
fn biped_pose(
    p: &mut Pose,
    rig: &Rig,
    s: f32,
    w: f32,
    breath: f32,
    action: Option<(Move, f32)>,
    selection: WalkingCarriage,
) {
    let (m, t) = action.map_or((None, 0.0), |(m, t)| (Some(m), t));
    let zombie = rig.style == Style::Zombie;
    let lumber = matches!(rig.style, Style::Guardian | Style::Yeti);
    let stride = if zombie { 0.38 } else { 0.55 };
    let swing = w.sin() * stride * s;
    p.r(J::LegL, fwd(swing));
    p.r(J::LegR, fwd(-swing * if zombie { 0.6 } else { 1.0 }));
    p.r(J::ShinL, fwd(-((w + 0.9).sin().max(0.0) * 0.9 * s)));
    p.r(J::ShinR, fwd(-((w + 0.9 + PI).sin().max(0.0) * 0.9 * s)));
    let gait = biped_gait(rig.style);
    let blend = ease(s * 4.0);
    let walking = m.is_none();
    let cycle = if walking { w * gait.cadence } else { w };
    // Contact phase zero is left-foot forward. Its opposite shoulder and arm
    // lead at the same time, rather than lagging the new foot path by a quarter.
    // Her arms swing a little less, carried close by the body.
    let poise = if rig.humanoid.is_some_and(|body| body.bust.is_some()) && !zombie {
        0.75
    } else {
        1.0
    };
    let arm_swing = if walking {
        cycle.cos() * gait.arm * blend * poise
    } else {
        swing * 0.7
    };
    let twist = if walking {
        cycle.cos() * 0.08 * blend
    } else {
        w.sin() * 0.12 * s
    };
    let bob = if walking {
        -(cycle * 2.0).cos() * gait.bounce * blend
    } else {
        -(w * 2.0).cos() * 0.016 * s
    };
    p.o(J::Lift, Vec3::Y * bob);
    p.r(
        J::Torso,
        Quat::from_rotation_y(twist) * bow(rig.hunch + 0.08 * s + breath * 0.015),
    );
    if walking {
        p.r(
            J::Hips,
            Quat::from_rotation_y(-cycle.cos() * if lumber { 0.014 } else { 0.025 } * blend),
        );
    }
    if rig.humanoid.is_some_and(|body| body.bust.is_some()) && !zombie {
        // Her carriage: the pelvis tips forward a little, the chest lifts
        // against it and the head levels, the S-curve of the concept sheets.
        p.r(J::Hips, bow(0.06));
        p.r(J::Torso, bow(-0.1));
        p.r(J::Head, bow(0.04));
    }
    let carriage = carriage::resolve(rig, selection);
    if walking && carriage.hips != 0.0 {
        // The shoulders and head counter the pelvis without changing cadence.
        let sway = cycle.sin() * blend;
        p.r(J::Hips, Quat::from_rotation_z(carriage.hips * sway));
        p.r(J::Torso, Quat::from_rotation_z(carriage.torso * sway));
        p.r(J::Head, Quat::from_rotation_z(carriage.head * sway));
    }
    if lumber {
        let roll = if walking {
            -cycle.cos() * 0.03 * blend
        } else {
            w.sin() * 0.06 * s
        };
        p.r(J::Lift, Quat::from_rotation_z(roll));
    }
    if zombie {
        // The arms stay reaching and the head lolls, while the right foot
        // drags through its longer, lower-clearance support/return path.
        p.r(
            J::ArmL,
            fwd(1.25 + breath * 0.05) * Quat::from_rotation_z(0.1),
        );
        p.r(
            J::ArmR,
            fwd(1.15 - breath * 0.05) * Quat::from_rotation_z(-0.1),
        );
        p.r(J::Head, Quat::from_rotation_z(0.25 + breath * 0.05));
        let roll = if walking {
            -cycle.cos() * 0.045 * blend
        } else {
            w.sin() * 0.1 * s
        };
        p.r(J::Lift, Quat::from_rotation_z(roll));
    } else {
        p.r(
            J::ArmL,
            fwd(-arm_swing) * Quat::from_rotation_z(0.1 + breath * 0.01),
        );
        p.r(
            J::ArmR,
            fwd(arm_swing) * Quat::from_rotation_z(-0.1 - breath * 0.01),
        );
        p.r(J::ForeL, fwd(0.25 + 0.2 * s));
        p.r(J::ForeR, fwd(0.3 + 0.2 * s));
    }
    p.r(J::Head, bow(-rig.hunch * 0.7 + breath * 0.02));
    // At rest the weapon hangs point-down from the fist; a move takes it over.
    let busy = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.85, 1.0));
    if !zombie {
        p.r(J::HandR, fwd(-0.8 * (1.0 - busy)));
    }
    let Some(m) = m else {
        return;
    };
    match m {
        Move::Strike => {
            let wind = envelope(t, 0.0, 0.38, 0.38, 0.4);
            let hit = envelope(t, 0.38, 0.5, 0.6, 1.0);
            p.r(
                J::ArmR,
                fwd(-2.1 * wind + 1.25 * hit) * Quat::from_rotation_z(-0.3 * wind),
            );
            p.r(J::ForeR, fwd(0.9 * wind + 0.1 * hit));
            p.r(J::HandR, fwd(-0.5 * wind + 1.2 * hit));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.4 * wind - 0.35 * hit) * bow(-0.1 * wind + 0.25 * hit),
            );
            p.o(J::Lift, Vec3::new(0.0, 0.0, -0.06 * wind + 0.3 * hit));
            p.r(J::LegR, fwd(-0.3 * hit));
            p.r(J::LegL, fwd(0.45 * hit));
        }
        Move::Heavy => {
            let up = envelope(t, 0.0, 0.45, 0.45, 0.48);
            let slam = envelope(t, 0.45, 0.56, 0.7, 1.0);
            for (arm, fore, hand) in [(J::ArmL, J::ForeL, J::HandL), (J::ArmR, J::ForeR, J::HandR)]
            {
                p.r(arm, fwd(-2.7 * up + 1.0 * slam));
                p.r(fore, fwd(0.5 * up));
                p.r(hand, fwd(-0.4 * up + 1.1 * slam));
            }
            p.r(J::ArmL, Quat::from_rotation_z(-0.3 * (up + slam)));
            p.r(J::ArmR, Quat::from_rotation_z(0.3 * (up + slam)));
            p.r(J::Torso, bow(-0.25 * up + 0.55 * slam));
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.05 * up - 0.18 * slam, 0.25 * slam),
            );
            p.r(J::LegL, fwd(0.5 * slam));
            p.r(J::ShinL, fwd(-0.6 * slam));
            p.r(J::ShinR, fwd(-0.4 * slam));
        }
        Move::Cast(_) => {
            let gather = envelope(t, 0.0, 0.4, 0.42, 0.5);
            let release = envelope(t, 0.42, 0.5, 0.65, 1.0);
            p.r(
                J::ArmR,
                fwd(0.9 * gather + 1.5 * release) * Quat::from_rotation_z(-0.2 * gather),
            );
            p.r(
                J::ArmL,
                fwd(1.1 * gather + 0.5 * release) * Quat::from_rotation_z(0.4 * gather),
            );
            p.r(J::ForeL, fwd(1.0 * gather));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.25 * gather - 0.2 * release)
                    * bow(-0.12 * gather + 0.18 * release),
            );
            p.r(J::Head, bow(-0.15 * gather));
            p.o(J::Lift, Vec3::new(0.0, 0.04 * gather, 0.12 * release));
        }
        Move::Hex => {
            let k = envelope(t, 0.0, 0.35, 0.7, 1.0);
            p.r(J::ArmL, fwd(1.3 * k) * Quat::from_rotation_z(0.6 * k));
            p.r(J::ArmR, fwd(1.3 * k) * Quat::from_rotation_z(-0.6 * k));
            p.r(J::ForeL, fwd(-0.2 * k));
            p.r(J::ForeR, fwd(-0.2 * k));
            p.r(
                J::Head,
                bow(0.2 * k) * Quat::from_rotation_z((t * 18.0).sin() * 0.04 * k),
            );
            p.o(J::Lift, Vec3::Y * 0.06 * k);
        }
        Move::Guard(_) => {
            let k = envelope(t, 0.0, 0.2, 0.75, 1.0);
            p.o(J::Lift, Vec3::new(0.0, -0.13 * k, -0.04 * k));
            p.r(J::ArmL, fwd(1.25 * k) * Quat::from_rotation_z(-0.55 * k));
            p.r(J::ForeL, fwd(1.0 * k));
            p.r(J::ArmR, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.6 * k));
            p.r(J::Torso, bow(0.2 * k));
            p.r(J::LegL, fwd(0.45 * k));
            p.r(J::ShinL, fwd(-0.7 * k));
            p.r(J::LegR, fwd(-0.25 * k));
            p.r(J::ShinR, fwd(-0.5 * k));
        }
        Move::Thrust => {
            let draw = envelope(t, 0.0, 0.3, 0.3, 0.34);
            let lunge = envelope(t, 0.32, 0.4, 0.55, 1.0);
            p.r(
                J::ArmR,
                fwd(-0.5 * draw + 1.45 * lunge) * Quat::from_rotation_z(-0.15 * draw),
            );
            p.r(J::ForeR, fwd(1.3 * draw));
            p.r(J::HandR, fwd(0.2 * draw + 1.45 * lunge));
            p.r(
                J::ArmL,
                fwd(-0.6 * lunge) * Quat::from_rotation_z(0.5 * lunge),
            );
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.35 * draw - 0.45 * lunge) * bow(0.15 * lunge),
            );
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.08 * lunge, -0.1 * draw + 0.45 * lunge),
            );
            p.r(J::LegL, fwd(0.75 * lunge));
            p.r(J::ShinL, fwd(-0.5 * lunge));
            p.r(J::LegR, fwd(-0.45 * lunge));
        }
        Move::Combo(other) => {
            // A backhand: the arm crosses the body, then sweeps out.
            let (arm, fore, hand, sign) = if other {
                (J::ArmL, J::ForeL, J::HandL, -1.0)
            } else {
                (J::ArmR, J::ForeR, J::HandR, 1.0)
            };
            let cross = envelope(t, 0.0, 0.3, 0.3, 0.33);
            let sweep = envelope(t, 0.32, 0.42, 0.55, 1.0);
            p.r(
                arm,
                fwd(0.9 * cross + 1.1 * sweep)
                    * Quat::from_rotation_z(sign * (1.1 * cross - 0.9 * sweep)),
            );
            p.r(fore, fwd(1.1 * cross + 0.2 * sweep));
            p.r(hand, fwd(0.6 * sweep));
            p.r(
                J::Torso,
                Quat::from_rotation_y(sign * (-0.5 * cross + 0.55 * sweep)),
            );
            p.o(J::Lift, Vec3::new(0.0, -0.04 * sweep, 0.2 * sweep));
            p.r(J::LegL, fwd(0.3 * sweep));
        }
        Move::Hook => {
            let low = envelope(t, 0.0, 0.35, 0.4, 0.8);
            let rip = envelope(t, 0.38, 0.48, 0.55, 1.0);
            p.o(J::Lift, Vec3::new(0.0, -0.18 * low, 0.25 * rip));
            p.r(
                J::ArmR,
                fwd(0.3 * low + 0.9 * rip) * Quat::from_rotation_z(-0.9 * low + 1.1 * rip),
            );
            p.r(J::ForeR, fwd(0.4 * low + 0.8 * rip));
            p.r(J::HandR, fwd(0.9 * low) * Quat::from_rotation_y(1.2 * rip));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.5 * low - 0.6 * rip) * bow(0.35 * low),
            );
            p.r(J::LegL, fwd(0.6 * low));
            p.r(J::ShinL, fwd(-0.9 * low));
            p.r(J::ShinR, fwd(-0.6 * low));
        }
        Move::Bash => {
            let set = envelope(t, 0.0, 0.3, 0.3, 0.33);
            let punch = envelope(t, 0.32, 0.42, 0.55, 1.0);
            p.r(
                J::ArmL,
                fwd(0.8 * set + 1.45 * punch) * Quat::from_rotation_z(-0.45 * set - 0.2 * punch),
            );
            p.r(J::ForeL, fwd(1.0 * set - 0.1 * punch));
            p.r(J::ArmR, fwd(-0.6 * set));
            p.r(
                J::Torso,
                Quat::from_rotation_y(-0.3 * set + 0.3 * punch) * bow(0.25 * punch),
            );
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.06 * set, -0.08 * set + 0.42 * punch),
            );
            p.r(J::LegL, fwd(0.6 * punch));
            p.r(J::LegR, fwd(-0.4 * punch));
        }
        Move::Shove => {
            let load = envelope(t, 0.0, 0.32, 0.32, 0.35);
            let drive = envelope(t, 0.34, 0.45, 0.6, 1.0);
            for (arm, fore) in [(J::ArmL, J::ForeL), (J::ArmR, J::ForeR)] {
                p.r(arm, fwd(0.7 * load + 1.5 * drive));
                p.r(fore, fwd(1.3 * load - 0.2 * drive));
            }
            p.r(J::ArmL, Quat::from_rotation_z(-0.2 * drive));
            p.r(J::ArmR, Quat::from_rotation_z(0.2 * drive));
            p.r(J::Torso, bow(0.2 * load + 0.45 * drive));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.12 * load - 0.05 * drive, -0.12 * load + 0.5 * drive),
            );
            p.r(J::LegL, fwd(0.5 * load + 0.8 * drive));
            p.r(J::ShinL, fwd(-0.7 * load - 0.3 * drive));
            p.r(J::LegR, fwd(-0.6 * drive));
        }
        Move::Filch => rogue::filch(p, t),
        Move::Gesture => {
            let k = envelope(t, 0.0, 0.25, 0.7, 1.0);
            p.r(J::ArmR, fwd(0.6 * k));
            p.r(J::ForeR, fwd(0.4 * k));
            p.r(J::Head, bow(0.1 * k));
        }
        Move::Focus => {
            let k = envelope(t, 0.0, 0.25, 0.8, 1.0);
            p.r(J::ArmR, fwd(1.2 * k) * Quat::from_rotation_z(0.35 * k));
            p.r(J::ForeR, fwd(1.0 * k));
            p.r(J::HandR, fwd(-0.7 * k));
            p.r(J::ArmL, fwd(0.5 * k) * Quat::from_rotation_z(0.3 * k));
            p.r(
                J::Head,
                Quat::from_rotation_z(0.18 * k * (t * 6.0).sin()) * bow(0.1 * k),
            );
            p.r(J::Torso, Quat::from_rotation_y(0.25 * k));
            // A planted deep crouch puts both knees hard into a floor-length
            // robe. Keep a small weight shift when that garment is actually
            // worn; the hands and torso still carry the channeling gesture.
            let dip = if rig.robe.is_some() { 0.025 } else { 0.08 };
            p.o(J::Lift, Vec3::Y * (-dip * k));
            p.r(J::LegL, fwd(0.35 * k));
            p.r(J::LegR, fwd(-0.3 * k));
            p.r(J::ShinR, fwd(-0.3 * k));
        }
        Move::Fear => {
            let draw = envelope(t, 0.0, 0.3, 0.3, 0.35);
            let cry = envelope(t, 0.33, 0.42, 0.75, 1.0);
            p.r(
                J::ArmL,
                fwd(-0.4 * cry - 0.2 * draw) * Quat::from_rotation_z(1.3 * cry + 0.3 * draw),
            );
            p.r(
                J::ArmR,
                fwd(-0.4 * cry - 0.2 * draw) * Quat::from_rotation_z(-1.3 * cry - 0.3 * draw),
            );
            p.r(J::ForeL, fwd(0.6 * cry));
            p.r(J::ForeR, fwd(0.6 * cry));
            p.r(J::Torso, bow(0.35 * draw - 0.3 * cry));
            p.r(
                J::Head,
                bow(0.3 * draw + 0.35 * cry) * Quat::from_rotation_z((t * 40.0).sin() * 0.05 * cry),
            );
            p.o(J::Lift, Vec3::new(0.0, 0.06 * cry, 0.12 * cry));
        }
        Move::Drain => {
            let reach = envelope(t, 0.0, 0.3, 0.45, 0.6);
            let draw = envelope(t, 0.4, 0.6, 0.85, 1.0);
            if matches!(rig.style, Style::Vampire | Style::Wraith) {
                rogue::drain_with_free_hand(p, reach, draw);
            } else {
                p.r(
                    J::ArmR,
                    fwd(1.5 * reach + 0.9 * draw) * Quat::from_rotation_z(-0.1 * reach),
                );
                p.r(J::ForeR, fwd(-0.1 * reach + 1.6 * draw));
                p.r(
                    J::ArmL,
                    fwd(0.9 * reach) * Quat::from_rotation_z(0.4 * reach),
                );
            }
            p.r(J::Torso, bow(0.25 * reach - 0.2 * draw));
            p.r(J::Head, bow(0.15 * reach - 0.45 * draw));
            let step = if rig.style == Style::Wraith {
                // Pull the energy in with the shoulders, without rocking
                // the long robe back over the planted knees.
                Vec3::Z * (0.13 * reach)
            } else {
                Vec3::new(0.0, 0.05 * draw, 0.25 * reach - 0.1 * draw)
            };
            p.o(J::Lift, step);
        }
        Move::Touch(_) => {
            let coil = envelope(t, 0.0, 0.3, 0.3, 0.35);
            let reach = envelope(t, 0.33, 0.45, 0.6, 1.0);
            p.r(J::ArmR, fwd(0.3 * coil + 1.45 * reach));
            p.r(J::ForeR, fwd(1.2 * coil));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.3 * coil - 0.25 * reach) * bow(0.2 * reach),
            );
            p.o(J::Lift, Vec3::new(0.0, 0.0, -0.08 * coil + 0.35 * reach));
            p.r(J::LegL, fwd(0.45 * reach));
        }
        Move::Step => {
            let back = envelope(t, 0.0, 0.35, 0.7, 1.0);
            // This is one guarded pickup, not a second sine lobe in recovery.
            let lift = (PI * (t / 0.35).clamp(0.0, 1.0)).sin().max(0.0);
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.05 * lift - 0.08 * back, -0.4 * back),
            );
            p.r(J::LegR, fwd(-0.6 * lift));
            p.r(J::ShinR, fwd(-0.8 * lift));
            p.r(
                J::ArmL,
                fwd(1.1 * back) * Quat::from_rotation_z(-0.5 * back),
            );
            p.r(J::ForeL, fwd(0.9 * back));
            p.r(J::ArmR, fwd(0.7 * back));
            p.r(J::ForeR, fwd(0.6 * back));
            p.r(J::Torso, bow(0.15 * back));
        }
        Move::Mend => {
            // Robed casters bind/channel upright; the remaining cast lowers
            // onto a supported knee, hands working a bandage round the arm.
            let kneel = envelope(t, 0.0, 0.25, 0.8, 1.0);
            let wrap = (t * 14.0).sin() * kneel;
            let upright = rig.style == Style::Caster;
            let descent = if upright { 0.025 } else { 0.4 };
            p.o(J::Lift, Vec3::Y * (-descent * kneel));
            if !upright {
                p.r(J::LegL, fwd(1.35 * kneel));
                p.r(J::ShinL, fwd(-1.45 * kneel));
                p.r(J::LegR, fwd(-0.15 * kneel));
                p.r(J::ShinR, fwd(-1.5 * kneel));
            }
            p.r(
                J::ArmL,
                fwd(0.9 * kneel) * Quat::from_rotation_z(-0.35 * kneel),
            );
            p.r(J::ForeL, fwd(1.1 * kneel));
            p.r(
                J::ArmR,
                fwd((0.9 + 0.15 * wrap) * kneel)
                    * Quat::from_rotation_z((0.5 + 0.2 * wrap) * kneel),
            );
            p.r(J::ForeR, fwd(1.3 * kneel));
            p.r(J::Torso, bow((if upright { 0.12 } else { 0.3 }) * kneel));
            p.r(J::Head, bow((if upright { 0.24 } else { 0.45 }) * kneel));
        }
        Move::Breath(_) | Move::Gaze | Move::Bite(_) => rogue::biped_move(p, rig.style, m, t),
        Move::Jump => {
            // The arms swing back into the crouch and up through the
            // spring, ride high in flight and come forward for the landing.
            let (gather, _, tuck) = jump_profile(t);
            // Anticipate takeoff and recover through landing. Deriving arm
            // lift from sqrt(body height) creates unbounded angular speed at
            // both flight boundaries, sweeping attached items through hair.
            let before = 0.7 * envelope(t, 0.0, 0.08, 0.08, 0.20);
            let after = 0.85 * envelope(t, 0.68, 0.80, 0.84, 1.0);
            let up = envelope(t, 0.04, 0.50, 0.50, 0.96);
            for (arm, fore, side) in [(J::ArmL, J::ForeL, 1.0f32), (J::ArmR, J::ForeR, -1.0)] {
                p.r(
                    arm,
                    fwd(-0.75 * before + 2.4 * up + 0.7 * after)
                        * Quat::from_rotation_z(side * (0.18 * up + 0.1 * after)),
                );
                p.r(fore, fwd(0.3 * before + 0.35 * up + 0.5 * after));
            }
            p.r(J::Torso, bow(0.32 * gather - 0.08 * tuck));
            p.r(J::Head, bow(-0.2 * gather + 0.05 * tuck));
        }
    }
}

/// Solve an animal ankle against its contact target in the lift frame, then
/// turn the terminal foot against the actual chain so its sole stays level.
/// The two bone lengths and ankle-to-sole profile come from the original mesh.
#[allow(clippy::too_many_arguments)]
fn beast_limb(
    p: &mut Pose,
    thigh: J,
    shin: J,
    foot: J,
    body_rest: Vec3,
    hip: Vec3,
    upper: Vec3,
    lower: Vec3,
    contact: Vec3,
    bend_side: f32,
    weight: f32,
) {
    if weight <= 0.0 {
        return;
    }
    let root = p.rot[J::Lift as usize];
    let body = p.rot[J::Body as usize];
    let target = body.inverse()
        * (root.inverse() * (contact - p.off[J::Lift as usize])
            - body_rest
            - p.off[J::Body as usize])
        - hip
        - p.off[thigh as usize];
    let upper_len = upper.length();
    let lower_len = lower.length();
    let distance = target.length().clamp(
        (upper_len - lower_len).abs() + 0.001,
        upper_len + lower_len - 0.001,
    );
    let direction = target.normalize_or(Vec3::NEG_Y);
    let along =
        (upper_len * upper_len - lower_len * lower_len + distance * distance) / (2.0 * distance);
    let bend = (upper_len * upper_len - along * along).max(0.0).sqrt();
    let forward = (Vec3::Z - direction * direction.dot(Vec3::Z)).normalize_or(Vec3::X);
    let knee = direction * along + forward * bend * bend_side;
    let upper_rot = Quat::from_rotation_arc(upper.normalize(), knee.normalize());
    let lower_rot = upper_rot.inverse()
        * Quat::from_rotation_arc(lower.normalize(), (direction * distance - knee).normalize());
    p.rot[thigh as usize] = p.rot[thigh as usize].slerp(upper_rot, weight);
    p.rot[shin as usize] = p.rot[shin as usize].slerp(lower_rot, weight);
    let actual = root * body * p.rot[thigh as usize] * p.rot[shin as usize];
    p.rot[foot as usize] = p.rot[foot as usize].slerp(actual.inverse(), weight);
}

#[allow(clippy::too_many_arguments)]
fn bird_pose(
    p: &mut Pose,
    s: f32,
    w: f32,
    breath: f32,
    m: Option<Move>,
    t: f32,
    local_legs: bool,
    idle_support: f32,
) {
    let walk = ease(s * 4.0);
    // Keep the front contact inside both bone lengths even at maximum
    // breast pitch. A shorter, quicker step suits the compact ground bird.
    const CADENCE: f32 = 1.6;
    let cycle = w * CADENCE;
    let action = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.86, 1.0));
    let support = walk.max(action);
    let sway = cycle.sin() * 0.026 * walk;
    let rise = (cycle * 2.0).cos() * 0.012 * walk;
    let pitch = 0.045 * walk + breath * 0.012;
    p.o(
        J::Lift,
        Vec3::new(0.025 * cycle.sin() * walk, -0.09 * support + rise, 0.0),
    );
    p.r(J::Body, Quat::from_rotation_z(sway) * bow(pitch));
    // Carry the eye line while the breast and hips transfer weight beneath it.
    // The quick, purposeful looks in idle() remain the only head twitches.
    p.r(
        J::Neck,
        bow(-pitch * 0.8) * Quat::from_rotation_z(-sway * 0.8),
    );
    p.o(J::Neck, Vec3::Y * (-rise * 0.7));
    p.r(J::Head, bow(-pitch * 0.2));
    let mut kick = 0.0;
    if let Some(m) = m {
        let hit = m.impact();
        let gather = envelope(t, 0.0, hit * 0.55, hit - 0.1, hit);
        let strike = envelope(t, hit - 0.07, hit, hit + 0.06, 1.0);
        if m.offensive() {
            if matches!(m, Move::Heavy | Move::Shove | Move::Hook | Move::Bash) {
                // One foot bears the bird while the other drives out; no
                // simultaneous hop masquerading as a planted kick.
                kick = strike;
                p.o(
                    J::Lift,
                    Vec3::new(0.055 * strike, -0.025 * gather, 0.06 * strike),
                );
                p.r(J::Body, bow(-0.14 * strike + 0.08 * gather));
                p.r(J::Neck, bow(0.14 * strike));
                p.r(J::Head, bow(-0.12 * strike));
            } else {
                // Neck retracts first, then the beak leads the breast through.
                p.r(J::Neck, bow(-0.34 * gather + 0.95 * strike));
                p.r(J::Head, bow(0.18 * gather - 0.2 * strike));
                p.r(J::Body, bow(-0.08 * gather + 0.12 * strike));
                p.o(
                    J::Lift,
                    Vec3::new(0.0, -0.025 * gather, -0.06 * gather + 0.16 * strike),
                );
            }
        } else {
            p.r(J::Neck, bow(-0.2 * action));
            p.r(J::Body, bow(-0.07 * action));
            if m == Move::Step {
                p.o(J::Lift, Vec3::new(0.0, 0.0, -0.16 * action));
            }
        }
    }
    if !local_legs {
        return;
    }
    for (thigh, shin, foot, side, offset) in [
        (J::LegL, J::ShinL, J::FootL, 1.0, 0.0),
        (J::LegR, J::ShinR, J::FootR, -1.0, 0.5),
    ] {
        let (z, lift) = beast_step(cycle / TAU + offset, 0.6, 0.6 / (0.9 * CADENCE), 0.1);
        let raised = if side < 0.0 { kick } else { 0.0 };
        beast_limb(
            p,
            thigh,
            shin,
            foot,
            Vec3::Y * BirdBody::BODY_HEIGHT,
            BirdBody::hip(side),
            BirdBody::UPPER,
            BIRD_ANKLE,
            Vec3::new(
                side * BirdBody::HIP.x,
                BirdBody::ANKLE_HEIGHT + 0.002 + lift * walk + 0.28 * raised,
                BirdBody::UPPER.z + BirdBody::LOWER.z + z * walk + 0.38 * raised,
            ),
            1.0,
            support.max(idle_support),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn quad_pose(
    p: &mut Pose,
    rig: &Rig,
    s: f32,
    w: f32,
    breath: f32,
    m: Option<Move>,
    t: f32,
    local_legs: bool,
) {
    let walk = ease(s * 4.0);
    let trot = ease((s - 0.38) / 0.45);
    let cadence = quadruped_cadence(rig.style);
    let cycle = w * cadence;
    let stance = 0.69 - 0.12 * trot;
    let span = stance / (0.9 * cadence * rig.stride.max(0.1));
    let action = m.map_or(0.0, |_| envelope(t, 0.0, 0.12, 0.86, 1.0));
    let support = walk.max(action);
    let roll = cycle.sin() * 0.022 * walk * (1.0 - 0.6 * trot);
    let pitch = (cycle * 2.0 - 0.4).sin() * 0.016 * walk + breath * 0.008;
    let rise = (cycle * 2.0).cos() * (0.008 + 0.01 * trot) * walk;
    // Idle geometry puts every sole on the floor. The lower travelling
    // pelvis leaves room for the knee to flex through a planted full stride.
    p.o(J::Lift, Vec3::Y * (-0.113 * rig.legs * support + rise));
    p.r(J::Body, bow(pitch) * Quat::from_rotation_z(roll));
    p.r(
        J::Neck,
        bow(-pitch * 0.75 + breath * 0.008) * Quat::from_rotation_z(-roll * 0.65),
    );
    p.r(J::Snout, bow(-breath * 0.008));
    // A loose tail follows the gait with a delay, rather than leading the body.
    p.r(
        J::Tail,
        Quat::from_rotation_y((cycle - 0.7).sin() * 0.12 * walk + breath * 0.035),
    );
    if rig.plan == Plan::Centaur {
        p.r(
            J::Torso,
            bow(-pitch * 0.85) * Quat::from_rotation_z(-roll * 0.85),
        );
        p.r(J::Head, bow(pitch * 0.2));
        p.r(
            J::ArmL,
            fwd(-cycle.sin() * 0.12 * walk) * Quat::from_rotation_z(0.08),
        );
        p.r(J::ArmR, fwd(0.35) * Quat::from_rotation_z(-0.08));
        p.r(J::ForeR, fwd(0.75));
    }
    let mut rear = 0.0;
    let mut stamp = 0.0;
    if rig.style.rogue_beast() {
        rogue::beast_pose(p, rig, breath, m, t, walk, cycle);
    } else if let Some(m) = m {
        let hit = m.impact();
        let gather = envelope(t, 0.0, hit * 0.55, hit - 0.1, hit);
        let strike = envelope(t, hit - 0.07, hit, hit + 0.06, 1.0);
        if !m.offensive() {
            p.r(J::Body, bow(-0.035 * action));
            p.r(J::Neck, bow(-0.14 * action));
            if m == Move::Step {
                p.o(J::Lift, Vec3::new(0.0, 0.0, -0.14 * action));
            }
            p.r(J::ArmL, fwd(0.65 * action));
        } else if rig.plan == Plan::Centaur {
            p.r(J::ArmR, fwd(-0.55 * gather + 0.9 * strike));
            p.r(J::ForeR, fwd(-0.5 * strike));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.27 * gather - 0.22 * strike) * bow(0.13 * strike),
            );
            p.r(J::ArmL, fwd(0.22 * gather - 0.12 * strike));
            // The modeled spear follows the grip's +Z axis. Keep the arm's
            // reach, but counter its rise so the point, not the shaft, leads.
            let aim = envelope(t, hit * 0.4, hit, hit + 0.11, 0.95);
            if aim > 0.0 {
                let grip = [J::Body, J::Torso, J::ArmR, J::ForeR]
                    .into_iter()
                    .fold(Quat::IDENTITY, |frame, joint| frame * p.rot[joint as usize]);
                // From the right-hand carry, meet the target slightly inward
                // and downward without driving the shaft through the chest.
                let toward = Vec3::new(0.10, -0.10, 1.0).normalize();
                let wrist = Quat::from_rotation_arc(Vec3::Z, grip.inverse() * toward);
                p.r(J::CentaurSpear, Quat::IDENTITY.slerp(wrist, aim));
            }
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.025 * gather, -0.055 * gather + 0.16 * strike),
            );
        } else if matches!(m, Move::Heavy | Move::Bash | Move::Shove) {
            // Hoof strike: the haunches load before the forequarters rise.
            // Each front limb follows a deliberate arc, never a sine-wave paw.
            rear = gather;
            stamp = strike;
            p.r(J::Body, bow(-0.38 * rear + 0.07 * stamp));
            p.o(
                J::Lift,
                Vec3::new(0.0, 0.1 * rear - 0.035 * stamp, -0.13 * rear + 0.1 * stamp),
            );
            p.r(J::Neck, bow(0.17 * rear - 0.06 * stamp));
        } else {
            // A unicorn aims its horn. Equines without a horn reach to bite;
            // neither needs to rear up for every ordinary attack.
            let horn = rig.style == Style::Unicorn;
            p.r(
                J::Neck,
                bow(-0.13 * gather + if horn { 0.54 } else { 0.29 } * strike),
            );
            p.r(J::Snout, bow(if horn { 0.19 } else { 0.06 } * strike));
            p.r(J::Body, bow(0.045 * strike));
            p.o(
                J::Lift,
                Vec3::new(0.0, -0.025 * gather, -0.075 * gather + 0.19 * strike),
            );
        }
    }
    if !local_legs {
        return;
    }
    for (thigh, shin, hoof, offset, front, side) in [
        (J::FL, J::FLs, J::HoofFL, 0.0, true, 1.0),
        (J::FR, J::FRs, J::HoofFR, 0.5, true, -1.0),
        // Touchdowns run LF, RH, RF, LH in the walk; the diagonal pairs
        // converge continuously as the gait becomes a collected trot.
        (J::BL, J::BLs, J::HoofBL, 0.25 + 0.25 * trot, false, 1.0),
        (J::BR, J::BRs, J::HoofBR, -0.25 + 0.25 * trot, false, -1.0),
    ] {
        if !rig.supports(hoof) {
            continue;
        }
        let clearance = if rig.style == Style::Unicorn {
            0.11
        } else {
            0.075 * rig.legs
        };
        let (z, lift) = beast_step(cycle / TAU + offset, stance, span, clearance);
        let hip = rig.rest[thigh as usize];
        let lead = if front {
            rear * 0.24 + stamp * 0.08
        } else {
            0.0
        };
        let raised = if front { rear * 0.32 } else { 0.0 };
        beast_limb(
            p,
            thigh,
            shin,
            hoof,
            rig.rest[J::Body as usize],
            hip,
            rig.rest[shin as usize],
            rig.rest[hoof as usize],
            Vec3::new(
                hip.x + rig.rest[shin as usize].x + rig.rest[hoof as usize].x + side * 0.012 * rear,
                // The swept horn wall sits below the shallow inner sole.
                rig.ankle_floor + lift * walk + raised * rig.legs,
                hip.z + rig.rest[shin as usize].z + rig.rest[hoof as usize].z + z * walk + lead,
            ),
            rig.support_bend(shin, hoof, if front { 1.0 } else { -1.0 }),
            support,
        );
    }
}

/// Carry the contact lunge and its crouched footing into the fall, using the
/// same release clock as the tendrils. A recently transferred step still owns
/// its first few frames, so don't reconstruct those feet from the action pose.
fn release_aquator_lunge(p: &mut Pose, rig: &Rig, a: &Anim) {
    let Some((entry, phase, fall)) = rogue::touch_fall_entry(a, Accent::Amber) else {
        return;
    };
    let motion = Move::Touch(Accent::Amber);
    if entry.speed > 0.02 && phase * motion.duration() < STRIKE_ENTRY_RELEASE_SECS {
        return;
    }
    let mut live = Pose::new();
    quad_pose(
        &mut live,
        rig,
        entry.speed,
        a.walk,
        (entry.started_at * 1.8 + a.seed).sin(),
        Some(motion),
        phase,
        true,
    );
    for (index, rot) in p.rot.iter_mut().enumerate() {
        *rot = live.rot[index].slerp(*rot, fall);
        p.off[index] = live.off[index].lerp(p.off[index], fall);
    }
}

fn death_pose(p: &mut Pose, rig: &Rig, d: f32, seed: f32) {
    let side = if seed > 5.0 { 1.0 } else { -1.0 };
    // A little variety from the seed, so no two bodies lie quite alike.
    let v = |k: u32| rand(u64::from(seed.to_bits()), k) - 0.5;
    match rig.plan {
        Plan::Biped => {
            // Crumpled onto the back: tipped over along the rig's x axis and
            // rolled face-up, flat to the floor.
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d)
                * Quat::from_rotation_y(side * FRAC_PI_2 * d)
                * Quat::from_rotation_x(0.08 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.86 * d, 0.1 * d, 0.0);
            // Arms flung loose to the floor, elbows soft.
            p.r(
                J::ArmL,
                Quat::from_rotation_z((0.85 + v(1)) * d) * fwd(-0.35 * d),
            );
            p.r(
                J::ArmR,
                Quat::from_rotation_z(-(0.55 + v(2)) * d) * fwd(-0.35 * d),
            );
            // Twisted so the elbows fold along the floor, not up off it.
            p.r(
                J::ForeL,
                Quat::from_rotation_y(-FRAC_PI_2 * d) * fwd((0.45 + v(3) * 0.5) * d),
            );
            p.r(
                J::ForeR,
                Quat::from_rotation_y(FRAC_PI_2 * d) * fwd((0.7 + v(4) * 0.5) * d),
            );
            p.r(J::HandL, fwd(-0.4 * d));
            p.r(J::HandR, fwd(-0.4 * d));
            // Legs splayed, one knee fallen outward.
            p.r(
                J::LegL,
                Quat::from_rotation_z(0.22 * d)
                    * Quat::from_rotation_y(0.5 * d)
                    * fwd((0.35 + v(5) * 0.3) * d),
            );
            p.r(J::ShinL, fwd(-0.55 * d));
            p.r(
                J::LegR,
                Quat::from_rotation_z(-0.12 * d) * Quat::from_rotation_y(-0.25 * d),
            );
            p.r(J::ShinR, fwd(-0.12 * d));
            p.r(
                J::Torso,
                Quat::from_rotation_y(0.15 * side * d) * bow(-0.08 * d),
            );
            // The head lolls to one side.
            p.r(
                J::Head,
                Quat::from_rotation_y(side * 1.0 * d) * bow(-0.3 * d),
            );
        }
        Plan::Centaur => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.72 * d, rig.girth * d, 0.0);
            limp_legs(p, side, d);
            // The horse is already on its side. Turn the human chest face-up
            // around its own long axis; an additional sideways torso roll
            // would drive its shoulders and head beneath the floor.
            p.r(J::Torso, Quat::from_rotation_y(side * FRAC_PI_2 * d));
            // The generated forearm's forward belly, smooth-union allowance,
            // and Storybook mesh scale bound its floor support. Derive the
            // shoulder-to-elbow drop from each rig's actual upper-arm length.
            let arm_support = (0.056 * 1.05 + 0.004 + 0.03 * 0.25) * 1.12 + 0.006;
            for (arm, fore, spread) in [(J::ArmL, J::ForeL, 0.55), (J::ArmR, J::ForeR, -0.55)] {
                // At the settled quarter-roll, body/torso x and arm z are
                // vertical. The body's small x-axis slump preserves them.
                let shoulder_height = rig.girth
                    + side * (rig.rest[J::Body as usize].x + rig.rest[J::Torso as usize].x)
                    + rig.rest[arm as usize].z;
                let length = rig.rest[fore as usize].length().max(0.001);
                let drop = ((shoulder_height - arm_support) / length)
                    .clamp(-0.9, 0.9)
                    .asin();
                let upper = Quat::from_rotation_z(spread) * bow(drop);
                // Replace the living weapon aim, including its elbow bend.
                // Counter-rotate the forearm so it rests along the floor.
                p.rot[arm as usize] = p.rot[arm as usize].slerp(upper, d);
                p.rot[fore as usize] = p.rot[fore as usize].slerp(bow(-drop), d);
            }
            // The spear's attachment turns its shaft across the grip. Relax
            // the wrist by the same quarter-turn to lay it beside the hand.
            p.rot[J::HandR as usize] = p.rot[J::HandR as usize].slerp(fwd(FRAC_PI_2), d);
            p.r(
                J::Head,
                Quat::from_rotation_y(side * 1.2 * d) * bow(0.4 * d),
            );
        }
        Plan::Quadruped => {
            // Fallen on its side: the body lies at its own girth above the
            // floor, close beside the spot where it stood.
            let height = rig.rest[J::Body as usize].y;
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * height * 0.715 * d, rig.girth * d, 0.0);
            limp_legs(p, side, d);
            rogue::death(p, rig, d, side);
            // Neck stretched out along the ground, head resting on it.
            p.r(
                J::Neck,
                bow(1.05 * d) * Quat::from_rotation_z(-side * 0.15 * d),
            );
            p.r(J::Snout, bow(-0.3 * d));
            rogue::settle_winged_beast_head(p, rig.style, d, side);
            settle_equine_head(p, rig, d, side);
            rogue::fallen_quadruped_tail(p, rig.style, d, side);
        }
        Plan::Bird if rig.style.flies() => rogue::death(p, rig, d, side),
        Plan::Rooted => rogue::death(p, rig, d, side),
        Plan::Bird => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * 1.5 * d);
            // Rest on the outer feather layer, not on the bare body shell.
            p.off[J::Lift as usize] = Vec3::new(side * 0.62 * d, 0.234 * d, 0.0);
            // Long legs sprawl; let the neck follow the torso toward the floor.
            p.r(
                J::LegL,
                fwd(0.6 * d) * Quat::from_rotation_z(-side * 0.35 * d),
            );
            p.r(J::ShinL, fwd(-0.2 * d));
            p.r(
                J::LegR,
                fwd(-0.2 * d) * Quat::from_rotation_z(-side * 0.1 * d),
            );
            p.r(J::ShinR, fwd(-0.5 * d));
            p.r(
                J::Neck,
                bow(1.25 * d) * Quat::from_rotation_z(side * 0.27 * d),
            );
            p.r(J::Head, bow(-0.4 * d));
        }
        Plan::Marker => {
            p.rot[J::Lift as usize] = Quat::from_rotation_z(side * FRAC_PI_2 * d);
            p.off[J::Lift as usize] = Vec3::new(side * 0.5 * d, 0.36 * d, 0.0);
        }
        Plan::Serpent | Plan::Floater => {}
    }
}

/// Let the neck give as the barrel comes to rest. The skull rotates
/// back independently so an equine settles on its cheek, not its ears
/// or the Unicorn's horn.
fn settle_equine_head(p: &mut Pose, rig: &Rig, fall: f32, side: f32) {
    if !matches!(rig.style, Style::Horse | Style::Quagga | Style::Unicorn) {
        return;
    }
    let settle = ease((fall - 0.25) / 0.75) * side;
    // A broader barrel keeps the neck joint higher on a sideways fall.
    let lean = 0.54 + 2.2 * (rig.girth - 0.32);
    let counter = if rig.style == Style::Unicorn {
        lean - 0.37
    } else {
        0.08
    };
    p.r(J::Neck, Quat::from_rotation_z(settle * lean));
    p.r(J::Snout, Quat::from_rotation_z(-settle * counter));
}

/// Four legs gone slack: the forelegs reach, the hind legs trail, and the
/// legs on top sag down toward the floor.
fn limp_legs(p: &mut Pose, side: f32, d: f32) {
    let sag = |left: bool| {
        if (side > 0.0) == left {
            -side * 0.35
        } else {
            -side * 0.08
        }
    };
    p.r(J::FL, Quat::from_rotation_z(sag(true) * d) * fwd(0.55 * d));
    p.r(J::FR, Quat::from_rotation_z(sag(false) * d) * fwd(0.3 * d));
    p.r(J::BL, Quat::from_rotation_z(sag(true) * d) * fwd(-0.35 * d));
    p.r(
        J::BR,
        Quat::from_rotation_z(sag(false) * d) * fwd(-0.55 * d),
    );
    for shin in [J::FLs, J::FRs, J::BLs, J::BRs] {
        p.r(shin, fwd(-0.18 * d));
    }
    p.r(J::Body, Quat::from_rotation_x(0.05 * d));
}

/// Ground the settled crystal core using its generated shard sections. The
/// circumscribed ring is a conservative bound for each six-sided section;
/// unlike a sphere bound it stays close to the visibly supporting shard.
fn settled_ice_height(rotation: Quat) -> f32 {
    let mut bottom = -0.24f32;
    for k in 0..9 {
        let a = k as f32 * TAU / 9.0;
        let direction = Vec3::new(a.cos(), (k as f32 * 1.3).sin() * 0.8, a.sin()).normalize();
        let axis = rotation * direction;
        let tip = 0.3 + rand(k as u64, 3) * 0.15;
        let width = 0.055 + 0.025 * rand(19, k);
        let radial_y = (1.0 - axis.y * axis.y).max(0.0).sqrt();
        for (along, radius) in [
            (0.06, width * 0.55),
            (0.06 + (tip - 0.06) * 0.68, width),
            (0.06 + (tip - 0.06) * 0.9, width * 0.3),
            (tip, 0.0),
        ] {
            bottom = bottom.min(axis.y * along - radius * radial_y);
        }
    }
    -bottom * 0.65 + 0.006
}

/// Snakes and floaters place their pieces along paths rather than joints.
fn place_segments(rig: &Rig, a: &Anim, fun: f32, joints: &mut Query<&mut Transform, Without<Rig>>) {
    let (m, t) = a
        .action
        .filter(|_| a.dead.is_none())
        .map_or((None, 0.0), |(m, t)| (Some(m), t));
    let d = ease(a.death_fall());
    let alive = 1.0 - d;
    let calm = (1.0 - ease(a.speed * 4.0))
        * f32::from(u8::from(m.is_none() && a.dead.is_none() && !a.quiet));
    let (w, gag, _) = antic(a);
    let spring = if gag == 0 { w * fun * calm } else { 0.0 };
    let attack_weights = |motion: Move, phase: f32| {
        let (gather, strike, _) = rogue::timing(motion, phase);
        (gather, if motion.offensive() { strike } else { 0.0 })
    };
    let (gather, strike) = m.map_or((0.0, 0.0), |motion| attack_weights(motion, t));
    if rogue::place(rig, a, fun, joints) {
        return;
    }
    match rig.plan {
        Plan::Serpent => {
            let n = rig.segments.len().min(SNAKE);
            if n == 0 {
                return;
            }
            // Release the last rendered bite into the fall. Clearing the
            // cancelled action immediately must not teleport the whole spine
            // out of its lunge, even during the live corridor-facing pre-roll.
            let (gather, strike) = match (a.dead, a.death_entry) {
                (Some(_), Some(entry)) => entry.action.map_or((0.0, 0.0), |(motion, phase)| {
                    let (g, s) = attack_weights(motion, phase);
                    let release = 1.0 - ease((a.time - entry.started_at) / 0.22);
                    (g * release, s * release)
                }),
                _ => (gather, strike),
            };
            // Keep the rattle on the existing drive-to-idle blend; starting
            // a death during a lunge must not pop it back to full height.
            let stopped = 1.0 - ease(a.speed * 4.0);
            let still = stopped * a.rest;
            let calm = still * f32::from(u8::from(a.dead.is_none() && !a.quiet));
            let rattler = rig.style == Style::Rattlesnake;
            let rear_weight = a.dead.map_or(calm, |phase| {
                if rattler || a.quiet {
                    0.0
                } else {
                    // The ordinary Snake may already be reared when it
                    // dies. Hand that lift off to the fall briefly rather
                    // than dropping the neck before the first authored beat.
                    let elapsed = phase.max(0.0) * a.death_motion.duration();
                    stopped * a.rest * (1.0 - ease(elapsed / 0.20))
                }
            });
            let rear = every(a.time + a.seed * 3.7, 8.0, 3.0, 6.5) * rear_weight;
            // Default rest keeps the whole animal extended and readable.
            // Head and rattle lift gently above the same continuous body path.
            let warning = if rattler { still } else { 0.0 };
            let idle_rise = 0.19 * rear + SNAKE_WARNING_LIFT * warning;
            let bite_rise = 0.24 * gather + 0.13 * strike;
            // Transfer support rather than stacking the idle rise onto
            // the established apex of the attack pose.
            let (brace, flourish) = a
                .dead
                .map_or((0.0, 0.0), |phase| a.death_motion.beats(phase));
            let raised = idle_rise.max(bite_rise) + 0.18 * brace + 0.30 * flourish;
            let path = SnakeLivePath {
                wave: a.walk * 2.2 + a.seed,
                speed: a.speed,
                neck_lift: raised,
                warning,
                calm,
                idle_phase: a.time * 0.9 + a.seed,
                gather,
                strike,
            };
            let mut settled = Vec3::new(0.0, snake_ground_height(0, 2.6, rig.style), 0.6);
            let mut points = [Vec3::ZERO; SNAKE];
            for (i, (point, live)) in points.iter_mut().zip(path.points(rig.style, n)).enumerate() {
                let f = i as f32;
                if i > 0 {
                    let fall_yaw = (f * 0.38 + a.seed * 0.2).sin() * 0.48;
                    let fall_dy = snake_ground_height(i, 2.6, rig.style) - settled.y;
                    settled += snake_link(fall_dy, fall_yaw);
                }
                let mut at = live.lerp(settled, d);
                if spring > 0.0 {
                    // The optional long-idle gag winds into a continuous helix,
                    // with approximately the same spacing as the resting body.
                    let turn = f * 0.48;
                    let bounce = (a.time * 7.0).sin().abs() * 0.1;
                    let coil = Vec3::new(
                        turn.cos() * 0.13,
                        0.46 - f * 0.02 + bounce * (1.0 - f / n as f32),
                        turn.sin() * 0.13,
                    );
                    at = at.lerp(coil, spring);
                }
                *point = at;
            }
            for (i, e) in rig.segments.iter().take(n).enumerate() {
                let before = points[i.saturating_sub(1)];
                let after = points[(i + 1).min(n - 1)];
                let tangent = before - after;
                // The raised neck supports a forward-looking skull rather
                // than pointing its eyes and fangs up at the ceiling.
                let direction = if i == 0 {
                    tangent.with_y(tangent.y * d).normalize_or(Vec3::Z)
                } else {
                    tangent.normalize_or(Vec3::Z)
                };
                // The rattle buzzes as a warning, at rest and when it strikes.
                let shake = if rattler && i + 3 >= n {
                    let buzz = ((calm * 0.6 + gather) * alive).min(1.0);
                    Quat::from_rotation_y((a.time * 42.0 + i as f32).sin() * 0.35 * buzz)
                } else {
                    Quat::IDENTITY
                };
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = points[i];
                    transform.rotation = Quat::from_rotation_arc(Vec3::Z, direction)
                        * Quat::from_rotation_z(d * 2.6)
                        * shake;
                }
            }
            let gape = (0.02 + 0.90 * gather) * (1.0 - strike) * alive;
            if let Some(jaw) = rig.joint(J::Jaw) {
                if let Ok(mut transform) = joints.get_mut(jaw) {
                    transform.rotation = bow(gape);
                }
            }
            if let Some(fangs) = rig.joint(J::Fangs) {
                if let Ok(mut transform) = joints.get_mut(fangs) {
                    // Hinged fangs fold forward along the palate as the mouth
                    // closes, rather than cutting through the lower jaw.
                    transform.rotation = snake_fang_rotation(gape);
                }
            }
            if let Some(tongue) = rig.joint(J::Snout) {
                if let Ok(mut transform) = joints.get_mut(tongue) {
                    let cycle = (a.time * 0.45 + a.seed).rem_euclid(1.0);
                    let flick = if cycle < 0.12 {
                        (cycle / 0.12 * TAU).sin().powi(2)
                    } else {
                        0.0
                    };
                    let out = flick.max(strike * 0.7) * alive;
                    transform.scale = Vec3::new(1.0, 1.0, out.max(0.01));
                }
            }
        }
        Plan::Floater => {
            let ice = rig.style == Style::Ice;
            let phase = a.time + a.seed * 2.7;
            let hover = if ice {
                (phase * 1.3).sin() * 0.025
            } else {
                (phase * 1.8).sin() * 0.055
            };
            const FREEZE: Move = Move::Touch(Accent::Travel);
            // The live action is cleared on death. Release the last reaching
            // posture and its aura while starting the original fall.
            let ice_entry = if ice {
                rogue::touch_fall_entry(a, Accent::Travel)
            } else {
                None
            };
            let (cue, keep) = ice_entry
                .map_or((m.map(|motion| (motion, t)), 1.0), |(_, phase, fall)| {
                    (Some((FREEZE, phase)), 1.0 - fall)
                });
            let freezing_touch = ice && matches!(cue, Some((FREEZE, _)));
            let (flare, gather, reach) = cue.map_or((0.0, 0.0, 0.0), |(motion, phase)| {
                let hit = motion.impact();
                let flare = envelope(phase, 0.0, hit * 0.8, hit + 0.04, 1.0) * keep;
                let gather = attack_weights(motion, phase).0 * keep;
                let reach = if freezing_touch {
                    envelope(phase, hit - 0.16, hit, hit + 0.06, 0.88) * keep
                } else {
                    0.0
                };
                (flare, gather, reach)
            });
            // The core braces instead of lunging with the reaching fan.
            let body_strike = if freezing_touch { 0.34 * reach } else { strike };
            let live_core = Vec3::new(
                (phase * 0.55).sin() * 0.015 * (1.0 - a.speed),
                FLOATER_HOVER + hover - 0.035 * gather,
                -0.04 * a.speed - 0.065 * gather + 0.25 * body_strike,
            );
            let fallen_rotation =
                Quat::from_rotation_y(a.seed * 0.4) * Quat::from_rotation_z(0.9) * bow(0.2);
            let fallen = Vec3::new(
                0.0,
                if ice {
                    settled_ice_height(fallen_rotation)
                } else {
                    0.055
                },
                0.0,
            );
            if let Some(core) = rig.joint(J::Core) {
                if let Ok(mut transform) = joints.get_mut(core) {
                    transform.translation = live_core.lerp(fallen, d);
                    // Ice keeps its mass and a readable face; fire breathes
                    // upward. Neither spins its eyes away from its target.
                    let pulse = if ice {
                        0.0
                    } else {
                        (phase * 3.6).sin() * 0.025
                    };
                    let charge = 1.0 + if ice { 0.08 } else { 0.16 } * flare;
                    let live_scale =
                        Vec3::new(1.0 - pulse * 0.5, 1.0 + pulse, 1.0 - pulse * 0.5) * charge;
                    transform.scale =
                        live_scale.lerp(Vec3::splat(if ice { 0.65 } else { 0.06 }), d);
                    let live_rotation = Quat::from_rotation_y((phase * 0.6).sin() * 0.08)
                        * bow(0.1 * a.speed - 0.14 * gather + 0.18 * body_strike)
                        * Quat::from_rotation_z(
                            (phase * 0.8).sin() * if ice { 0.035 } else { 0.06 },
                        );
                    transform.rotation = live_rotation.slerp(fallen_rotation, d);
                }
            }
            for (j, side) in [(J::ArmL, 1.0f32), (J::ArmR, -1.0)] {
                if let Some(e) = rig.joint(j) {
                    if let Ok(mut transform) = joints.get_mut(e) {
                        let lag = (phase * 1.3 - 0.55 + side * 0.25).sin() * 0.035;
                        let (offset, live_rotation) = if freezing_touch {
                            rogue::ice_touch_fan(side, lag, gather, reach)
                        } else {
                            (
                                Vec3::ZERO,
                                Quat::from_rotation_y(-side * 0.22 * strike)
                                    * Quat::from_rotation_z(side * (lag + 0.28 * flare)),
                            )
                        };
                        // The broken shard fans lie out to either side of
                        // the core, supported at their broad roots. Cancel
                        // the core's fall rotation so neither arm is buried.
                        let ground_rotation =
                            fallen_rotation.inverse() * Quat::from_rotation_z(side * FRAC_PI_2);
                        let ground_at = fallen_rotation.inverse()
                            * Vec3::new(side * 0.42, (0.025 - fallen.y) / 0.65, 0.06);
                        transform.translation = (rig.rest[j as usize] + offset).lerp(ground_at, d);
                        transform.rotation = live_rotation.slerp(ground_rotation, d);
                    }
                }
            }
            let count = rig.segments.len().max(1) as f32;
            for (i, e) in rig.segments.iter().enumerate() {
                let (at, size) = floater_mote(ice, live_core, phase, flare, i as f32, count);
                if let Ok(mut transform) = joints.get_mut(*e) {
                    transform.translation = at.lerp(fallen, d);
                    transform.scale = Vec3::splat(size * alive);
                }
            }
        }
        _ => {}
    }
}

/// How quickly soft flesh springs back, and how its motion dies away.
const SOFT_STIFFNESS: f32 = 420.0;
const SOFT_DAMPING: f32 = 10.0;
/// The furthest a soft region strays from rest, in model units.
const SOFT_REACH: f32 = 0.02;

/// Soft regions lag and settle as the body carrying them moves: each is a
/// damped spring driven by its frame's own acceleration, so every motion
/// that moves the body moves them too, without per-animation keys.
pub fn soften(
    time: Res<Time>,
    frames: Query<&GlobalTransform>,
    parents: Query<&ChildOf>,
    owners: Query<&Anim>,
    mut bones: Query<(&mut crate::props::SoftBone, &mut Transform, &ChildOf)>,
) {
    let dt = time.delta_secs();
    if dt <= 0.0 {
        return;
    }
    let dt = dt.min(0.05);
    for (mut bone, mut transform, parent) in &mut bones {
        let Ok(frame) = frames.get(parent.parent()) else {
            continue;
        };
        let world = frame.transform_point(bone.rest);
        let accel = match bone.history {
            Some((last, before)) => (world - 2.0 * last + before) / (dt * dt),
            None => Vec3::ZERO,
        };
        bone.history = Some((world, bone.history.map_or(world, |(last, _)| last)));
        // Teleports and spawns are not motion the flesh should answer.
        let accel = accel.clamp_length_max(40.0);
        let (scale, rotation, _) = frame.to_scale_rotation_translation();
        let local = (rotation.inverse() * accel) / scale.max(Vec3::splat(1e-3));
        let steps = 4;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let a = -SOFT_STIFFNESS * bone.offset - SOFT_DAMPING * bone.velocity - local;
            bone.velocity += a * h;
            let next = bone.offset + bone.velocity * h;
            bone.offset = next.clamp_length_max(SOFT_REACH);
        }
        // Soft frames sit below the rig's joints. Resolve their current owner
        // through the hierarchy, including after asynchronous model transfer.
        // Death stops only breathing; the passive spring above still settles.
        let mut ancestor = parent.parent();
        let breathing = loop {
            if let Ok(owner) = owners.get(ancestor) {
                break owner.dead.is_none();
            }
            let Ok(parent) = parents.get(ancestor) else {
                // Standalone soft models retain their independent breathing.
                break true;
            };
            ancestor = parent.parent();
        };
        // A slow breath lifts a chest a little forward and up, each body on
        // its own rhythm.
        let phase = parent.parent().index() as f32 * 1.37;
        let breath = if breathing {
            (time.elapsed_secs() * 1.7 + phase).sin() * bone.breath
        } else {
            0.0
        };
        transform.translation =
            bone.rest + bone.offset * bone.give + Vec3::new(0.0, 0.0022, 0.0014) * breath;
    }
}

#[cfg(test)]
mod support_tests {
    use super::*;
    use crate::creatures::rogue_tests::spawn_one;

    #[test]
    fn idle_ground_bird_pecks_without_dragging_its_toes() {
        let (_, rig, _, _) = spawn_one(&Look::of(CreatureModel::Emu, 49));
        let at = |time| {
            let mut anim = Anim::new(49, Vec3::ZERO);
            anim.seed = 0.0;
            anim.time = time;
            pose(&rig, &mut anim, 0.0, 0.02, true, false)
        };
        let foot_frame = |pose: &Pose, [thigh, shin, foot]: [J; 3]| {
            let mut origin = Vec3::ZERO;
            let mut rotation = Quat::IDENTITY;
            for joint in [J::Lift, J::Body, thigh, shin, foot] {
                let j = joint as usize;
                origin += rotation * (rig.rest[j] + pose.off[j]);
                rotation *= pose.rot[j];
            }
            // Near the forward toe pad: tracks both a sliding ankle and a
            // foot pitching through the ground with the body.
            (
                origin + rotation * Vec3::new(0.0, -0.032, 0.17),
                rotation * Vec3::Y,
            )
        };
        let rest = at(2.55);
        let peak = at(3.30);
        assert!(
            peak.rot[J::Neck as usize].angle_between(rest.rot[J::Neck as usize]) > 1.0,
            "keep the visible neck reach"
        );
        for (thigh, shin, foot) in BIPED_LEGS {
            let limbs = [thigh, shin, foot];
            let (initial, _) = foot_frame(&rest, limbs);
            for frame in (140..=190).step_by(2) {
                let time = frame as f32 / 50.0;
                let (toe, sole_up) = foot_frame(&at(time), limbs);
                assert!(
                    toe.distance(initial) < 0.020,
                    "{foot:?} toe moves {:.3} m while pecking at {time:.2}s",
                    toe.distance(initial),
                );
                assert!(
                    sole_up.dot(Vec3::Y) > 0.998,
                    "{foot:?} tips off the floor while pecking at {time:.2}s"
                );
            }
        }
    }

    #[test]
    fn yeti_idle_brings_each_fist_in_to_the_chest_in_turn() {
        let (_, rig, _, _) = spawn_one(&Look::of(CreatureModel::Yeti, 17));
        let at = |time, weight| {
            let mut p = Pose::new();
            idle(&mut p, &rig, time, 0.0, weight);
            p
        };
        let wrist = |p: &Pose, arm: J, fore: J, hand: J| {
            let [a, e, w] = [arm as usize, fore as usize, hand as usize];
            rig.rest[a] + p.rot[a] * (rig.rest[e] + p.rot[e] * rig.rest[w])
        };
        for (side, (arm, fore, hand, time)) in [
            (
                1.0,
                (J::ArmL, J::ForeL, J::HandL, (PI / 2.0 + 12.0 * TAU) / 12.0),
            ),
            (
                -1.0,
                (J::ArmR, J::ForeR, J::HandR, (-PI / 2.0 + 13.0 * TAU) / 12.0),
            ),
        ] {
            // The native fur chest sits around y=.28 and is .30 across the
            // midline to its outer edge. The wrist must be on its own side,
            // but come inward and back to the forward fur instead of the air.
            let point = wrist(&at(time, 1.0), arm, fore, hand);
            assert!(
                (0.12..0.32).contains(&(side * point.x))
                    && (0.18..0.50).contains(&point.y)
                    && (0.23..0.43).contains(&point.z),
                "chest beat misses on {hand:?}: {point:?}"
            );
            let other_beat = wrist(&at(time + PI / 12.0, 1.0), arm, fore, hand);
            assert!(
                other_beat.z > point.z + 0.055,
                "one fist must release while the other comes in"
            );
            let rest = wrist(&at(5.35, 1.0), arm, fore, hand);
            assert_eq!(rest, wrist(&Pose::new(), arm, fore, hand));
            assert_eq!(rest, wrist(&at(time, 0.0), arm, fore, hand));
        }
    }

    #[test]
    fn scout_idle_checks_its_grip_without_spinning_the_wrist() {
        let (_, mut rig, _, _) = spawn_one(&Look::of(CreatureModel::Scout, 104));
        let rest = Pose::new();
        let at = |time, weight| {
            let mut p = Pose::new();
            idle(&mut p, &rig, time, 0.0, weight);
            p
        };
        let right = J::HandR as usize;
        for time in [2.95, 3.0, 4.2, 4.25, 5.5] {
            assert!(at(time, 1.0).rot[right].angle_between(rest.rot[right]) < 0.001);
        }
        let mut furthest: f32 = 0.0;
        for frame in 150..=210 {
            let time = frame as f32 / 50.0;
            let wrist = at(time, 1.0).rot[right];
            let twist = wrist.angle_between(rest.rot[right]);
            assert!(twist < 1.4, "idle wrist turns {twist:.2} rad at {time:.2}s");
            furthest = furthest.max(twist);
        }
        assert!(furthest > 0.45, "the grip check should still read");
        let active = at(3.60, 1.0);
        assert!(
            active.rot[J::ForeR as usize].angle_between(rest.rot[J::ForeR as usize]) > 0.42,
            "the forearm should bring the grip up instead of just turning the hand"
        );
        assert!(
            at(3.6, 0.4).rot[right].angle_between(rest.rot[right])
                < active.rot[right].angle_between(rest.rot[right]) * 0.5,
            "walking/action blend must also fade the grip check"
        );
        // Compare against the same idle body at the same time: this hand may
        // still be resting on a hip because of the shared idle stance.
        rig.style = Style::Common;
        let mut common = Pose::new();
        idle(&mut common, &rig, 3.6, 0.0, 1.0);
        assert_eq!(active.rot[J::HandL as usize], common.rot[J::HandL as usize]);
    }

    #[test]
    fn strike_entry_admits_a_fresh_pose_and_releases_before_impact() {
        let (_, rig, _, _) = spawn_one(&Look::of(CreatureModel::Human, 22));
        let mut anim = Anim::new(22, Vec3::ZERO);
        anim.yaw = 0.0;
        anim.time = 1.0;
        anim.speed = 0.16;
        for (phase, planted_index) in [(0.3, 0), (0.8, 1)] {
            anim.contacts = WalkContacts::default();
            anim.action = None;
            anim.walk = phase * TAU / biped_gait(rig.style).cadence;
            for _ in 0..8 {
                let _ = pose(&rig, &mut anim, 0.0, 0.02, true, true);
            }
            let walk = pose(&rig, &mut anim, 0.0, 0.02, true, true);
            let seed = strike_entry_from_walk(&walk, &rig, &anim).expect("qualified walk");
            assert!(seed.feet[planted_index].planted);
            assert!(!seed.feet[1 - planted_index].planted);
            let legs = walk_legs(&rig, &anim);
            for (i, leg) in legs.iter().take(2).enumerate() {
                let leg = leg.unwrap();
                assert_eq!(
                    seed.feet[i].ankle,
                    posed_ankle(&walk, &rig, leg) * rig.scale
                );
            }
            anim.contacts.strike_entry = Some(seed);
            anim.act(Move::Strike);
            assert!(anim.contacts.rig.is_none());
            assert!(valid_strike_entry(&rig, &anim, 0.0));
            let transfer = anim.strike_entry.unwrap();
            anim.strike_entry = Some(StrikeEntryTransfer {
                seed: StrikeEntry {
                    rig: Entity::PLACEHOLDER,
                    ..seed
                },
                ..transfer
            });
            assert!(!valid_strike_entry(&rig, &anim, 0.0));
            anim.strike_entry = Some(transfer);
            anim.last.y += 0.1;
            assert!(!valid_strike_entry(&rig, &anim, 0.0));
            anim.last.y -= 0.1;
            anim.time = seed.time + 0.10;
            assert!(valid_strike_entry(
                &rig,
                &anim,
                0.10 / Move::Strike.duration()
            ));
            anim.time = seed.time + 0.25;
            assert!(!valid_strike_entry(&rig, &anim, 0.0));
            anim.time = seed.time;
            anim.action = Some((Move::Strike, 0.17 / Move::Strike.duration()));
            let _ = pose(&rig, &mut anim, 0.0, 0.0, false, false);
            assert!(anim.strike_entry.is_none());

            anim.action = None;
            anim.contacts.strike_entry = Some(seed);
            anim.time = seed.time + 0.10;
            anim.act(Move::Strike);
            assert!(
                anim.strike_entry.is_none(),
                "freshness is tested at the cue"
            );
        }
        anim.contacts.strike_entry = None;
        anim.action = None;
        anim.act(Move::Strike);
        assert!(
            anim.strike_entry.is_none(),
            "stationary action needs no carry"
        );
    }

    #[test]
    fn walking_emu_brings_solved_feet_into_its_first_peck() {
        let (_, rig, _, _) = spawn_one(&Look::of(CreatureModel::Emu, 42));
        let strike = Move::Strike;
        for phase in [0.20, 0.70] {
            let mut anim = Anim::new(42, Vec3::ZERO);
            anim.yaw = 0.0;
            anim.time = 1.0;
            anim.speed = 0.32;
            anim.walk = phase * TAU / 1.6;
            for _ in 0..8 {
                let _ = pose(&rig, &mut anim, 0.0, 0.02, true, true);
            }
            let walk = pose(&rig, &mut anim, 0.0, 0.02, true, true);
            let [Some(left), Some(right), ..] = walk_legs(&rig, &anim) else {
                panic!("ground bird must have two walk contacts");
            };
            let feet = [left, right];
            let prior = feet.map(|leg| posed_ankle(&walk, &rig, leg));
            anim.contacts.strike_entry = strike_entry_from_walk(&walk, &rig, &anim);
            anim.act(strike);
            anim.time += 0.016;
            anim.action = Some((strike, 0.016 / strike.duration()));
            let entry = pose(&rig, &mut anim, 0.0, 0.016, false, false);
            for (i, leg) in feet.into_iter().enumerate() {
                let ankle = posed_ankle(&entry, &rig, leg);
                let shift = ankle.distance(prior[i]);
                assert!(
                    shift < 0.035,
                    "walk phase {phase} foot {i} jumped {shift:.3} m into the peck",
                );
            }

            anim.time += 0.19;
            anim.action = Some((strike, 0.206 / strike.duration()));
            let released = pose(&rig, &mut anim, 0.0, 0.0, false, false);
            assert!(anim.strike_entry.is_none(), "release before beak contact");
            let plain = pose(&rig, &mut anim, 0.0, 0.0, false, false);
            for leg in feet {
                assert_eq!(
                    posed_ankle(&released, &rig, leg),
                    posed_ankle(&plain, &rig, leg)
                );
            }
        }
    }

    #[test]
    fn moving_strike_entry_can_release_over_multiple_capped_steps() {
        let max_step = 0.05;
        assert!(max_step / Move::Strike.duration() > ACTION_SUPPORT_IN);
        assert!(STRIKE_ENTRY_RELEASE_SECS < Move::Strike.impact() * Move::Strike.duration());
        assert!(strike_entry_release(max_step, true) < strike_entry_release(max_step, false));
        for planted in [true, false] {
            assert_eq!(strike_entry_release(0.0, planted), 0.0);
            assert_eq!(
                strike_entry_release(STRIKE_ENTRY_RELEASE_SECS, planted),
                1.0
            );
            for step in [0.02, max_step] {
                let mut previous = 0.0;
                for frame in 1..=8 {
                    let next = strike_entry_release(step * frame as f32, planted);
                    assert!(next >= previous && next - previous < 0.65);
                    if frame == 1 {
                        assert!(0.0 < next && next < 0.5);
                    }
                    previous = next;
                }
                assert_eq!(previous, 1.0);
            }
        }
    }

    #[test]
    fn hero_strike_stages_recovery_without_changing_impact() {
        let impact = Move::Strike.impact();
        assert_eq!(Move::Strike.duration(), 0.62);
        for step in 0..=1000 {
            let t = step as f32 / 1000.0;
            let (advance, lead_lift) = action_step(t, impact, true);
            let rear_lift = hero_strike_rear_phase(t).map_or(0.0, hero_strike_rear_lift);
            if t <= 0.715 {
                assert_eq!((advance, lead_lift), action_step(t, impact, false));
            }
            if rear_lift > 0.0 {
                assert_eq!((advance, lead_lift), (0.0, 0.0));
            }
        }
        let look = Look::with_choice(
            CreatureModel::Human,
            22,
            Some(Variant {
                woman: true,
                build: 0,
                figure: 2,
                ..Variant::default()
            }),
        );
        let (_, rig, _, _) = spawn_one(&look);
        let mut anim = Anim::new(22, Vec3::ZERO);
        anim.rest = 0.0;
        let legs = walk_legs(&rig, &anim);
        let legs = [legs[0].unwrap(), legs[1].unwrap()];
        let rest = biped_rest_contacts(&rig);
        let ankle = |anim: &mut Anim, t| {
            anim.action = Some((Move::Strike, t));
            let posed = pose(&rig, anim, 0.0, 0.0, false, false);
            legs.map(|leg| posed_ankle(&posed, &rig, leg))
        };
        let landed = ankle(&mut anim, 0.905);
        assert!(landed[0].distance(rest[0]) < 0.001, "{landed:?}");
        assert!(landed[1].distance(rest[1]) < 0.001, "{landed:?}");
        let pickup = ankle(&mut anim, 0.945);
        assert!((pickup[0].y - rest[0].y).abs() < 0.004, "{pickup:?}");
        assert!(pickup[1].y > rest[1].y + 0.008, "{pickup:?}");
        anim.action = Some((Move::Strike, 0.96));
        let posed = pose(&rig, &mut anim, 0.0, 0.0, false, false);
        let (rear_leg, rear_shin, rear_foot) = BIPED_LEGS[1];
        let sole_up = [J::Lift, J::Hips, rear_leg, rear_shin, rear_foot]
            .into_iter()
            .fold(Quat::IDENTITY, |chain, j| chain * posed.rot[j as usize])
            * Vec3::Y;
        assert!(sole_up.distance(Vec3::Y) < 1e-5, "{sole_up:?}");
        let end = ankle(&mut anim, 1.0);
        anim.action = None;
        let idle = pose(&rig, &mut anim, 0.0, 0.0, false, false);
        assert_eq!(end, legs.map(|leg| posed_ankle(&idle, &rig, leg)));
    }

    #[test]
    fn death_preroll_consumes_intended_heading_but_falling_bodies_keep_theirs() {
        for phase in [None, Some(-0.35), Some(0.0), Some(1.0)] {
            let (mut world, rig, _, meshes) = spawn_one(&Look::of(CreatureModel::Human, 7));
            let root = world
                .get::<ChildOf>(rig.joint(J::Lift).unwrap())
                .unwrap()
                .parent();
            let mut anim = Anim::new(7, Vec3::ZERO);
            anim.yaw = 0.0;
            anim.dead = phase;
            // Scene death supplies a corridor-aware heading during the negative
            // pre-roll; a nonnegative death must discard stale walking targets.
            anim.target_yaw = Some(FRAC_PI_2);
            world.entity_mut(root).insert((rig, anim));
            world.insert_resource(meshes);
            let mut time = Time::<()>::default();
            time.advance_by(std::time::Duration::from_secs_f32(0.05));
            world.insert_resource(time);
            let mut animation = Schedule::default();
            animation.add_systems(animate);
            animation.run(&mut world);
            let anim = world.get::<Anim>(root).unwrap();
            if phase.is_none_or(|d| d < 0.0) {
                let expected = FRAC_PI_2 * (1.0 - (-0.05f32 * 12.0).exp());
                assert!(
                    (anim.yaw - expected).abs() < 1e-6,
                    "pre-roll/living heading was discarded: {phase:?}, yaw={}",
                    anim.yaw
                );
                assert_eq!(anim.target_yaw, Some(FRAC_PI_2));
            } else {
                assert_eq!(anim.yaw, 0.0);
                assert_eq!(anim.target_yaw, None);
            }
            if phase == Some(-0.35) {
                for _ in 0..8 {
                    animation.run(&mut world);
                }
                let anim = world.get::<Anim>(root).unwrap();
                assert!(anim.dead.unwrap() > 0.0);
                assert_eq!(anim.target_yaw, None);
                assert!(
                    (anim.yaw - FRAC_PI_2).abs() < 0.02,
                    "body must face the selected corridor before the fall"
                );
                let heading = anim.yaw;
                for _ in 0..25 {
                    animation.run(&mut world);
                }
                assert_eq!(
                    world.get::<Anim>(root).unwrap().yaw,
                    heading,
                    "falling body must not keep turning"
                );
            }
        }
    }

    #[test]
    fn living_to_dead_transition_stops_idle_layers_with_retained_rest() {
        for (model, wardrobe) in [
            (CreatureModel::Human, Wardrobe::Commoner),
            (CreatureModel::Human, Wardrobe::Knight),
            (CreatureModel::Human, Wardrobe::SylvanDuelist),
            (CreatureModel::Mage, Wardrobe::Native),
        ] {
            for woman in [false, true] {
                let look = Look::with_choice(model, 7, Some(Variant { woman, ..default() }))
                    .dressed(wardrobe);
                let (_, rig, _, _) = spawn_one(&look);
                for death in [0.0, 0.5, 1.0] {
                    for gag in 0..3 {
                        // Explicitly select an active antic and establish that it
                        // actually moves this living fixture before testing death.
                        let mut lively = Anim::new(7, Vec3::ZERO);
                        lively.time = 4.0;
                        lively.antic = Some(gag);
                        let dancing = pose(&rig, &mut lively, 1.0, 0.0, false, false);
                        let resting = pose(&rig, &mut lively, 0.0, 0.0, false, false);
                        assert!(
                            dancing.rot != resting.rot || dancing.off != resting.off,
                            "inactive antic fixture: {model:?}/{wardrobe:?}, gag={gag}"
                        );
                        let mut ordinary = Anim::new(7, Vec3::ZERO);
                        ordinary.dead = Some(death);
                        let mut quiet = Anim::new(7, Vec3::ZERO);
                        quiet.dead = Some(death);
                        ordinary.antic = Some(gag);
                        quiet.antic = Some(gag);
                        quiet.quiet = true;
                        let mut first = None;
                        // Explicit refresh can publish the first pose without a
                        // time step. Continue through an active antic interval.
                        for frame in 0..4 {
                            let time = 4.0 + frame as f32 / 60.0;
                            let dt = if frame == 0 { 0.0 } else { 1.0 / 60.0 };
                            ordinary.time = time;
                            quiet.time = time;
                            let actual = pose(&rig, &mut ordinary, 1.0, dt, false, false);
                            let expected = pose(&rig, &mut quiet, 1.0, dt, false, false);
                            assert_eq!(actual.rot, expected.rot,
                            "living fidget leaked into transitioning {model:?}/{wardrobe:?}, woman={woman}, time={time}");
                            assert_eq!(actual.off, expected.off);
                            let snapshot = (actual.rot, actual.off);
                            if let Some(first) = first {
                                assert_eq!(snapshot, first, "fixed death phase must not idle");
                            } else {
                                first = Some(snapshot);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn discovered_corpses_start_without_living_fidgets_across_humanoid_outfits() {
        for (model, wardrobe) in [
            (CreatureModel::Human, Wardrobe::Commoner),
            (CreatureModel::Human, Wardrobe::Knight),
            (CreatureModel::Human, Wardrobe::SylvanDuelist),
            (CreatureModel::Mage, Wardrobe::Native),
        ] {
            for woman in [false, true] {
                let look = Look::with_choice(model, 7, Some(Variant { woman, ..default() }))
                    .dressed(wardrobe);
                let (_, rig, _, _) = spawn_one(&look);
                for gag in 0..3 {
                    // Explicitly select an active antic and establish that it
                    // actually moves this living fixture before testing death.
                    let mut lively = Anim::new(7, Vec3::ZERO);
                    lively.time = 4.0;
                    lively.antic = Some(gag);
                    let dancing = pose(&rig, &mut lively, 1.0, 0.0, false, false);
                    let resting = pose(&rig, &mut lively, 0.0, 0.0, false, false);
                    assert!(
                        dancing.rot != resting.rot || dancing.off != resting.off,
                        "inactive antic fixture: {model:?}/{wardrobe:?}, gag={gag}"
                    );
                    let mut ordinary = Anim::corpse(7);
                    let mut quiet = Anim::corpse(7);
                    ordinary.antic = Some(gag);
                    quiet.antic = Some(gag);
                    quiet.quiet = true;
                    let mut first = None;
                    // Explicit refresh can publish the first pose without a
                    // time step. Continue through an active antic interval.
                    for frame in 0..4 {
                        let time = 4.0 + frame as f32 / 60.0;
                        let dt = if frame == 0 { 0.0 } else { 1.0 / 60.0 };
                        ordinary.time = time;
                        quiet.time = time;
                        let actual = pose(&rig, &mut ordinary, 1.0, dt, false, false);
                        let expected = pose(&rig, &mut quiet, 1.0, dt, false, false);
                        assert_eq!(actual.rot, expected.rot,
                            "living fidget leaked into discovered {model:?}/{wardrobe:?}, woman={woman}, time={time}");
                        assert_eq!(actual.off, expected.off);
                        let snapshot = (actual.rot, actual.off);
                        if let Some(first) = first {
                            assert_eq!(snapshot, first, "settled skeleton must not animate");
                        } else {
                            first = Some(snapshot);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn discovered_corpse_revival_recovers_living_idle_blend() {
        let (mut world, rig, _, meshes) = spawn_one(&Look::of(CreatureModel::Human, 7));
        let root = world
            .get::<ChildOf>(rig.joint(J::Lift).unwrap())
            .unwrap()
            .parent();
        let entities: Vec<_> = rig.joints.iter().flatten().copied().collect();
        assert!(!entities.is_empty());
        world.entity_mut(root).insert((rig, Anim::corpse(7)));
        world.insert_resource(meshes);
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_secs_f32(1.0 / 60.0));
        world.insert_resource(time);
        let mut animation = Schedule::default();
        animation.add_systems(animate);
        animation.run(&mut world);
        let corpse = world.get::<Anim>(root).unwrap();
        assert_eq!(corpse.fallen, 1.0, "ordinary update retains settled fall");
        assert_eq!(
            corpse.rest, 0.0,
            "discovered corpse starts without living idle"
        );
        world.get_mut::<Anim>(root).unwrap().dead = None;
        let mut previous: Vec<_> = entities
            .iter()
            .map(|&entity| *world.get::<Transform>(entity).unwrap())
            .collect();
        let mut prior_rest = 0.0;
        for _ in 0..60 {
            animation.run(&mut world);
            let anim = world.get::<Anim>(root).unwrap();
            assert!(
                anim.rest > prior_rest,
                "revival gradually restores idle blend"
            );
            prior_rest = anim.rest;
            for (index, &entity) in entities.iter().enumerate() {
                let current = *world.get::<Transform>(entity).unwrap();
                assert!(current.rotation.is_finite() && current.translation.is_finite());
                assert!(
                    current.rotation.angle_between(previous[index].rotation) < 0.2,
                    "revival must not snap a skeletal joint"
                );
                assert!(
                    current.translation.distance(previous[index].translation) < 0.1,
                    "revival must not jump a skeletal joint"
                );
                previous[index] = current;
            }
        }
        let revived = world.get::<Anim>(root).unwrap();
        assert_eq!(revived.fallen, 0.0);
        assert!(revived.rest > 0.99);
        assert_eq!(
            Anim::new(7, Vec3::ZERO).rest,
            1.0,
            "ordinary live construction still starts fully at rest"
        );
    }

    #[test]
    fn explicit_pose_refresh_preserves_time_and_held_frames() {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        )
        .dressed(Wardrobe::Leotard);
        let (mut world, rig, _, meshes) = spawn_one(&look);
        let root = world
            .get::<ChildOf>(rig.joint(J::Lift).unwrap())
            .unwrap()
            .parent();
        let hand = rig.joint(J::HandR).unwrap();
        let original_hand = *world.get::<Transform>(hand).unwrap();
        let mut anim = Anim::new(7, Vec3::ZERO);
        anim.time = 3.0;
        anim.walk = 0.7;
        anim.speed = 0.2;
        anim.hurt = Some(0.1);
        anim.action = Some((Move::Jump, 0.4));
        world.entity_mut(root).insert((rig, anim, PoseRefresh));
        world.insert_resource(meshes);
        let mut refresh = Schedule::default();
        refresh.add_systems(refresh_pose);
        refresh.run(&mut world);
        assert!(
            world.get::<PoseRefresh>(root).is_none(),
            "requests are consumed once"
        );
        let anim = world.get::<Anim>(root).unwrap();
        assert_eq!(
            (anim.time, anim.walk, anim.speed, anim.hurt),
            (3.0, 0.7, 0.2, Some(0.1))
        );
        assert!(matches!(anim.action, Some((Move::Jump, 0.4))));
        assert_ne!(
            *world.get::<Transform>(hand).unwrap(),
            original_hand,
            "refresh must actually pose geometry"
        );
        let snapshot = |world: &mut World| {
            let transforms: Vec<_> = world
                .query::<(Entity, &Transform)>()
                .iter(world)
                .map(|(e, t)| (e, *t))
                .collect();
            let geometry: Vec<_> = world
                .resource::<Assets<Mesh>>()
                .iter()
                .map(|(id, mesh)| {
                    let positions = match mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap() {
                        bevy::render::mesh::VertexAttributeValues::Float32x3(p) => p.clone(),
                        _ => panic!("expected positions"),
                    };
                    (id, positions)
                })
                .collect();
            (transforms, geometry)
        };
        let held = snapshot(&mut world);
        for _ in 0..5 {
            refresh.run(&mut world);
        }
        assert_eq!(
            snapshot(&mut world),
            held,
            "unrequested refresh cannot alter joints or cloth/hair meshes"
        );
    }

    #[test]
    fn repeated_explicit_pose_refresh_preserves_joints_without_dirtying_them() {
        // Costume names are cosmetic recipes, never mechanical classes.
        for (model, wardrobe) in [
            (CreatureModel::Human, Wardrobe::Leotard),
            (CreatureModel::Human, Wardrobe::Commoner),
            (CreatureModel::Human, Wardrobe::Knight),
            (CreatureModel::Human, Wardrobe::RoyalPrincess),
            (CreatureModel::Human, Wardrobe::SylvanDuelist),
            (CreatureModel::Mage, Wardrobe::Native),
            (CreatureModel::Emu, Wardrobe::Native),
            (CreatureModel::Quagga, Wardrobe::Native),
            (CreatureModel::Centaur, Wardrobe::Native),
        ] {
            let look = Look::with_choice(
                model,
                7,
                Some(Variant {
                    woman: true,
                    ..default()
                }),
            )
            .dressed(wardrobe);
            let (mut world, rig, _, meshes) = spawn_one(&look);
            let root = world
                .get::<ChildOf>(rig.joint(J::Lift).unwrap())
                .unwrap()
                .parent();
            let entities: Vec<_> = rig.joints.iter().flatten().copied().collect();
            assert!(
                !entities.is_empty(),
                "real articulated rig, not an empty query"
            );
            let mut anim = Anim::new(7, Vec3::ZERO);
            anim.time = 3.0;
            anim.action = Some((Move::Jump, 0.4));
            world.entity_mut(root).insert((rig, anim, PoseRefresh));
            world.insert_resource(meshes);
            let mut refresh = Schedule::default();
            refresh.add_systems(refresh_pose);
            refresh.run(&mut world);
            let before: Vec<_> = entities
                .iter()
                .map(|&e| *world.get::<Transform>(e).unwrap())
                .collect();
            world.increment_change_tick();
            world.clear_trackers();
            world.entity_mut(root).insert(PoseRefresh);
            refresh.run(&mut world);
            let after: Vec<_> = entities
                .iter()
                .map(|&e| *world.get::<Transform>(e).unwrap())
                .collect();
            assert_eq!(
                before, after,
                "same timestamp must preserve every joint value"
            );
            let changed = entities
                .iter()
                .filter(|&&e| world.entity(e).get_ref::<Transform>().unwrap().is_changed())
                .count();
            assert_eq!(
                changed,
                0,
                "identical explicit pose dirtied {changed}/{} joints",
                entities.len()
            );
            eprintln!(
                "{model:?}/{wardrobe:?}: {} joint publications, {changed} changed",
                entities.len()
            );
            assert_eq!(world.get::<Anim>(root).unwrap().time, 3.0);
            world.get_mut::<Anim>(root).unwrap().action = Some((Move::Jump, 0.6));
            world.entity_mut(root).insert(PoseRefresh);
            refresh.run(&mut world);
            if model == CreatureModel::Human {
                assert!(
                    entities
                        .iter()
                        .zip(before)
                        .any(|(&e, t)| *world.get::<Transform>(e).unwrap() != t),
                    "a different requested phase must still publish its changed pose"
                );
            }
        }
    }

    #[test]
    fn jump_crouch_has_bounded_speed_at_phase_boundaries() {
        for boundary in [0.0_f32, JUMP_TAKEOFF, JUMP_LANDING, 1.0] {
            let start = (boundary - 0.0001).max(0.0);
            let end = (boundary + 0.0001).min(1.0);
            let speed = (jump_profile(end).0 - jump_profile(start).0).abs()
                / ((end - start) * Move::Jump.duration());
            assert!(speed < 12.0, "crouch speed={speed} at phase {boundary}");
        }
        assert!(jump_profile(0.15).0 > 0.9, "retain takeoff crouch");
        assert!(jump_profile(0.85).0 > 0.8, "retain landing absorption");
        assert_eq!(jump_profile(0.5).0, 0.0);
    }

    #[test]
    fn jump_arms_move_continuously_through_takeoff_and_landing() {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: false,
                ..default()
            }),
        );
        let (_, rig, _, _) = spawn_one(&look);
        let arms = |t| {
            let mut p = Pose::new();
            biped_pose(
                &mut p,
                &rig,
                0.0,
                0.0,
                0.0,
                Some((Move::Jump, t)),
                WalkingCarriage::Native,
            );
            [J::ArmL, J::ArmR, J::ForeL, J::ForeR].map(|j| p.rot[j as usize])
        };
        // Limit angular travel per real second at supported review rates,
        // including samples straddling the flight boundaries at finer steps.
        for hz in [20.0, 50.0, 60.0, 120.0, 240.0] {
            let dt = 1.0 / hz / Move::Jump.duration();
            let mut t = 0.0;
            while t < 1.0 {
                let end = (t + dt).min(1.0);
                for (a, b) in arms(t).into_iter().zip(arms(end)) {
                    let speed = a.angle_between(b) / ((end - t) * Move::Jump.duration());
                    assert!(speed < 14.0, "{hz}Hz t={t}: arm speed={speed}");
                }
                t = end;
            }
        }
        for boundary in [JUMP_TAKEOFF, JUMP_LANDING] {
            for (a, b) in arms(boundary - 0.0001)
                .into_iter()
                .zip(arms(boundary + 0.0001))
            {
                assert!(
                    a.angle_between(b) < 0.004,
                    "arm snaps at flight boundary {boundary}"
                );
            }
        }
        // Smoothing must preserve the recognizable overhead flight pose.
        assert!(arms(0.5)[0].angle_between(arms(0.0)[0]) > 2.0);
    }

    #[test]
    fn medusa_gaze_keeps_its_knees_under_the_skirt_with_both_feet_down() {
        for build in 0..=2 {
            let (_, rig, _, _) = spawn_one(&Look::with_choice(
                CreatureModel::Medusa,
                42,
                Some(Variant {
                    woman: true,
                    build,
                    ..default()
                }),
            ));
            let a = Anim::new(42, Vec3::ZERO);
            let legs = walk_legs(&rig, &a);
            let sample = |action: Option<(Move, f32)>| {
                let mut p = Pose::new();
                biped_pose(&mut p, &rig, 0.0, 0.0, 0.0, action, WalkingCarriage::Native);
                if let Some((m, t)) = action {
                    grounded_action(&mut p, &rig, &a, m, t);
                }
                p
            };
            let knee_z = |p: &Pose, leg: WalkLeg| {
                let chain =
                    p.rot[J::Lift as usize] * p.rot[J::Hips as usize] * p.rot[leg.thigh as usize];
                let at = walk_hip(p, &rig, leg)
                    + chain * (rig.rest[leg.shin as usize] + p.off[leg.shin as usize]);
                // Cloth hangs from the hips, so compare in this worn frame.
                let hips = p.off[J::Lift as usize]
                    + p.rot[J::Lift as usize]
                        * (rig.rest[J::Hips as usize] + p.off[J::Hips as usize]);
                let frame = p.rot[J::Lift as usize] * p.rot[J::Hips as usize];
                (frame.inverse() * (at - hips)).z
            };
            let rest = sample(None);
            for phase in [0.28, Move::Gaze.impact(), 0.68] {
                let gaze = sample(Some((Move::Gaze, phase)));
                assert!(
                    gaze.rot[J::ArmL as usize].angle_between(rest.rot[J::ArmL as usize]) > 0.20
                );
                for leg in legs.into_iter().flatten() {
                    let push = knee_z(&gaze, leg) - knee_z(&rest, leg);
                    assert!(
                        push < 0.052,
                        "build {build}, phase {phase:.2}, {:?} advanced {push:.3} into the skirt",
                        leg.foot
                    );
                    let foot_slide =
                        posed_ankle(&gaze, &rig, leg).distance(posed_ankle(&rest, &rig, leg));
                    assert!(
                        foot_slide < 0.024,
                        "build {build}, {:?} moved {foot_slide:.3} during the planted gaze",
                        leg.foot
                    );
                }
            }
        }
    }

    #[test]
    fn robed_focus_keeps_the_knees_near_rest_with_both_feet_planted() {
        let at = |p: &Pose, rig: &Rig, leg: J, shin: J, foot: J| {
            let l = J::Lift as usize;
            let h = J::Hips as usize;
            let [leg, shin, foot] = [leg, shin, foot].map(|j| j as usize);
            let hips = p.off[l] + p.rot[l] * (rig.rest[h] + p.off[h]);
            let torso = p.rot[l] * p.rot[h];
            let knee = hips
                + torso
                    * (rig.rest[leg] + p.off[leg] + p.rot[leg] * (rig.rest[shin] + p.off[shin]));
            let ankle = knee + torso * p.rot[leg] * p.rot[shin] * (rig.rest[foot] + p.off[foot]);
            (knee, ankle)
        };
        let anim = Anim::new(22, Vec3::ZERO);
        for model in [CreatureModel::Mage, CreatureModel::RedMage] {
            for woman in [false, true] {
                let look = Look::with_choice(
                    model,
                    22,
                    Some(Variant {
                        woman,
                        build: 2,
                        ..default()
                    }),
                );
                let (_, rig, _, _) = spawn_one(&look);
                assert!(rig.robe.is_some());
                let mut idle = Pose::new();
                biped_pose(
                    &mut idle,
                    &rig,
                    0.0,
                    0.0,
                    0.0,
                    None,
                    WalkingCarriage::Native,
                );
                for phase in [0.27, 0.5, 0.78] {
                    let mut p = Pose::new();
                    biped_pose(
                        &mut p,
                        &rig,
                        0.0,
                        0.0,
                        0.0,
                        Some((Move::Focus, phase)),
                        WalkingCarriage::Native,
                    );
                    grounded_action(&mut p, &rig, &anim, Move::Focus, phase);
                    for ((leg, shin, foot), target) in
                        BIPED_LEGS.into_iter().zip(biped_rest_contacts(&rig))
                    {
                        let (knee, ankle) = at(&p, &rig, leg, shin, foot);
                        let (rest_knee, _) = at(&idle, &rig, leg, shin, foot);
                        assert!(
                            ankle.distance(target) < 0.014,
                            "{model:?} foot drift {ankle:?} vs {target:?}"
                        );
                        assert!(
                            knee.z - rest_knee.z < 0.14,
                            "{model:?} focus drives knee {:.3}m forward",
                            knee.z - rest_knee.z
                        );
                    }
                    assert!(
                        p.rot[J::ArmR as usize].angle_between(idle.rot[J::ArmR as usize]) > 1.0
                    );
                }
                let (_, unrobed, _, _) = spawn_one(&look.dressed(Wardrobe::Knight));
                assert!(unrobed.robe.is_none());
                let mut p = Pose::new();
                biped_pose(
                    &mut p,
                    &unrobed,
                    0.0,
                    0.0,
                    0.0,
                    Some((Move::Focus, 0.5)),
                    WalkingCarriage::Native,
                );
                assert_eq!(p.off[J::Lift as usize].y, -0.08);
            }
        }
    }

    #[test]
    fn mage_replacement_support_has_no_invisible_floor_length_robe() {
        for model in [CreatureModel::Mage, CreatureModel::RedMage] {
            for woman in [false, true] {
                let look = Look::with_choice(model, 17, Some(Variant { woman, ..default() }))
                    .dressed(Wardrobe::Knight);
                let (_, mut rig, _, _) = spawn_one(&look);
                assert!(rig.robe.is_none());
                let contacts = biped_rest_contacts(&rig);
                let mut caster = Pose::new();
                caster.rot[J::Hips as usize] = fwd(0.65);
                caster.off[J::Lift as usize] = Vec3::new(0.0, -0.2, 0.3);
                let mut ordinary = Pose::new();
                ordinary.rot = caster.rot;
                ordinary.off = caster.off;
                fit_biped_support(&mut caster, &rig, &contacts, 1.0);
                rig.style = Style::Common;
                fit_biped_support(&mut ordinary, &rig, &contacts, 1.0);
                assert_eq!(
                    caster.off, ordinary.off,
                    "support follows worn cloth, not caster identity"
                );
            }
        }
    }

    #[test]
    fn women_of_every_kind_flirt_and_men_do_not() {
        // The flirt's kiss: her left forearm bent to her lips.
        let kissing = |model: CreatureModel, woman: bool| {
            let look = Look::with_choice(model, 7, Some(Variant { woman, ..default() }));
            let (mut app, root) =
                crate::creatures::rogue_tests::animate_look(look, 1, |_| Vec3::ZERO);
            let world = app.world_mut();
            let mut anim = world.get_mut::<Anim>(root).unwrap();
            anim.antic = Some(2);
            // A second of frames, ending at the height of the kiss, lets
            // her idle and the antic ease fully in.
            anim.time = 1.0 + 0.5 * 7.0 - 60.0 * 0.016;
            world.insert_resource(Boredom(BORED_AFTER + 10.0));
            for _ in 0..60 {
                app.update();
            }
            let rig = app.world().get::<Rig>(root).unwrap();
            let fore = rig.joint(J::ForeL).unwrap();
            let bend = app.world().get::<Transform>(fore).unwrap().rotation;
            // The kiss pose, with the arm's resting elbow bend laid over it.
            bend.angle_between(fwd(2.2) * fwd(0.25)) < 0.08
        };
        for model in [
            CreatureModel::Human,
            CreatureModel::Orc,
            CreatureModel::Nymph,
        ] {
            assert!(kissing(model, true), "{model:?}: she blows a kiss");
            assert!(!kissing(model, false), "{model:?}: he does not");
        }
    }

    #[test]
    fn troll_scratch_brings_free_claws_to_the_upper_scalp() {
        use crate::creatures::rogue_tests::points_under;
        for (woman, build) in [(false, 0), (false, 2), (true, 1), (true, 2)] {
            let look = Look::with_choice(
                CreatureModel::Troll,
                42,
                Some(Variant {
                    woman,
                    build,
                    ..default()
                }),
            );
            let (world, rig, _, meshes) = spawn_one(&look);
            let hand = rig.joint(J::HandL).unwrap();
            let tips: Vec<_> = points_under(&world, &meshes, hand)
                .into_iter()
                .filter(|v| v.y < -0.085)
                .collect();
            let scalp: Vec<_> = points_under(&world, &meshes, rig.joint(J::Head).unwrap())
                .into_iter()
                .filter(|v| v.x > 0.05 && v.y > 0.24 && v.z.abs() < 0.14)
                .collect();
            assert!(!tips.is_empty() && !scalp.is_empty());
            let frame = |time: f32, quiet: bool| {
                let mut anim = Anim::new(42, Vec3::ZERO);
                anim.seed = 0.0;
                anim.time = time;
                anim.quiet = quiet;
                pose(&rig, &mut anim, 0.0, 0.016, false, false)
            };
            let gap = |p: &Pose| {
                let local = |j: J| {
                    Affine3A::from_rotation_translation(
                        p.rot[j as usize],
                        rig.rest[j as usize] + p.off[j as usize],
                    )
                };
                let grip = local(J::ArmL) * local(J::ForeL) * local(J::HandL);
                let scale = world.get::<Transform>(hand).unwrap().scale;
                let to_head = local(J::Head).inverse() * grip;
                tips.iter()
                    .map(|tip| to_head.transform_point3(*tip * scale))
                    .flat_map(|tip| scalp.iter().map(move |at| tip.distance(*at)))
                    .fold(f32::INFINITY, f32::min)
            };
            assert!(gap(&frame(5.0, false)) > 0.20, "claws start at the side");
            assert!(gap(&frame(7.5, false)) > 0.20, "claws return to the side");
            let peak = frame(6.25, false);
            let distance = gap(&peak);
            assert!(
                distance < 0.055,
                "Troll woman={woman} build={build}: scalp gap {distance:.3}"
            );
            let quiet = frame(6.25, true);
            for j in [J::ArmR, J::ForeR, J::HandR] {
                assert!(
                    peak.rot[j as usize].angle_between(quiet.rot[j as usize]) < 0.01,
                    "the equipment hand stays in its resting pose"
                );
            }
        }
    }

    #[test]
    fn troll_scratch_yields_to_a_covered_head_and_an_attack() {
        let base = Look::of(CreatureModel::Troll, 42);
        let mut helmet = Equipment::default();
        helmet.select("helm").unwrap();
        for (look, action) in [
            (base.clone().equipped(helmet), None),
            (base, Some((Move::Strike, 0.26))),
        ] {
            let (_, rig, _, _) = spawn_one(&look);
            let mut anim = Anim::new(42, Vec3::ZERO);
            anim.seed = 0.0;
            anim.time = 6.25;
            anim.action = action;
            let mut quiet = Anim::new(42, Vec3::ZERO);
            quiet.seed = anim.seed;
            quiet.time = anim.time;
            quiet.action = anim.action;
            quiet.quiet = true;
            let active = pose(&rig, &mut anim, 0.0, 0.016, false, false);
            let reference = pose(&rig, &mut quiet, 0.0, 0.016, false, false);
            for j in [J::ArmL, J::ForeL, J::HandL] {
                assert!(active.rot[j as usize].angle_between(reference.rot[j as usize]) < 0.01);
            }
        }
    }

    #[test]
    fn now_and_then_she_tosses_her_head() {
        let look = Look::with_choice(
            CreatureModel::Human,
            7,
            Some(Variant {
                woman: true,
                ..default()
            }),
        );
        let (_, rig, _, _) = spawn_one(&look);
        let seed = 0.25;
        // The toss peaks at the middle of its window in each 11-second beat.
        let at_beat = |beat: f32| (beat - (seed * 0.37f32).fract()).rem_euclid(1.0) * 11.0;
        let head = |time: f32| {
            let mut pose = Pose::new();
            idle(&mut pose, &rig, time, seed, 1.0);
            pose.rot[J::Head as usize]
        };
        let tossed = head(at_beat(0.035));
        // Just after the toss, before her other motions have moved on.
        let calm = head(at_beat(0.08));
        assert!(
            tossed.angle_between(calm) > 0.25,
            "the toss turns her head well aside: {}",
            tossed.angle_between(calm)
        );
    }

    #[test]
    fn her_walk_places_her_feet_nearer_the_line_of_travel() {
        let narrowing = |woman: bool| {
            let look = Look::with_choice(
                CreatureModel::Human,
                7,
                Some(Variant { woman, ..default() }),
            );
            let (_, rig, _, _) = spawn_one(&look);
            let mut walking = Anim::new(7, Vec3::ZERO);
            walking.speed = 1.0;
            let rest = biped_rest_contacts(&rig);
            let planned = biped_walk_contacts(&rig, &walking);
            planned[0].x.abs() / rest[0].x.abs()
        };
        assert!(narrowing(true) < 0.8, "her feet track nearer the line");
        assert!(
            (narrowing(false) - 1.0).abs() < 1e-4,
            "his stay under his hips"
        );
    }

    #[test]
    fn xeroc_ambush_reaches_with_open_fangs_before_clamping() {
        let (_, rig, _, _) = spawn_one(&Look::of(CreatureModel::Xeroc, 42));
        let animation = Anim::new(42, Vec3::ZERO);
        let ambush = Move::for_ability("rogue.ambush_from_guise", None);
        let hit = ambush.impact();
        let sample = |motion, phase| {
            let mut p = Pose::new();
            quad_pose(&mut p, &rig, 0.0, 0.0, 0.0, motion, phase, true);
            p
        };
        let rest = sample(None, 0.0);
        let open = sample(Some(ambush), hit - 0.08);
        let contact = sample(Some(ambush), hit);
        let closed = sample(Some(ambush), hit + 0.17);
        let jaw = |p: &Pose| p.rot[J::Jaw as usize].angle_between(rest.rot[J::Jaw as usize]);
        assert!(
            jaw(&open) > 0.8,
            "the windup must expose both rows of fangs"
        );
        assert!(
            jaw(&contact) > 0.4,
            "the ambush must reach the target before clamping, got {}",
            jaw(&contact)
        );
        assert!(
            jaw(&closed) < 0.16,
            "the jaws should clamp in the follow-through"
        );
        assert!(
            contact.off[J::Neck as usize].z > 0.08,
            "reach should come from an extending head, not just the whole root"
        );
        for leg in walk_legs(&rig, &animation).into_iter().flatten() {
            let slip = (posed_ankle(&contact, &rig, leg) - posed_ankle(&rest, &rig, leg)).length();
            assert!(slip < 0.05, "{:?} ambush pad slipped {slip}", leg.foot);
        }
        let normal_bite = sample(Some(Move::Strike), hit);
        assert_eq!(normal_bite.off[J::Neck as usize], Vec3::ZERO);
        for phase in [0.0, 1.0] {
            let end = sample(Some(ambush), phase);
            assert_eq!(end.off, rest.off, "cue boundary {phase} must not pop");
            assert!(jaw(&end) < 1e-5);
        }
    }

    #[test]
    fn amphibian_local_support_preserves_authored_lateral_foot_landmarks() {
        let mut failures = Vec::new();
        for model in [CreatureModel::Aquator, CreatureModel::Xeroc] {
            let (_, rig, _, _) = spawn_one(&Look::of(model, 42));
            let animation = Anim::new(42, Vec3::ZERO);
            // A stationary guarded action invokes the local solver at full
            // support weight; no walking-memory target can mask this path.
            let mut pose = Pose::new();
            quad_pose(&mut pose, &rig, 0.0, 0.0, 0.0, Some(Move::Step), 0.4, true);
            let mut checked = 0;
            for leg in walk_legs(&rig, &animation).into_iter().flatten() {
                let actual = posed_ankle(&pose, &rig, leg);
                if (actual.x - leg.rest.x).abs() > 0.002 {
                    failures.push(format!(
                        "{model:?} {:?}: posed x={:.5}, authored x={:.5}",
                        leg.foot, actual.x, leg.rest.x
                    ));
                }
                checked += 1;
            }
            assert_eq!(checked, 4, "{model:?} needs four supporting limbs");
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}

#[cfg(test)]
mod cloak_pacing_tests {
    use super::CloakPacing;

    #[test]
    fn an_interval_of_zero_fits_every_wearer_every_frame() {
        let mut pacing = CloakPacing::every(0);
        for _ in 0..5 {
            assert!((0..4).all(|ordinal| pacing.fits(ordinal)));
            pacing.finish_frame(4);
        }
    }

    #[test]
    fn new_or_growing_crowds_do_not_repeat_a_previous_slot() {
        for previous in [0, 1, 3] {
            let mut pacing = CloakPacing::every(1);
            if previous != 0 {
                pacing.finish_frame(previous);
            }
            let selected: Vec<_> = (0..8).filter(|&ordinal| pacing.fits(ordinal)).collect();
            assert_eq!(selected.len(), 1, "previous {previous}: {selected:?}");
        }
    }

    #[test]
    fn a_crowd_returning_after_an_empty_frame_keeps_the_budget() {
        let mut pacing = CloakPacing::every(1);
        pacing.finish_frame(5);
        pacing.finish_frame(0);
        let selected: Vec<_> = (0..5).filter(|&ordinal| pacing.fits(ordinal)).collect();
        assert_eq!(selected, vec![0]);
    }

    #[test]
    fn crowd_changes_keep_interval_slots_and_resume_fair_turns() {
        for interval in [2, 3] {
            let mut pacing = CloakPacing::every(interval);
            for wearers in [0, 8, 1, 8, 0, 5, 2, 7] {
                let count = (0..wearers).filter(|&ordinal| pacing.fits(ordinal)).count();
                assert!(count <= 1);
                if !pacing.frame.is_multiple_of(interval) {
                    assert_eq!(count, 0);
                }
                pacing.finish_frame(wearers);
            }
            let mut fitted = [0; 7];
            for _ in 0..interval * 7 {
                for ordinal in 0..7 {
                    if pacing.fits(ordinal) {
                        fitted[ordinal as usize] += 1;
                    }
                }
                pacing.finish_frame(7);
            }
            assert_eq!(fitted, [1; 7]);
        }
    }

    #[test]
    fn paced_wearers_take_turns_one_per_slot() {
        for (interval, wearers) in [(1, 1), (1, 3), (2, 1), (2, 5), (3, 4)] {
            let mut pacing = CloakPacing::every(interval);
            pacing.finish_frame(wearers);
            let mut fitted = vec![0; wearers as usize];
            let frames = interval * wearers * 3;
            for _ in 0..frames {
                let now: Vec<u32> = (0..wearers)
                    .filter(|&ordinal| pacing.fits(ordinal))
                    .collect();
                assert!(now.len() <= 1, "at most one fit in a frame");
                for ordinal in now {
                    fitted[ordinal as usize] += 1;
                }
                pacing.finish_frame(wearers);
            }
            assert!(
                fitted.iter().all(|&count| count == 3),
                "interval {interval}, {wearers} wearers: {fitted:?}"
            );
        }
    }
}

#[cfg(test)]
mod profile_tests {
    use super::*;

    #[test]
    fn held_frame_clears_profile_and_profile_is_optional() {
        let mut app = App::new();
        app.insert_resource(Time::<()>::default())
            .init_resource::<Assets<Mesh>>()
            .insert_resource(MotionProfile {
                rigs: 9,
                phases: [std::time::Duration::from_secs(1); 4],
                hair_phases: [std::time::Duration::from_secs(1); 4],
            })
            .add_systems(Update, animate);
        app.update();
        assert_eq!(
            *app.world().resource::<MotionProfile>(),
            MotionProfile::default()
        );
        app.world_mut().remove_resource::<MotionProfile>();
        app.update();
    }
}
