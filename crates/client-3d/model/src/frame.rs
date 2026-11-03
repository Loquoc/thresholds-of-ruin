//! Recover presentation translation from uniquely disclosed cell identities.
use std::collections::BTreeMap;
use tor_protocol::{BranchId, CellView, Position, StreamContext};

/// Alignment of the newly observed picture with the current presentation scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameChange {
    /// Translation to add to observer-frame coordinates before drawing them.
    /// This is accumulated across aligned views and reset to zero on discontinuity.
    pub offset: Position,
    /// No safe translation from the previous picture was established.
    /// Consumers must not interpolate actors or effects across this boundary.
    pub discontinuous: bool,
}

/// Infer scene translation from cell keys that occur exactly once in both views.
///
/// Keys are disclosure identities, not globally usable world coordinates. A
/// changed stream/reset or branch, absent shared anchor, conflicting translations
/// or an offset outside `i32` starts a new scene. Duplicate keys cannot anchor it.
///
/// # Example
///
/// A stationary cell moves west in the observer frame when the player moves
/// east. The presentation offset compensates; a new attachment cannot reuse it.
///
/// ```
/// use tor_client_3d_model::SceneFrame;
/// use tor_protocol::{BranchId, CellView, Position, StreamContext, StreamId};
/// # fn cell(x: i32) -> CellView {
/// #     CellView {
/// #         key: "known-floor".into(), position: Position { x, y: 0, z: 0 },
/// #         wall: false, material: String::new(), asset: None, lit: true,
/// #         door: None, trap: None, object_presentation: None,
/// #         stairs_up: false, stairs_down: false, place_hint: false,
/// #     }
/// # }
/// let mut frame = SceneFrame::default();
/// let mut context = StreamContext { stream: StreamId("view".into()), epoch: 0 };
/// let branch = BranchId("play".into());
/// assert!(frame.observe(&[cell(0)], &context, &branch).discontinuous);
/// let moved = frame.observe(&[cell(-1)], &context, &branch);
/// assert!(!moved.discontinuous);
/// assert_eq!(moved.offset, Position { x: 1, y: 0, z: 0 });
/// context.epoch += 1;
/// let reset = frame.observe(&[cell(-1)], &context, &branch);
/// assert!(reset.discontinuous);
/// assert_eq!(reset.offset, Position { x: 0, y: 0, z: 0 });
/// ```
#[derive(Default)]
pub struct SceneFrame {
    previous: BTreeMap<String, Position>,
    offset: [i32; 3],
    identity: Option<(StreamContext, BranchId)>,
}

impl SceneFrame {
    /// Replace the previous anchor set and return this picture's alignment.
    ///
    /// The first call is discontinuous. Even a failed alignment retains this
    /// call's unique keys, so the next picture can establish a new translation.
    pub fn observe(
        &mut self,
        cells: &[CellView],
        context: &StreamContext,
        branch: &BranchId,
    ) -> FrameChange {
        let mut counts = BTreeMap::<&str, (Position, usize)>::new();
        for cell in cells {
            let entry = counts.entry(&cell.key).or_insert((cell.position, 0));
            entry.1 += 1;
        }
        let mut shift = None;
        let mut conflict = false;
        for (key, (now, count)) in &counts {
            if *count != 1 {
                continue;
            }
            if let Some(before) = self.previous.get(*key) {
                let delta = [
                    i64::from(now.x) - i64::from(before.x),
                    i64::from(now.y) - i64::from(before.y),
                    i64::from(now.z) - i64::from(before.z),
                ];
                conflict |= shift.is_some_and(|old| old != delta);
                shift = Some(delta);
            }
        }
        let same_identity = self
            .identity
            .as_ref()
            .is_some_and(|(old_context, old_branch)| {
                old_context == context && old_branch == branch
            });
        let translated = shift
            .filter(|_| same_identity && !conflict)
            .and_then(|delta| {
                Some([
                    i32::try_from(i64::from(self.offset[0]) - delta[0]).ok()?,
                    i32::try_from(i64::from(self.offset[1]) - delta[1]).ok()?,
                    i32::try_from(i64::from(self.offset[2]) - delta[2]).ok()?,
                ])
            });
        self.offset = translated.unwrap_or([0; 3]);
        self.identity = Some((context.clone(), branch.clone()));
        self.previous = counts
            .into_iter()
            .filter(|(_, (_, n))| *n == 1)
            .map(|(key, (position, _))| (key.to_owned(), position))
            .collect();
        FrameChange {
            offset: Position {
                x: self.offset[0],
                y: self.offset[1],
                z: self.offset[2],
            },
            discontinuous: translated.is_none(),
        }
    }
}
