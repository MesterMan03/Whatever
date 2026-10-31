use super::{PhysicsRegistry, config::*, registry::reference};
use crate::ecs::{EntityId, World};
use serde_json::{Value, json};

fn set(physics: &mut PhysicsRegistry, world: &mut World, id: EntityId, kind: &str, data: Value) {
    let data = physics
        .normalize_component(world, id, kind, data)
        .expect("valid physics component");
    assert!(world.set_component(&id, kind, data));
    physics.invalidate();
}
fn transform(world: &mut World, id: EntityId, position: [f32; 3]) {
    assert!(world.set_component(
        &id,
        "core:transform",
        json!({"position":position,"rotation":[0,0,0,1],"scale":[1,1,1]})
    ));
}
fn body(
    physics: &mut PhysicsRegistry,
    world: &mut World,
    position: [f32; 3],
    kind: &str,
) -> EntityId {
    let id = world.create_entity();
    transform(world, id, position);
    set(physics, world, id, BODY, json!({"type":kind}));
    set(
        physics,
        world,
        id,
        COLLIDER,
        json!({"shape":{"type":"box","size":[1,1,1]},"mass":2}),
    );
    id
}
fn command(
    physics: &mut PhysicsRegistry,
    world: &mut World,
    id: EntityId,
    action: &str,
    vector: [f32; 3],
) {
    physics
        .request(
            world,
            json!({"op":"body","entity":reference(id),"action":action,"vector":vector}),
        )
        .expect("body command");
}
fn steps(physics: &mut PhysicsRegistry, world: &mut World, count: u64) {
    for tick in 1..=count {
        physics.begin_tick(tick);
        physics.step(world, 1.0 / 60.0).expect("physics step");
    }
}
fn state(world: &World, id: EntityId) -> Value {
    world.get_component(&id, STATE).expect("body state")
}

#[test]
fn gravity_contacts_and_queries_use_full_box_dimensions() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let floor = world.create_entity();
    transform(&mut world, floor, [0.0, -0.5, 0.0]);
    set(
        &mut physics,
        &mut world,
        floor,
        COLLIDER,
        json!({"body":null,"shape":{"type":"box","size":[100,1,100]}}),
    );
    let cube = body(&mut physics, &mut world, [0.0, 5.0, 0.0], "dynamic");
    let default = physics.ensure_default(&mut world);
    let hit=physics.request(&mut world,json!({"op":"castRay","world":reference(default),"origin":[0,10,0],"direction":[0,-4,0],"maxDistance":20})).expect("query");
    assert_eq!(hit["collider"], reference(cube));
    assert!((hit["distance"].as_f64().expect("distance") - 4.5).abs() < 0.001);
    command(&mut physics, &mut world, cube, "teleport", [3.0, 5.0, 0.0]);
    let hit=physics.request(&mut world,json!({"op":"castRay","world":reference(default),"origin":[0,10,0],"direction":[0,-1,0],"maxDistance":20})).expect("query after move");
    assert_eq!(hit["collider"], reference(floor));
    steps(&mut physics, &mut world, 180);
    let position = world.world_transform(&cube).expect("transform").position;
    assert!(
        (position[1] - 0.5).abs() < 0.03,
        "cube should rest on floor: {position:?}"
    );
}

#[test]
fn compounds_are_separate_ecs_colliders_and_stale_references_never_rebind() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let owner = world.create_entity();
    set(
        &mut physics,
        &mut world,
        owner,
        BODY,
        json!({"type":"dynamic"}),
    );
    let a = world.create_entity();
    let b = world.create_entity();
    for (id, x) in [(a, -1.0), (b, 1.0)] {
        set(
            &mut physics,
            &mut world,
            id,
            COLLIDER,
            json!({"body":reference(owner),"shape":{"type":"sphere","radius":0.5},"position":[x,0,0],"mass":3}),
        );
    }
    physics.synchronize(&mut world).expect("sync");
    assert!((state(&world, owner)["mass"].as_f64().expect("mass") - 6.0).abs() < 0.001);
    assert_eq!(world.query(&[COLLIDER]).len(), 2);
    world.remove_component(&owner, BODY);
    physics.invalidate();
    physics.synchronize(&mut world).expect("remove body");
    assert_eq!(
        world.get_component(&a, STATUS).expect("status")["collider"]["active"],
        false
    );
    assert!(world.get_component(&a, COLLIDER).is_some());
    assert!(world.get_component(&owner, STATE).is_none());
    set(
        &mut physics,
        &mut world,
        owner,
        BODY,
        json!({"type":"fixed"}),
    );
    physics.synchronize(&mut world).expect("reactivate");
    assert_eq!(
        world.get_component(&a, STATUS).expect("status")["collider"]["active"],
        true
    );
    world.destroy_entity(owner);
    physics.invalidate();
    let replacement = world.create_entity();
    assert_eq!(replacement.index, owner.index);
    assert_ne!(replacement.generation, owner.generation);
    set(
        &mut physics,
        &mut world,
        replacement,
        BODY,
        json!({"type":"fixed"}),
    );
    physics.synchronize(&mut world).expect("reuse slot");
    assert_eq!(
        world.get_component(&a, STATUS).expect("status")["collider"]["active"],
        false
    );
    assert_eq!(
        world.get_component(&a, COLLIDER).expect("config")["body"],
        reference(owner)
    );
}

