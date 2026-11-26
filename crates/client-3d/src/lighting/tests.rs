use super::*;

#[test]
fn death_and_resurrection_while_dark_preserve_the_creatures_glow_state() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    let source = library.materials.glow(props::Accent::Amber);
    let ember = library.materials.dim(&source).unwrap();
    app.insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, (shade, lamps).chain());
    let root = app.world_mut().spawn(Appearance::Lit).id();
    let mesh = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    let lamp = app
        .world_mut()
        .spawn((
            PointLight {
                intensity: 123.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    app.update();
    app.world_mut().entity_mut(root).insert(Appearance::Unlit);
    app.update();
    app.world_mut()
        .entity_mut(root)
        .insert(creatures::Snuffed::default());
    app.update();
    app.world_mut().entity_mut(root).insert(Appearance::Lit);
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        ember
    );
    assert_eq!(app.world().get::<PointLight>(lamp).unwrap().intensity, 0.0);
    app.world_mut()
        .entity_mut(root)
        .remove::<creatures::Snuffed>();
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        source
    );
    assert_eq!(
        app.world().get::<PointLight>(lamp).unwrap().intensity,
        123.0
    );
}

#[test]
fn dark_roots_keep_pigments_and_suppress_lights_including_new_children() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let source = materials.add(StandardMaterial {
        base_color: Color::linear_rgba(0.7, 0.4, 0.2, 0.8),
        emissive: LinearRgba::WHITE,
        ..default()
    });
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    let initial_materials = materials.len();
    app.init_resource::<Game>()
        .insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, (shade, lamps).chain());
    let root = app.world_mut().spawn(Appearance::Lit).id();
    let mesh = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    let lamp = app
        .world_mut()
        .spawn((
            PointLight {
                intensity: 123.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        source
    );
    for lit in [false, true, false, true] {
        app.world_mut().entity_mut(root).insert(if lit {
            Appearance::Lit
        } else {
            Appearance::Unlit
        });
        app.world_mut().resource_mut::<Game>().view_serial += 1;
        app.update();
        let handle = &app
            .world()
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0;
        assert_eq!(*handle == source, lit);
        let material = app
            .world()
            .resource::<Assets<StandardMaterial>>()
            .get(handle)
            .unwrap();
        assert!(
            !material.unlit,
            "studio light still reveals form in darkness"
        );
        if !lit {
            assert_eq!(material.emissive, LinearRgba::BLACK);
            let color = material.base_color.to_linear();
            assert!(color.red > color.green && color.green > color.blue);
            assert!((color.alpha - 0.8).abs() < 1e-5);
        }
        assert_eq!(
            app.world().get::<PointLight>(lamp).unwrap().intensity,
            if lit { 123.0 } else { 0.0 }
        );
    }
    app.world_mut().entity_mut(root).insert(Appearance::Unlit);
    app.world_mut().resource_mut::<Game>().view_serial += 1;
    app.update();
    let held = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    app.update();
    assert_ne!(
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(held)
            .unwrap()
            .0,
        source
    );
    assert_eq!(
        app.world().resource::<Assets<StandardMaterial>>().len(),
        initial_materials + 2,
        "one readable variant per source, not per descendant or frame"
    );
}

#[test]
fn fading_a_body_never_changes_shared_materials_or_a_newly_claimed_corpse() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let source = materials.add(StandardMaterial {
        base_color: Color::srgba(0.6, 0.4, 0.8, 0.70),
        alpha_mode: AlphaMode::Blend,
        ..default()
    });
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, shade);
    let root = app.world_mut().spawn(Appearance::Lit).id();
    let neighbor = app.world_mut().spawn(Appearance::Lit).id();
    let mut part = |parent| {
        app.world_mut()
            .spawn((MeshMaterial3d(source.clone()), ChildOf(parent)))
            .id()
    };
    let mesh = part(root);
    let other_piece = part(root);
    let same_recipe = part(neighbor);
    app.world_mut()
        .entity_mut(other_piece)
        .insert(NotShadowCaster);
    let handle = |app: &App, e| {
        app.world()
            .get::<MeshMaterial3d<StandardMaterial>>(e)
            .unwrap()
            .0
            .clone()
    };
    let alpha = |app: &App, h: &Handle<StandardMaterial>| {
        app.world()
            .resource::<Assets<StandardMaterial>>()
            .get(h)
            .unwrap()
            .base_color
            .alpha()
    };

    app.update();
    app.world_mut().entity_mut(root).insert(CorpseOpacity(0.5));
    app.update();
    let fading = handle(&app, mesh);
    assert!(app.world().get::<NotShadowCaster>(mesh).is_some());
    assert!(app.world().get::<NotShadowCaster>(same_recipe).is_none());
    assert_ne!(fading, source);
    assert_eq!(
        handle(&app, other_piece),
        fading,
        "one fade copy per recipe and body"
    );
    assert_eq!(handle(&app, same_recipe), source);
    assert!((alpha(&app, &fading) - 0.35).abs() < 1e-5);
    assert!((alpha(&app, &source) - 0.70).abs() < 1e-5);

    app.world_mut().entity_mut(root).insert(CorpseOpacity(0.2));
    app.update();
    assert_eq!(
        handle(&app, mesh),
        fading,
        "update, don't allocate a material every frame"
    );
    assert!((alpha(&app, &fading) - 0.14).abs() < 1e-5);

    app.world_mut()
        .entity_mut(root)
        .insert(Appearance::Undisclosed);
    app.update();
    let dark = handle(&app, mesh);
    assert_ne!(dark, fading);
    assert!((alpha(&app, &dark) - 0.2).abs() < 1e-5);
    assert!(
        app.world()
            .resource::<Assets<StandardMaterial>>()
            .get(&dark)
            .unwrap()
            .unlit
    );

    app.world_mut()
        .entity_mut(root)
        .insert(Appearance::Lit)
        .remove::<CorpseOpacity>();
    app.update();
    assert_eq!(
        handle(&app, mesh),
        source,
        "an item claim must restore the recipe material"
    );
    assert_eq!(handle(&app, same_recipe), source);
    assert!(app.world().get::<NotShadowCaster>(mesh).is_none());
    assert!(app.world().get::<NotShadowCaster>(other_piece).is_some());
}

