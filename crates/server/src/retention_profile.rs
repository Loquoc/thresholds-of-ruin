//! Exact ownership witnesses plus bounded, opt-in-output retention diagnostics.
//! Run one case per process for meaningful Linux RSS samples. Timings are
//! observations only; neither elapsed time nor RSS is a correctness threshold.
use super::*;
use serde_json::json;

fn rss_bytes() -> Option<u64> {
    std::fs::read_to_string("/proc/self/status")
        .ok()?
        .lines()
        .find_map(|line| {
            line.strip_prefix("VmRSS:")?
                .split_whitespace()
                .next()?
                .parse::<u64>()
                .ok()
        })
        .map(|kib| kib * 1024)
}

fn sample(engine: &Engine, held: &[PresentationBoundary], phase: &str) -> serde_json::Value {
    // Pointer identities are diagnostic only and never influence simulation.
    // Deduplicate actual allocations, not handles or equal serialized values.
    let mut identities = std::collections::BTreeSet::new();
    let mut roots = Vec::new();
    for root in engine
        .boundaries
        .iter()
        .chain(held.iter().map(|capability| &capability.boundary))
    {
        if identities.insert(Arc::as_ptr(root)) {
            roots.push(root.as_ref());
        }
    }
    let rss = rss_bytes(); // Before checkpoint diagnostics allocate temporary state.
    #[derive(Default)]
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut encoded = Counter::default();
    // Presentation roots can outnumber a legal checkpoint's root list. Encode
    // bounded independent pools rather than weakening the production limit.
    // Sharing across batches is intentionally not counted by this size proxy.
    let batch_limit = tor_simulation::checkpoint::MAX_CHECKPOINT_ROOTS;
    for batch in roots.chunks(batch_limit) {
        let mut shared = tor_simulation::checkpoint::SharedState::default();
        let snapshots: Vec<_> = batch
            .iter()
            .map(|boundary| boundary.game.checkpoint(&mut shared))
            .collect();
        serde_json::to_writer(&mut encoded, &(snapshots, shared)).unwrap();
    }
    json!({
        "phase": phase,
        "records": engine.profile_counts().0,
        "rewind_roots": engine.profile_counts().1,
        "presentation_handles": held.len(),
        "unique_boundary_roots": roots.len(),
        "boundary_inline_bytes": roots.len() * std::mem::size_of::<Boundary>(),
        "snapshot_batch_json_bytes": encoded.0,
        "snapshot_batches": roots.len().div_ceil(batch_limit),
        "snapshot_batch_root_limit": batch_limit,
        "rss_bytes": rss,
    })
}

pub(super) fn run(history: usize, attachments: usize, path: &Path) -> serde_json::Value {
    assert!([0, 100, 1000, 10000].contains(&history));
    assert!([1, 8].contains(&attachments));
    let rss_before_engine = rss_bytes();
    let scenario = Scenario::performance(42, 8, 1).unwrap();
    let mut engine = Engine::memory(scenario.clone()).unwrap();
    engine.seed_profile_history(history).unwrap();
    assert_eq!(engine.profile_counts().0, history);
    let initial = sample(&engine, &[], "seeded");
    assert_eq!(
        initial["unique_boundary_roots"],
        (history + 1).min(REWIND_BOUNDARIES)
    );
    let mut held = Vec::new();
    // Worst-case staggered clients: each holds 32 distinct old boundaries.
    // This models only capability ownership, not Service or socket overhead.
    // The Service witness separately verifies its actual per-attachment bound.
    for _ in 0..attachments {
        for _ in 0..32 {
            engine.seed_profile_history(1).unwrap();
            let capability = engine.presentation_boundary();
            assert!(Arc::ptr_eq(
                &capability.boundary,
                engine.boundaries.back().unwrap()
            ));
            held.push(capability);
        }
    }
    let weak: Vec<_> = held
        .iter()
        .map(|capability| Arc::downgrade(&capability.boundary))
        .collect();
    let old = engine.state_at(&held[0], ActorId(1), 7).unwrap();
    // Expiring the rewind window does not revoke an issued presentation.
    engine.seed_profile_history(REWIND_BOUNDARIES).unwrap();
    assert_eq!(engine.boundaries.len(), REWIND_BOUNDARIES);
    assert_eq!(engine.state_at(&held[0], ActorId(1), 7).unwrap(), old);
    assert!(weak.iter().all(|root| root.strong_count() == 1));
    let retained = sample(&engine, &held, "rewind_expired");
    assert_eq!(
        retained["unique_boundary_roots"],
        REWIND_BOUNDARIES + 32 * attachments
    );
    assert_eq!(
        retained["snapshot_batches"],
        if attachments == 8 { 2 } else { 1 }
    );
    assert!(retained["snapshot_batch_json_bytes"].as_u64().unwrap() > 0);
    drop(held);
    assert!(
        weak.iter().all(|root| root.upgrade().is_none()),
        "the final presentation owner must release every expired root"
    );
    let released = sample(&engine, &[], "capabilities_released");
    assert_eq!(released["unique_boundary_roots"], REWIND_BOUNDARIES);

    // Normal checkpoint restart validates all retained records. It must not
    // replay the complete archive merely because the rewind window expired.
    engine = engine
        .attach_profile_save_with_policy(
            path,
            crate::SavePolicy {
                checkpoint_interval: 1,
                ..crate::SavePolicy::default()
            },
        )
        .unwrap();
    engine
        .command(
            "bench",
            "headless",
            ActorId(1),
            "capture",
            &engine.branch().clone(),
            Command::Act {
                expected_boundary: engine.causal_boundary(ActorId(1)).unwrap(),
                action: Action::Wait,
            },
        )
        .unwrap();
    engine.flush().unwrap();
    let expected = engine.state(ActorId(1)).unwrap();
    let records = history + 32 * attachments + REWIND_BOUNDARIES + 1;
    assert_eq!(engine.profile_counts().0, records);
    let checkpoint_json_bytes = engine.profile_checkpoint_json_encoding().unwrap().0;
    drop(engine);
    let start = Instant::now();
    let restored = Engine::open(path, scenario).unwrap();
    let restart_ms = start.elapsed().as_secs_f64() * 1000.;
    assert_eq!(restored.state(ActorId(1)).unwrap(), expected);
    assert_eq!(restored.recovery_profile().records_loaded, records);
    assert_eq!(restored.recovery_profile().records_replayed, 0);
    json!({
        "version": 2,
        "history": history,
        "attachments": attachments,
        "rss_before_engine": rss_before_engine,
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "stages": [initial, retained, released],
        "checkpoint_json_bytes": checkpoint_json_bytes,
        "saved_bytes": std::fs::metadata(path).unwrap().len(),
        "restart_ms": restart_ms,
        "recovery": restored.recovery_profile(),
    })
}