#[test]
fn configuration_preserves_velocity_and_sleep_and_low_level_mass_is_applied() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let id = body(&mut physics, &mut world, [0.0, 5.0, 0.0], "dynamic");
    command(
        &mut physics,
        &mut world,
        id,
        "setLinearVelocity",
        [4.0, 2.0, 1.0],
    );
    set(
        &mut physics,
        &mut world,
        id,
        BODY,
        json!({"linearDamping":0.2,"rapier":{"additional_solver_iterations":7,"additional_pgs_iterations":3,"additional_mass":3,"dominance_group":2,"activation":{"time_until_sleep":1.5},"user_data":"340282366920938463463374607431768211455"}}),
    );
    physics.synchronize(&mut world).expect("edit config");
    assert_eq!(state(&world, id)["linearVelocity"], json!([4.0, 2.0, 1.0]));
    assert!((state(&world, id)["mass"].as_f64().expect("mass") - 5.0).abs() < 0.001);
    physics
        .request(
            &mut world,
            json!({"op":"body","entity":reference(id),"action":"sleep"}),
        )
        .expect("sleep");
    let mut data = world.get_component(&id, BODY).expect("body config");
    data["angularDamping"] = json!(0.1);
    set(&mut physics, &mut world, id, BODY, data);
    physics.synchronize(&mut world).expect("sleeping edit");
    assert_eq!(state(&world, id)["sleeping"], true);
    assert!(
        physics
            .normalize_component(&mut world, id, STATE, json!({}))
            .is_err()
    );
    assert!(
        physics
            .normalize_component(&mut world, id, BODY, json!({"rapier":{"typo":true}}))
            .is_err()
    );
    assert!(
        physics
            .normalize_component(&mut world, id, COLLIDER, json!({"mass":2,"density":3}))
            .is_err()
    );
}

#[test]
fn world_settings_are_independent_normalized_and_engine_clock_is_owned() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let custom = world.create_entity();
    set(
        &mut physics,
        &mut world,
        custom,
        WORLD,
        json!({"gravity":[0,0,0],"substeps":3,"rapier":{"integration":{"num_solver_iterations":8,"max_ccd_substeps":4,"contact_softness":{"natural_frequency":40},"friction_model":"Coulomb"}}}),
    );
    let id = world.create_entity();
    transform(&mut world, id, [0.0, 5.0, 0.0]);
    set(
        &mut physics,
        &mut world,
        id,
        BODY,
        json!({"world":reference(custom),"rapier":{"additional_mass":1}}),
    );
    command(
        &mut physics,
        &mut world,
        id,
        "setLinearVelocity",
        [1.0, 0.0, 0.0],
    );
    steps(&mut physics, &mut world, 60);
    let position = world.world_transform(&id).expect("pose").position;
    assert!((position[0] - 1.0).abs() < 0.002);
    assert_eq!(position[1], 5.0);
    let settings = world.get_component(&custom, WORLD).expect("settings");
    assert_eq!(
        settings["rapier"]["integration"]["num_solver_iterations"],
        8
    );
    assert!(settings["rapier"]["integration"]["contact_softness"]["damping_ratio"].is_number());
    assert!(settings["rapier"]["integration"].get("dt").is_none());
    assert!(
        physics
            .normalize_component(
                &mut world,
                custom,
                WORLD,
                json!({"rapier":{"integration":{"dt":0.1}}})
            )
            .is_err()
    );
    assert!(
        physics
            .normalize_component(
                &mut world,
                custom,
                WORLD,
                json!({"rapier":{"integration":{"unknown_knob":2}}})
            )
            .is_err()
    );
    assert!(
        physics
            .normalize_component(&mut world, custom, WORLD, json!({"substeps":0}))
            .is_err()
    );
    set(
        &mut physics,
        &mut world,
        custom,
        WORLD,
        json!({"timeScale":0}),
    );
    steps(&mut physics, &mut world, 10);
    assert_eq!(
        world.world_transform(&id).expect("paused pose").position,
        position
    );
    world.remove_component(&custom, WORLD);
    physics.invalidate();
    physics.synchronize(&mut world).expect("remove world");
    assert_eq!(
        world.get_component(&id, STATUS).expect("status")["body"]["active"],
        false
    );
}

