use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;
use pigeon_post::pigeon::{
    FLAP_COST, FLAP_SPEED, GLIDE_FALL_SPEED, Grounded, JUMP_SPEED, PigeonInput, PigeonPlugin,
    WALK_SPEED, pigeon_body,
};
use pigeon_post::stamina::Stamina;
/// A headless app with a floor at y = 0 and one pigeon 1 m above it.
fn app_with_pigeon() -> (App, Entity) {
    app_with_pigeon_at(1.0)
}

fn app_with_pigeon_at(height: f32) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
        PhysicsPlugins::default(),
        PigeonPlugin,
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
        .spawn((pigeon_body(), Transform::from_xyz(0.0, height, 0.0)))
        .id();
    (app, pigeon)
}

fn run_seconds(app: &mut App, seconds: f32) {
    for _ in 0..(seconds * 60.0) as usize {
        app.update();
    }
}

fn velocity(app: &App, pigeon: Entity) -> Vec3 {
    app.world()
        .get::<LinearVelocity>(pigeon)
        .expect("pigeon has a velocity")
        .0
}

fn input(app: &mut App, pigeon: Entity) -> Mut<'_, PigeonInput> {
    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
}

fn stamina(app: &App, pigeon: Entity) -> f32 {
    app.world()
        .get::<Stamina>(pigeon)
        .expect("pigeon has stamina")
        .value()
}

fn set_stamina(app: &mut App, pigeon: Entity, value: f32) {
    app.world_mut()
        .entity_mut(pigeon)
        .insert(Stamina::new(value));
}

fn is_grounded(app: &App, pigeon: Entity) -> bool {
    app.world()
        .get::<Grounded>(pigeon)
        .expect("pigeon has Grounded")
        .0
}

#[test]
fn pigeon_lands_on_floor() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 2.0);

    assert!(is_grounded(&app, pigeon));
    assert!(velocity(&app, pigeon).y.abs() < 0.1);
}

#[test]
fn pigeon_walks_forward_at_walk_speed() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 1.0);

    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .movement = Vec2::Y;
    run_seconds(&mut app, 1.0);

    let velocity = velocity(&app, pigeon);
    assert!((velocity.z + WALK_SPEED).abs() < 0.1, "velocity {velocity}");
    assert!(velocity.x.abs() < 0.1, "velocity {velocity}");
}

#[test]
fn pigeon_jumps_only_from_ground() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 1.0);

    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .jump = true;
    app.update();
    let after_jump = velocity(&app, pigeon).y;
    assert!(
        after_jump > JUMP_SPEED * 0.8,
        "vertical velocity {after_jump}"
    );

    run_seconds(&mut app, 0.2);
    assert!(!is_grounded(&app, pigeon));
    // Without stamina, a press in the air does nothing.
    set_stamina(&mut app, pigeon, 0.0);
    let before_second_press = velocity(&app, pigeon).y;
    app.world_mut()
        .get_mut::<PigeonInput>(pigeon)
        .expect("pigeon has input")
        .jump = true;
    app.update();
    assert!(velocity(&app, pigeon).y < before_second_press);
}

#[test]
fn pigeon_flaps_in_air_for_stamina() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 1.0);
    input(&mut app, pigeon).jump = true;
    run_seconds(&mut app, 0.3);
    assert!(velocity(&app, pigeon).y < FLAP_SPEED * 0.8);

    input(&mut app, pigeon).jump = true;
    app.update();

    let after_flap = velocity(&app, pigeon).y;
    assert!(
        after_flap > FLAP_SPEED * 0.9,
        "vertical velocity {after_flap}"
    );
    assert!((stamina(&app, pigeon) - (1.0 - FLAP_COST)).abs() < 1e-3);
}

#[test]
fn pigeon_glide_limits_fall_speed() {
    let (mut app, pigeon) = app_with_pigeon_at(30.0);
    input(&mut app, pigeon).glide = true;
    run_seconds(&mut app, 1.0);

    let fall = velocity(&app, pigeon).y;
    assert!(fall >= -GLIDE_FALL_SPEED - 0.2, "vertical velocity {fall}");
    assert!(stamina(&app, pigeon) < 1.0);
}

#[test]
fn pigeon_falls_fast_without_glide() {
    let (mut app, pigeon) = app_with_pigeon_at(30.0);
    run_seconds(&mut app, 1.0);

    assert!(velocity(&app, pigeon).y < -GLIDE_FALL_SPEED * 2.0);
}

#[test]
fn ground_refills_stamina() {
    let (mut app, pigeon) = app_with_pigeon();
    run_seconds(&mut app, 1.0);
    set_stamina(&mut app, pigeon, 0.0);

    run_seconds(&mut app, 1.0);

    let refilled = stamina(&app, pigeon);
    assert!((0.4..=0.6).contains(&refilled), "stamina {refilled}");
}