#[test]
fn remembered_terrain_is_neutral_and_casts_no_live_shadow_or_lamp() {
    let mut app = App::new();
    let mut materials = Assets::<StandardMaterial>::default();
    let source = materials.add(StandardMaterial {
        base_color: Color::linear_rgb(0.8, 0.4, 0.2),
        emissive: LinearRgba::WHITE,
        ..default()
    });
    let library = PropLibrary::new(PropMaterials::new(&mut materials));
    app.insert_resource(materials)
        .insert_resource(library)
        .add_systems(Update, (shade, lamps).chain());
    let root = app.world_mut().spawn(Appearance::Remembered).id();
    let mesh = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    let lamp = app
        .world_mut()
        .spawn((
            PointLight {
                intensity: 123.0,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    app.update();
    let world = app.world();
    let handle = &world
        .get::<MeshMaterial3d<StandardMaterial>>(mesh)
        .unwrap()
        .0;
    let muted = world
        .resource::<Assets<StandardMaterial>>()
        .get(handle)
        .unwrap();
    let c = muted.base_color.to_linear();
    assert!(c.red > 0.1 && c.red < 0.4 && c.red == c.green && c.red == c.blue);
    assert!(muted.unlit && muted.emissive == LinearRgba::BLACK);
    assert_eq!(world.get::<PointLight>(lamp).unwrap().intensity, 0.0);
    assert!(world.get::<NotShadowCaster>(mesh).is_some());
    app.world_mut().entity_mut(root).insert(Appearance::Lit);
    app.update();
    let world = app.world();
    assert_eq!(
        world
            .get::<MeshMaterial3d<StandardMaterial>>(mesh)
            .unwrap()
            .0,
        source
    );
    assert!(world.get::<NotShadowCaster>(mesh).is_none());
    assert_eq!(world.get::<PointLight>(lamp).unwrap().intensity, 123.0);
}

#[test]
fn current_darkness_suppresses_procedural_rim_without_flattening_the_model() {
    use crate::surface::{ModelExtension, ModelMaterial, SurfaceRecipe};
    let mut materials = Assets::<ModelMaterial>::default();
    let recipe = SurfaceRecipe::SPECTRAL.to_surface_params(true, false);
    assert!(
        recipe.finish.x > 0.1,
        "positive control: this recipe adds a shader rim"
    );
    let source = materials.add(ModelMaterial {
        base: StandardMaterial {
            emissive: LinearRgba::WHITE,
            ..default()
        },
        extension: ModelExtension {
            params: recipe,
            noise: None,
        },
    });
    let mut app = App::new();
    app.insert_resource(materials)
        .add_systems(Update, shade_models);
    let root = app.world_mut().spawn(Appearance::Unlit).id();
    let mesh = app
        .world_mut()
        .spawn((MeshMaterial3d(source.clone()), ChildOf(root)))
        .id();
    app.update();
    let world = app.world();
    let handle = &world.get::<MeshMaterial3d<ModelMaterial>>(mesh).unwrap().0;
    let assets = world.resource::<Assets<ModelMaterial>>();
    let copied = assets.get(handle).unwrap();
    assert_eq!(copied.base.emissive, LinearRgba::BLACK);
    assert_eq!(copied.extension.params.finish.x, 0.0);
    assert_eq!(copied.extension.params.detail, recipe.detail);
    assert_eq!(
        copied.extension.params.kind_and_flags,
        recipe.kind_and_flags
    );
    assert_eq!(
        assets.get(&source).unwrap().extension.params.finish.x,
        recipe.finish.x
    );
    assert!(!copied.base.unlit);
    app.world_mut().entity_mut(root).insert(Appearance::Lit);
    app.update();
    assert_eq!(
        app.world()
            .get::<MeshMaterial3d<ModelMaterial>>(mesh)
            .unwrap()
            .0,
        source
    );
}