#[test]
fn fixed_joints_hold_for_both_solvers_and_cross_world_joints_are_inactive() {
    for solver in ["impulse", "multibody"] {
        let mut world = World::new();
        let mut physics = PhysicsRegistry::default();
        let a = body(&mut physics, &mut world, [0.0, 5.0, 0.0], "fixed");
        let b = body(&mut physics, &mut world, [2.0, 5.0, 0.0], "dynamic");
        let joint = world.create_entity();
        set(
            &mut physics,
            &mut world,
            joint,
            JOINT,
            json!({"bodyA":reference(a),"bodyB":reference(b),"anchorB":[-2,0,0],"solver":solver,"rapier":{"softness":{"natural_frequency":1000,"damping_ratio":1}}}),
        );
        command(
            &mut physics,
            &mut world,
            b,
            "applyImpulse",
            [0.0, -20.0, 0.0],
        );
        steps(&mut physics, &mut world, 90);
        assert_eq!(
            world.get_component(&joint, STATUS).expect("joint status")["joint"]["active"],
            true
        );
        let y = world.world_transform(&b).expect("held pose").position[1];
        assert!((y - 5.0).abs() < 0.05, "{solver} joint drifted: {y}");
        let other = world.create_entity();
        set(&mut physics, &mut world, other, WORLD, json!({}));
        set(
            &mut physics,
            &mut world,
            b,
            BODY,
            json!({"world":reference(other)}),
        );
        physics.synchronize(&mut world).expect("cross world");
        assert_eq!(
            world.get_component(&joint, STATUS).expect("joint status")["joint"]["active"],
            false
        );
        assert!(world.get_component(&joint, JOINT).is_some());
    }
}

#[test]
fn revolute_limits_motors_and_generic_axes_are_native_settings() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let a = body(&mut physics, &mut world, [0.0, 5.0, 0.0], "fixed");
    let b = body(&mut physics, &mut world, [0.0, 3.0, 0.0], "dynamic");
    let joint = world.create_entity();
    set(
        &mut physics,
        &mut world,
        joint,
        JOINT,
        json!({"type":"revolute","bodyA":reference(a),"bodyB":reference(b),"anchorB":[0,2,0],"axisA":[0,0,1],"axisB":[0,0,1],"limits":{"min":-0.5,"max":0.5},"motor":{"targetVelocity":1,"damping":10,"maxForce":30},"rapier":{"coupled_axes":[],"user_data":"42"}}),
    );
    steps(&mut physics, &mut world, 120);
    let pose = world.world_transform(&b).expect("motor pose");
    assert!(pose.rotation[2].abs() > 0.01, "motor should rotate body");
    assert!(
        pose.rotation[2].abs() < 0.3,
        "hinge must respect angular limit: {:?}",
        pose.rotation
    );
    assert!(physics.normalize_component(&mut world,joint,JOINT,json!({"type":"generic","bodyA":reference(a),"bodyB":reference(b),"rapier":{"locked_axes":["linearX","linearY","linearZ"],"limits":{"angularZ":{"min":-1,"max":1}},"motors":{"angularX":{"targetVelocity":0.3,"damping":1}}}})).is_ok());
}

#[test]
fn physics_writes_parented_bodies_back_in_local_space() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let parent = world.create_entity();
    world.set_component(
        &parent,
        "core:transform",
        json!({"position":[10,2,0],"rotation":[0,0.70710677,0,0.70710677],"scale":[2,2,2]}),
    );
    let child = body(&mut physics, &mut world, [1.0, 3.0, 0.0], "dynamic");
    assert!(world.set_parent(child, Some(parent)));
    physics.synchronize(&mut world).expect("sync parent");
    let before = world.world_transform(&child).expect("before").position;
    steps(&mut physics, &mut world, 30);
    let after = world.world_transform(&child).expect("after").position;
    assert!((before[0] - after[0]).abs() < 0.002);
    assert!((before[2] - after[2]).abs() < 0.002);
    assert!(after[1] < before[1] - 1.0);
    assert!(world.transforms[&child.index].position[1] < 3.0);
    assert_eq!(
        world.get_component(&child, BODY).expect("config")["world"],
        reference(physics.ensure_default(&mut world))
    );
}

