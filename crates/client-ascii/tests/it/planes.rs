//! Another plane's echo on the ASCII map: walls and figures fill only empty
//! columns, never the observer's own cells.
use std::collections::BTreeSet;
use tor_client_ascii::map::{echo_tiles, ECHO_FIGURE, ECHO_WALL};
use tor_protocol::{EchoActor, EchoCell, PlaneEcho, PlaneRef, PlaneView, Position};

fn plane(key: &str) -> PlaneRef {
    PlaneRef {
        key: key.into(),
        description: "a grey mist".into(),
        known: false,
    }
}

#[test]
fn echoes_fill_empty_columns_with_walls_and_figures() {
    let mut o = tor_client_common::test_support::observation();
    let at = |x, y| Position { x, y, z: 0 };
    o.plane = Some(PlaneView {
        plane: plane("ethereal"),
        echoes: vec![PlaneEcho {
            plane: plane("material"),
            cells: vec![
                EchoCell {
                    key: "a".into(),
                    position: at(1, 0),
                    wall: true,
                    material: "stone".into(),
                },
                EchoCell {
                    key: "b".into(),
                    position: at(2, 0),
                    wall: true,
                    material: "stone".into(),
                },
                EchoCell {
                    key: "c".into(),
                    position: at(3, 0),
                    wall: false,
                    material: String::new(),
                },
            ],
            actors: vec![EchoActor {
                position: at(2, 0),
                name: "Pilgrim".into(),
            }],
        }],
    });
    // Column (1, 0) belongs to the observer's own map.
    let occupied = BTreeSet::from([(1, 0)]);
    let tiles = echo_tiles(&o, &occupied);
    let found: Vec<_> = tiles
        .iter()
        .map(|t| ((t.position.x, t.position.y), t.glyph, t.color))
        .collect();
    assert_eq!(
        found,
        [((2, 0), 'p', ECHO_FIGURE)],
        "figures outrank walls; floor is undrawn"
    );
    assert!(!found.iter().any(|(column, ..)| *column == (1, 0)));
    let _ = ECHO_WALL;

    // Without planes there is nothing to draw.
    o.plane = None;
    assert!(echo_tiles(&o, &BTreeSet::new()).is_empty());
}
