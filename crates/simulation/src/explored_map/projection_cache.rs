//! One bounded, derived projection entry shared only by clones of a Game.
use super::*;
use std::sync::{Arc, Mutex};
use tor_world::SharedIdentity;

const KEY_LIMIT: usize = MAX_STATES;
#[derive(Debug)]
struct Entry {
    actor: ActorId,
    root: Option<SharedIdentity<ExploredMap>>,
    window: MapWindow,
    domain: MapWindow,
    cells: Vec<(Position, Location, u8)>,
    projection: Arc<MapProjection>,
}
impl Entry {
    fn matches(
        &self,
        actor: ActorId,
        root: Option<&Shared<ExploredMap>>,
        scene: &[SightCell],
        window: MapWindow,
        domain: MapWindow,
        halo: MapWindow,
    ) -> bool {
        self.actor == actor
            && self.window == window
            && self.domain == domain
            && match (&self.root, root) {
                (None, None) => true,
                (Some(a), Some(b)) => a.matches(b),
                _ => false,
            }
            && self.cells.iter().copied().eq(scene
                .iter()
                .filter(|c| c.physical && halo.contains(c.offset))
                .map(|c| (c.offset, c.location, c.rotation)))
    }
}
#[derive(Clone, Debug, Default)]
pub(crate) struct ProjectionCache(Arc<Mutex<Option<Entry>>>);
impl PartialEq for ProjectionCache {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for ProjectionCache {}
impl ProjectionCache {
    pub(super) fn project(
        &self,
        actor: ActorId,
        root: Option<&Shared<ExploredMap>>,
        scene: &[SightCell],
        window: MapWindow,
        domain: MapWindow,
    ) -> Result<MapProjection, GameError> {
        let halo = window
            .projection_halo(domain)
            .ok_or(GameError::InvalidLocation)?;
        let hit = {
            let entry = self.0.lock().expect("map projection cache");
            entry
                .as_ref()
                .filter(|e| e.matches(actor, root, scene, window, domain, halo))
                .map(|e| Arc::clone(&e.projection))
        };
        if let Some(hit) = hit {
            let mut result = (*hit).clone();
            // Work records what this request actually traversed, not historical
            // work used to produce the cached cells and completeness flags.
            result.work = MapWorkCounts::default();
            return Ok(result);
        }
        let result = match root {
            Some(map) => map.project_region(scene, window, domain),
            None => ExploredMap::default().project_region(scene, window, domain),
        }?;
        let mut cells = Vec::new();
        for c in scene
            .iter()
            .filter(|c| c.physical && halo.contains(c.offset))
        {
            if cells.len() == KEY_LIMIT {
                return Ok(result);
            }
            cells.push((c.offset, c.location, c.rotation));
        }
        // The cache retains neither the map root nor wire-salted presentation.
        // Its output is bounded by ordinary projection limits; oversized input
        // keys use the unchanged projector without admission to the cache.
        let entry = Entry {
            actor,
            root: root.map(Shared::weak_identity),
            window,
            domain,
            cells,
            projection: Arc::new(result.clone()),
        };
        *self.0.lock().expect("map projection cache") = Some(entry);
        Ok(result)
    }
}