#[test]
fn sensors_report_events_and_axis_velocity_preserves_gravity() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let sensor = world.create_entity();
    set(
        &mut physics,
        &mut world,
        sensor,
        COLLIDER,
        json!({"body":null,"shape":{"type":"sphere","radius":2},"sensor":true,"events":["collision"]}),
    );
    let dynamic = body(&mut physics, &mut world, [0.0, 0.0, 0.0], "dynamic");
    command(
        &mut physics,
        &mut world,
        dynamic,
        "setLinearVelocity",
        [0.0, -3.0, 0.0],
    );
    physics.request(&mut world,json!({"op":"body","entity":reference(dynamic),"action":"setLinearVelocity","vector":[2,0,1],"axes":["x","z"]})).expect("masked velocity");
    assert_eq!(
        state(&world, dynamic)["linearVelocity"],
        json!([2.0, -3.0, 1.0])
    );
    steps(&mut physics, &mut world, 1);
    let events = physics.take_events();
    assert!(
        events
            .iter()
            .any(|event| event["kind"] == "started" && event["sensor"] == true)
    );
    assert!(
        state(&world, dynamic)["linearVelocity"][1]
            .as_f64()
            .expect("vertical velocity")
            < -3.0
    );
}

#[test]
fn a_dynamic_capsule_can_push_a_cube_by_walking() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let floor = world.create_entity();
    transform(&mut world, floor, [0.0, -0.5, 0.0]);
    set(
        &mut physics,
        &mut world,
        floor,
        COLLIDER,
        json!({"body":null,"shape":{"type":"box","size":[100,1,100]},"friction":0.7}),
    );
    let player = world.create_entity();
    transform(&mut world, player, [0.0, 1.0, -3.0]);
    set(
        &mut physics,
        &mut world,
        player,
        BODY,
        json!({"ccd":true,"lockedAxes":["rotationX","rotationY","rotationZ"]}),
    );
    set(
        &mut physics,
        &mut world,
        player,
        COLLIDER,
        json!({"shape":{"type":"capsule","radius":0.4,"height":2},"mass":75,"friction":0}),
    );
    let cube = body(&mut physics, &mut world, [0.0, 0.5, 0.0], "dynamic");
    for tick in 1..=150 {
        physics.begin_tick(tick);
        physics.request(&mut world,json!({"op":"body","entity":reference(player),"action":"setLinearVelocity","vector":[0,0,3],"axes":["x","z"]})).expect("walk");
        physics.step(&mut world, 1.0 / 60.0).expect("step");
    }
    let position = world.world_transform(&cube).expect("cube pose").position;
    assert!(
        position[2] > 3.0,
        "walking capsule must push cube: {position:?}"
    );
}

#[test]
fn identical_commands_produce_identical_solver_states() {
    fn run(queries: bool) -> Vec<Value> {
        let mut world = World::new();
        let mut physics = PhysicsRegistry::default();
        let floor = world.create_entity();
        transform(&mut world, floor, [0.0, -0.5, 0.0]);
        set(
            &mut physics,
            &mut world,
            floor,
            COLLIDER,
            json!({"body":null,"shape":{"type":"box","size":[30,1,30]}}),
        );
        let mut ids = Vec::new();
        for index in 0..12 {
            ids.push(body(
                &mut physics,
                &mut world,
                [
                    (index % 3) as f32 * 0.9,
                    1.0 + (index / 3) as f32 * 1.1,
                    0.0,
                ],
                "dynamic",
            ));
        }
        let arena = physics.ensure_default(&mut world);
        for tick in 1..=180 {
            physics.begin_tick(tick);
            if queries {
                physics.request(&mut world,json!({"op":"castRay","world":reference(arena),"origin":[0,10,0],"direction":[0,-1,0],"maxDistance":20})).expect("read-only query");
            }
            physics.step(&mut world, 1.0 / 60.0).expect("step");
        }
        ids.into_iter().map(|id| state(&world, id)).collect()
    }
    let baseline = run(false);
    assert_eq!(baseline, run(false));
    assert_eq!(
        baseline,
        run(true),
        "queries must not alter the solver's future state"
    );
}

