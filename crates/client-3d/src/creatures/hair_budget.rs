//! Fitting loose hair against the body is the costliest animation work, and
//! a crowd cannot have it every frame on the main thread. In play, solves
//! run on background threads instead: between them each wearer's hair stays
//! on its head in its last fitted shape, and a finished solve is written in
//! a later frame. Free workers go to the wearers that have waited longest,
//! sooner the nearer they are to the camera.
use bevy::prelude::Resource;

/// Where hair contact solves run.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HairSolving {
    /// Every frame, on the main thread, before the frame is drawn: exact
    /// and deterministic, for tests, benchmarks and still reviews.
    #[default]
    Inline,
    /// On background threads, written when finished: for play.
    Background,
}

/// Distance at which waiting counts half as much as right beside the camera.
pub(crate) const NEAR: f32 = 4.0;
/// The most motion one hair solve answers at once, in seconds.
pub(crate) const MAX_STEP: f32 = 0.25;

/// One wearer whose hair awaits a solve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Waiting {
    /// Seconds since its last solve began.
    pub wait: f32,
    /// Distance from the camera.
    pub distance: f32,
}

impl Waiting {
    fn priority(&self) -> f32 {
        let far = self.distance / NEAR;
        self.wait / (1.0 + far * far)
    }
}

/// The waiting wearers to solve now, most pressing first, at most `free`.
pub(crate) fn choose(waiting: &[Waiting], free: usize) -> Vec<usize> {
    // Background solves can occupy all workers for several frames. Do not
    // allocate and sort a crowd-sized order that cannot dispatch any work.
    if free == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..waiting.len())
        .filter(|&i| waiting[i].wait > 0.0)
        .collect();
    // Select only the dispatchable prefix before sorting it. An explicit input
    // ordinal tie-break preserves the old stable priority order across partitioning.
    let compare = |&a: &usize, &b: &usize| {
        waiting[b]
            .priority()
            .total_cmp(&waiting[a].priority())
            .then(a.cmp(&b))
    };
    if free < order.len() {
        order.select_nth_unstable_by(free, compare);
        order.truncate(free);
    }
    order.sort_unstable_by(compare);
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    fn waiting(wait: f32, distance: f32) -> Waiting {
        Waiting { wait, distance }
    }

    #[test]
    fn free_workers_go_to_the_longest_waits() {
        let jobs = [waiting(0.1, 5.0), waiting(0.4, 5.0), waiting(0.2, 5.0)];
        assert_eq!(choose(&jobs, 2), vec![1, 2]);
    }

    #[test]
    fn nearer_wearers_are_solved_sooner_for_the_same_wait() {
        let jobs = [waiting(0.1, 12.0), waiting(0.1, 2.0)];
        assert_eq!(choose(&jobs, 1), vec![1]);
    }

    #[test]
    fn busy_workers_do_not_allocate_a_discarded_crowd_order() {
        // Background solves commonly occupy every worker across several frames.
        // Positive waits keep this distinct from the empty/no-motion path.
        let crowd = vec![waiting(0.3, 1.0); 4096];
        assert_eq!(choose(&crowd, 1), vec![0]);
        let order = choose(&crowd, 0);
        assert!(order.is_empty());
        assert_eq!(
            order.capacity(),
            0,
            "busy workers allocated discarded order"
        );
    }

    #[test]
    fn available_workers_preserve_ties_and_skip_in_flight_wearers() {
        let jobs = [
            waiting(0.2, 0.0),
            waiting(0.0, 0.0),
            waiting(0.4, NEAR),
            waiting(-0.1, 0.0),
            waiting(0.2, 0.0),
        ];
        assert_eq!(choose(&jobs, 2), vec![0, 2]);
        assert_eq!(choose(&jobs, usize::MAX), vec![0, 2, 4]);
    }

    #[test]
    fn partial_selection_matches_stable_full_sort_at_budget_boundaries() {
        let mut jobs = vec![
            waiting(0.2, 0.0),
            waiting(0.4, NEAR), // Equal priority through a different distance.
            waiting(0.0, 0.0),
            waiting(-0.0, 0.0),
            waiting(f32::NAN, 0.0),
            waiting(1.0, f32::INFINITY),
            waiting(1.0, f32::NAN),
            waiting(f32::INFINITY, 1.0),
        ];
        jobs.extend((0..120).map(|i| waiting((i % 7) as f32 * 0.1, (i % 3) as f32)));
        for count in [0, 1, 8, 128] {
            let jobs = &jobs[..count];
            let mut reference: Vec<_> = (0..count).filter(|&i| jobs[i].wait > 0.0).collect();
            reference.sort_by(|&a, &b| jobs[b].priority().total_cmp(&jobs[a].priority()));
            for free in [0, 1, 4, count.saturating_sub(1), count, usize::MAX] {
                assert_eq!(
                    choose(jobs, free),
                    reference[..free.min(reference.len())],
                    "count={count}, free={free}"
                );
            }
        }
    }

    #[test]
    fn no_free_worker_or_no_motion_solves_nothing() {
        assert!(choose(&[waiting(0.3, 1.0)], 0).is_empty());
        assert!(choose(&[waiting(0.0, 1.0)], 4).is_empty());
        assert!(choose(&[], 4).is_empty());
    }
}
