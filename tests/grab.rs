use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::grab::{GrabPlugin, Grabbable, Grips, LEFT_WING, RIGHT_WING};
use pigeon_post::pigeon::{PigeonInput, PigeonPlugin, pigeon_body};

/// The box lies on the floor 1.6 m in front of the pigeon, along -Z.
const BOX_SIZE: f32 = 0.6;
/// The center of a box that a wing holds up, clearly above a box on the floor or on its edge.
const LIFTED: f32 = BOX_SIZE / 2.0 + 0.2;

/// A headless app with a floor, a pigeon at the origin, and a grabbable box in front of it.
fn app_with_box(density: f32) -> (App, Entity, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        PhysicsPlugins::default(),
        PigeonPlugin,
        GrabPlugin,
    ))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
        1.0 / 60.0,
    )));
    app.finish();

    app.world_mut().spawn((
        RigidBody::Static,
        Collider::cuboid(50.0, 1.0, 50.0),
        Transform::from_xyz(0.0, -0.5, 0.0),
    ));
    let pigeon = app
        .world_mut()
        .spawn((pigeon_body(), Transform::from_xyz(0.0, 0.5, 0.0)))
        .id();
    let item = app
        .world_mut()
        .spawn((
            RigidBody::Dynamic,
            Collider::cuboid(BOX_SIZE, BOX_SIZE, BOX_SIZE),
            ColliderDensity(density),
            Grabbable,
            Transform::from_xyz(0.0, BOX_SIZE / 2.0, -1.6),
        ))
        .id();
    (app, pigeon, item)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn set_input(app: &mut App, pigeon: Entity, pitch: f32, wings: [bool; 2]) {
    let mut input = app
        .world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input");
    input.look = Vec2::new(0.0, pitch);
    input.grab = wings;
}

/// Looks down at the box and grabs it with the given wings, then looks up to lift it.
fn grab_and_lift(app: &mut App, pigeon: Entity, wings: [bool; 2], lift_seconds: f32) {
    set_input(app, pigeon, -0.35, wings);
    run_seconds(app, 0.2);
    set_input(app, pigeon, 0.3, wings);
    run_seconds(app, lift_seconds);
}

fn height(app: &App, entity: Entity) -> f32 {
    app.world()
        .get::<Position>(entity)
        .expect("entity has a position")
        .y
}

fn grips(app: &App, pigeon: Entity) -> Grips {
    *app.world().get::<Grips>(pigeon).expect("pigeon has grips")
}

#[test]
fn one_wing_lifts_a_light_box() {
    let (mut app, pigeon, item) = app_with_box(2.0);
    run_seconds(&mut app, 1.0);
    grab_and_lift(&mut app, pigeon, [true, false], 1.5);

    assert_eq!(
        grips(&app, pigeon).0[LEFT_WING].map(|grip| grip.body),
        Some(item)
    );
    assert!(grips(&app, pigeon).0[RIGHT_WING].is_none());
    let lifted = height(&app, item);
    assert!(lifted > LIFTED, "box at {lifted} m");
}

#[test]
fn released_box_falls() {
    let (mut app, pigeon, item) = app_with_box(2.0);
    run_seconds(&mut app, 1.0);
    grab_and_lift(&mut app, pigeon, [true, false], 1.5);
    set_input(&mut app, pigeon, 0.3, [false, false]);
    run_seconds(&mut app, 1.5);

    assert!(grips(&app, pigeon).0[LEFT_WING].is_none());
    let rest = height(&app, item);
    assert!((rest - BOX_SIZE / 2.0).abs() < 0.1, "box at {rest} m");
}

#[test]
fn heavy_box_needs_both_wings() {
    // 0.216 m³ at this density is about 6 kg: a weight of 59 N, between one wing (40 N)
    // and two wings (80 N).
    let (mut one, one_pigeon, one_box) = app_with_box(28.0);
    run_seconds(&mut one, 1.0);
    grab_and_lift(&mut one, one_pigeon, [true, false], 2.0);

    let (mut two, two_pigeon, two_box) = app_with_box(28.0);
    run_seconds(&mut two, 1.0);
    grab_and_lift(&mut two, two_pigeon, [true, true], 2.0);

    let with_one = height(&one, one_box);
    let with_two = height(&two, two_box);
    assert!(with_one < LIFTED, "one wing lifted the box to {with_one} m");
    assert!(
        with_two > LIFTED,
        "two wings lifted the box only to {with_two} m"
    );
}

#[test]
fn nothing_to_grab_out_of_reach() {
    let (mut app, pigeon, item) = app_with_box(2.0);
    app.world_mut()
        .entity_mut(item)
        .insert(Transform::from_xyz(0.0, BOX_SIZE / 2.0, -6.0));
    run_seconds(&mut app, 1.0);
    grab_and_lift(&mut app, pigeon, [true, true], 0.5);

    assert_eq!(grips(&app, pigeon), Grips::default());
}