#[test]
fn world_migration_keeps_motion_even_when_the_old_world_is_removed() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let a = world.create_entity();
    let b = world.create_entity();
    for id in [a, b] {
        set(
            &mut physics,
            &mut world,
            id,
            WORLD,
            json!({"gravity":[0,0,0]}),
        );
    }
    let id = world.create_entity();
    set(
        &mut physics,
        &mut world,
        id,
        BODY,
        json!({"world":reference(a)}),
    );
    set(
        &mut physics,
        &mut world,
        id,
        COLLIDER,
        json!({"shape":{"type":"sphere","radius":1},"mass":2}),
    );
    command(
        &mut physics,
        &mut world,
        id,
        "setLinearVelocity",
        [4.0, 2.0, 1.0],
    );
    set(
        &mut physics,
        &mut world,
        id,
        BODY,
        json!({"world":reference(b)}),
    );
    world.remove_component(&a, WORLD);
    physics.invalidate();
    physics.synchronize(&mut world).expect("migrate");
    assert_eq!(state(&world, id)["world"], reference(b));
    assert_eq!(state(&world, id)["linearVelocity"], json!([4.0, 2.0, 1.0]));
    assert!((state(&world, id)["mass"].as_f64().expect("mass") - 2.0).abs() < 0.001);
    steps(&mut physics, &mut world, 60);
    assert!((world.world_transform(&id).expect("pose").position[0] - 4.0).abs() < 0.002);
}

#[test]
fn kinematic_targets_are_distributed_across_world_substeps() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let id = body(
        &mut physics,
        &mut world,
        [0.0, 5.0, 0.0],
        "kinematicPosition",
    );
    let arena = physics.ensure_default(&mut world);
    set(
        &mut physics,
        &mut world,
        arena,
        WORLD,
        json!({"substeps":4}),
    );
    physics.request(&mut world,json!({"op":"body","entity":reference(id),"action":"setKinematicTarget","vector":[1,5,0]})).expect("kinematic target");
    steps(&mut physics, &mut world, 1);
    let state = state(&world, id);
    assert!((state["position"][0].as_f64().expect("position") - 1.0).abs() < 0.001);
    assert!((state["linearVelocity"][0].as_f64().expect("velocity") - 60.0).abs() < 0.02);
}

#[test]
fn inactive_parented_bodies_reactivate_when_the_parent_scale_is_fixed() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let parent = world.create_entity();
    world.set_component(
        &parent,
        "core:transform",
        json!({"position":[0,0,0],"rotation":[0,0,0,1],"scale":[0,0,0]}),
    );
    let child = body(&mut physics, &mut world, [0.0, 5.0, 0.0], "dynamic");
    world.set_parent(child, Some(parent));
    physics.synchronize(&mut world).expect("invalid parent");
    assert_eq!(
        world.get_component(&child, STATUS).expect("status")["body"]["active"],
        false
    );
    physics.component_changed(&world, parent, "core:transform");
    transform(&mut world, parent, [0.0, 0.0, 0.0]);
    physics.synchronize(&mut world).expect("fixed parent");
    assert_eq!(
        world.get_component(&child, STATUS).expect("status")["body"]["active"],
        true
    );
}

#[test]
fn recursive_scene_destruction_clears_derived_state_before_slots_are_reused() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let root = world.create_entity();
    let child = body(&mut physics, &mut world, [0.0, 2.0, 0.0], "dynamic");
    world.set_parent(child, Some(root));
    physics.synchronize(&mut world).expect("sync");
    assert!(world.get_component(&child, STATE).is_some());
    physics.entity_destroyed(&world, root);
    assert!(world.destroy_entity(root));
    for _ in 0..2 {
        let replacement = world.create_entity();
        assert!(
            world.get_component(&replacement, STATE).is_none(),
            "new entities cannot inherit destroyed children's native state"
        );
    }
    physics.step(&mut world, 1.0 / 60.0).expect("cleanup");
}

#[test]
fn destroying_a_scene_parent_of_a_world_deactivates_external_dependents() {
    let mut world = World::new();
    let mut physics = PhysicsRegistry::default();
    let room = world.create_entity();
    let arena = world.create_entity();
    world.set_parent(arena, Some(room));
    set(&mut physics, &mut world, arena, WORLD, json!({}));
    let external = world.create_entity();
    set(
        &mut physics,
        &mut world,
        external,
        BODY,
        json!({"world":reference(arena)}),
    );
    set(
        &mut physics,
        &mut world,
        external,
        COLLIDER,
        json!({"shape":{"type":"sphere","radius":1}}),
    );
    physics.synchronize(&mut world).expect("sync");
    physics.entity_destroyed(&world, room);
    assert!(world.destroy_entity(room));
    physics
        .step(&mut world, 1.0 / 60.0)
        .expect("cleanup missing world");
    assert!(world.is_alive(&external));
    assert!(world.get_component(&external, BODY).is_some());
    assert!(world.get_component(&external, STATE).is_none());
    assert_eq!(
        world.get_component(&external, STATUS).expect("status")["body"]["active"],
        false
    );
}
