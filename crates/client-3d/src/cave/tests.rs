use super::*;
use bevy::render::mesh::VertexAttributeValues;

fn cave(key: u64, seen: bool, lit: bool, above_lit: bool) -> CaveWorld {
    let mut world = CaveWorld::default();
    world.update(
        [IVec3::ZERO, IVec3::new(1, 1, 0)]
            .into_iter()
            .map(|at| (at, Solid { key, seen, lit }))
            .collect(),
        HashMap::from([(IVec3::Y, Appearance::terrain(seen, above_lit))]),
        HashSet::new(),
    );
    world
}

#[test]
fn current_unlit_and_memory_have_distinct_mesh_coverage_and_neither_glows() {
    let key = (0..1000)
        .find(|key| {
            !cave(*key, true, true, true)
                .build(IVec3::ZERO)
                .lights
                .is_empty()
        })
        .expect("positive crystal control");
    let lit = cave(key, true, true, true).build(IVec3::ZERO);
    assert!(lit.crystals.iter().any(Option::is_some));
    for (seen, lit, above_lit, coverage) in [(true, false, false, 0.5), (false, true, false, 0.0)] {
        let built = cave(key, seen, lit, above_lit).build(IVec3::ZERO);
        assert!(built.lights.is_empty());
        assert!(built.crystals.iter().all(Option::is_none));
        let rock = built.rock.unwrap();
        let Some(VertexAttributeValues::Float32x4(colors)) = rock.attribute(Mesh::ATTRIBUTE_COLOR)
        else {
            panic!("vertex mask")
        };
        assert!(!colors.is_empty());
        assert!(colors.iter().all(|color| color[3] == coverage));
    }
    assert!(cave(key, true, false, true)
        .build(IVec3::ZERO)
        .lights
        .is_empty());
    assert!(
        cave(key, true, true, false)
            .build(IVec3::ZERO)
            .lights
            .is_empty(),
        "a lit floor must not grow a light into a dark open cell"
    );
}

#[test]
fn changing_only_illumination_rebuilds_the_affected_chunks() {
    let mut world = cave(12, true, true, true);
    let solids = world
        .solids
        .iter()
        .map(|(at, solid)| {
            (
                *at,
                Solid {
                    lit: false,
                    ..*solid
                },
            )
        })
        .collect();
    let dirty = world.update(solids, world.open.clone(), HashSet::new());
    assert!(dirty.contains(&IVec3::ZERO));
}

#[test]
fn losing_sight_of_an_unlit_open_cell_rebuilds_adjacent_support_at_chunk_edge() {
    let mut world = CaveWorld::default();
    let floor = IVec3::new(0, CHUNK - 1, 0);
    let air = floor + IVec3::Y;
    let solids = HashMap::from([(
        floor,
        Solid {
            seen: true,
            lit: false,
            key: 75,
        },
    )]);
    world.update(
        solids.clone(),
        HashMap::from([(air, Appearance::Unlit)]),
        HashSet::new(),
    );
    let dirty = world.update(
        solids,
        HashMap::from([(air, Appearance::Remembered)]),
        HashSet::new(),
    );
    assert!(dirty.contains(&chunk_of(floor)));
    assert!(dirty.contains(&chunk_of(air)));
    let mesh = world.build(chunk_of(floor)).rock.unwrap();
    let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute(Mesh::ATTRIBUTE_COLOR)
    else {
        panic!("expected a coverage attribute");
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        panic!("expected geometric normals");
    };
    let top = colors
        .iter()
        .zip(normals)
        .filter(|(_, n)| n[1] > 0.9)
        .collect::<Vec<_>>();
    assert!(!top.is_empty());
    assert!(top.iter().all(|(rgba, _)| rgba[3] == 0.0));
}
