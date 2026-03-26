mod module_bindings;
mod ui;

use bevy::prelude::*;
use bevy_spacetimedb::{
    DeleteEvent, InsertEvent, StdbConnectedEvent, StdbConnection, StdbPlugin,
};
use module_bindings::{
    DbConnection, Hero,
    hero_table::HeroTableAccess,
    create_hero_reducer::create_hero,
    move_hero_reducer::move_hero,
};
use ui::{OpeningEventPlugin, OpeningEventState, TriggerOpeningEvent, OpeningEventComplete};
use std::collections::HashSet;

const HERO_SIZE: f32 = 32.0;
const MOVE_SPEED: f32 = 200.0;
const SYNC_INTERVAL_SECS: f32 = 0.1;
const CAMERA_LERP_SPEED: f32 = 5.0;

const TILE_SIZE: f32 = 64.0;
const TILE_RENDER_RADIUS: i32 = 12;

const COLOR_LOCAL_HERO: Color = Color::srgb(1.0, 0.6, 0.2);
const COLOR_REMOTE_HERO: Color = Color::srgb(0.0, 0.8, 0.9);

// ─── Grid ────────────────────────────────────────────────────────────────────

enum TileType {
    Grass,
    Dirt,
}

fn tile_type(tx: i32, ty: i32) -> TileType {
    if (tx.wrapping_mul(31).wrapping_add(ty.wrapping_mul(17))).unsigned_abs() % 7 == 0 {
        TileType::Dirt
    } else {
        TileType::Grass
    }
}

#[derive(Component)]
struct GridTile {
    tx: i32,
    ty: i32,
}

#[derive(Resource, Default)]
struct SpawnedTiles(HashSet<(i32, i32)>);

// ─── Resources ───────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct LocalHero {
    id: Option<u64>,
    name: String,
    hero_requested: bool,
    is_new_hero: bool,
}

#[derive(Resource)]
struct SyncTimer(Timer);

impl Default for SyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(SYNC_INTERVAL_SECS, TimerMode::Repeating))
    }
}

// ─── Components ──────────────────────────────────────────────────────────────

#[derive(Component)]
struct HeroEntity {
    id: u64,
}

#[derive(Component)]
struct IsLocalPlayer;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct HeroNameText;

#[derive(Component)]
struct PositionText;

#[derive(Component)]
struct OriginText;

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "What May Become".into(),
                resolution: (1280., 720.).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(
            StdbPlugin::default()
                .with_connection(|send_connected, send_disconnected, send_connect_error, _app| {
                    let conn = DbConnection::builder()
                        .with_module_name("what-may-become")
                        .with_uri("http://localhost:3000")
                        .on_connect(move |_ctx, _identity, _token| {
                            send_connected.send(StdbConnectedEvent).unwrap();
                        })
                        .on_connect_error(move |_ctx, err| {
                            send_connect_error
                                .send(bevy_spacetimedb::StdbConnectionErrorEvent { err })
                                .unwrap();
                        })
                        .on_disconnect(move |_ctx, err| {
                            send_disconnected
                                .send(bevy_spacetimedb::StdbDisconnectedEvent { err })
                                .unwrap();
                        })
                        .build()
                        .expect("Failed to connect to SpacetimeDB");

                    conn.run_threaded();
                    conn
                })
                .with_events(|plugin, app, db, _reducers| {
                    plugin
                        .on_insert(app, db.hero())
                        .on_delete(app, db.hero());
                }),
        )
        .add_plugins(OpeningEventPlugin)
        .init_resource::<LocalHero>()
        .init_resource::<SyncTimer>()
        .init_resource::<SpawnedTiles>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                on_connected,
                on_hero_inserted,
                on_hero_deleted,
                handle_input,
                sync_position,
                smooth_camera_follow,
                update_hud,
                check_trigger_opening,
                on_opening_complete,
                update_grid,
            ),
        )
        .run();
}

// ─── Systems ─────────────────────────────────────────────────────────────────

fn setup(mut commands: Commands) {
    commands.spawn(Camera2d);

    // HUD
    commands
        .spawn(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(10.0),
            top: Val::Px(10.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            ..default()
        })
        .with_children(|p| {
            p.spawn((
                Text::new("Status: Connecting..."),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::WHITE),
                StatusText,
            ));
            p.spawn((
                Text::new("Hero: —"),
                TextFont { font_size: 15.0, ..default() },
                TextColor(COLOR_LOCAL_HERO),
                HeroNameText,
            ));
            p.spawn((
                Text::new("Pos: (0, 0)"),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
                PositionText,
            ));
            p.spawn((
                Text::new(""),
                TextFont { font_size: 15.0, ..default() },
                TextColor(Color::srgb(0.6, 0.8, 0.6)),
                OriginText,
            ));
        });
}

fn on_connected(
    mut events: EventReader<StdbConnectedEvent>,
    conn: Res<StdbConnection<DbConnection>>,
    mut local_hero: ResMut<LocalHero>,
    mut status_q: Query<&mut Text, With<StatusText>>,
) {
    for _ in events.read() {
        info!("Connected to SpacetimeDB! Identity: {:?}", conn.try_identity());

        conn.subscribe()
            .on_applied(|_ctx| info!("Hero subscription applied"))
            .on_error(|_ctx, err| error!("Subscription error: {err}"))
            .subscribe(["SELECT * FROM hero"]);

        if let Ok(mut text) = status_q.get_single_mut() {
            **text = "Status: Connected".into();
        }

        if !local_hero.hero_requested {
            local_hero.hero_requested = true;
            info!("Requesting create_hero...");
            let _ = conn.reducers().create_hero("Hero".to_string());
        }
    }
}

fn on_hero_inserted(
    mut events: EventReader<InsertEvent<Hero>>,
    conn: Res<StdbConnection<DbConnection>>,
    mut local_hero: ResMut<LocalHero>,
    mut commands: Commands,
    mut name_q: Query<&mut Text, With<HeroNameText>>,
) {
    let my_identity = conn.try_identity();

    for event in events.read() {
        let hero = &event.row;
        info!("Hero insert: id={} name={} owner={:?}", hero.id, hero.name, hero.player_identity);

        let is_local = Some(hero.player_identity) == my_identity;
        let color = if is_local { COLOR_LOCAL_HERO } else { COLOR_REMOTE_HERO };

        let mut entity = commands.spawn((
            Sprite {
                color,
                custom_size: Some(Vec2::splat(HERO_SIZE)),
                ..default()
            },
            Transform::from_xyz(hero.x, hero.y, 1.0),
            HeroEntity { id: hero.id },
        ));

        if is_local {
            entity.insert(IsLocalPlayer);

            // Check if this is a newly created hero (at origin position)
            let is_new = hero.x == 0.0 && hero.y == 0.0;

            local_hero.id = Some(hero.id);
            local_hero.name = hero.name.clone();
            local_hero.is_new_hero = is_new;

            if let Ok(mut text) = name_q.get_single_mut() {
                **text = format!("Hero: {}", hero.name);
            }

            info!("Local hero spawned: {} (id={}, is_new={})", hero.name, hero.id, is_new);
        }
    }
}

fn on_hero_deleted(
    mut events: EventReader<DeleteEvent<Hero>>,
    mut commands: Commands,
    query: Query<(Entity, &HeroEntity)>,
) {
    for event in events.read() {
        for (entity, he) in query.iter() {
            if he.id == event.row.id {
                commands.entity(entity).despawn();
                info!("Hero despawned: id={}", event.row.id);
            }
        }
    }
}

fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    opening_state: Res<OpeningEventState>,
    mut query: Query<&mut Transform, With<IsLocalPlayer>>,
) {
    // Block input during opening event
    if !opening_state.seen_opening {
        return;
    }

    let Ok(mut transform) = query.get_single_mut() else { return };

    let mut dir = Vec2::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp)    { dir.y += 1.0; }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown)  { dir.y -= 1.0; }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)  { dir.x -= 1.0; }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) { dir.x += 1.0; }

    if dir != Vec2::ZERO {
        let delta = dir.normalize() * MOVE_SPEED * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
    }
}

fn sync_position(
    time: Res<Time>,
    mut timer: ResMut<SyncTimer>,
    conn: Res<StdbConnection<DbConnection>>,
    local_hero: Res<LocalHero>,
    query: Query<&Transform, With<IsLocalPlayer>>,
) {
    if !timer.0.tick(time.delta()).just_finished() { return }
    let Ok(transform) = query.get_single() else { return };
    let Some(id) = local_hero.id else { return };

    let x = transform.translation.x;
    let y = transform.translation.y;
    let _ = conn.reducers().move_hero(id, x, y);
}

fn smooth_camera_follow(
    time: Res<Time>,
    hero_q: Query<&Transform, (With<IsLocalPlayer>, Without<Camera2d>)>,
    mut camera_q: Query<&mut Transform, With<Camera2d>>,
) {
    let Ok(hero_transform) = hero_q.get_single() else { return };
    let Ok(mut camera_transform) = camera_q.get_single_mut() else { return };

    let target = Vec3::new(
        hero_transform.translation.x,
        hero_transform.translation.y,
        camera_transform.translation.z,
    );

    let lerp_factor = CAMERA_LERP_SPEED * time.delta_secs();
    camera_transform.translation = camera_transform.translation.lerp(target, lerp_factor.min(1.0));
}

fn update_hud(
    local_hero: Res<LocalHero>,
    opening_state: Res<OpeningEventState>,
    mut pos_q: Query<&mut Text, (With<PositionText>, Without<OriginText>)>,
    mut origin_q: Query<&mut Text, (With<OriginText>, Without<PositionText>)>,
    hero_q: Query<&Transform, With<IsLocalPlayer>>,
) {
    if local_hero.id.is_none() { return }
    let Ok(transform) = hero_q.get_single() else { return };

    if let Ok(mut text) = pos_q.get_single_mut() {
        **text = format!("Pos: ({:.0}, {:.0})", transform.translation.x, transform.translation.y);
    }

    if let Ok(mut text) = origin_q.get_single_mut() {
        if let Some(origin) = opening_state.origin {
            **text = format!("Origin: {} | Fame: {}", origin.as_str(), opening_state.fame_local);
        }
    }
}

fn check_trigger_opening(
    local_hero: Res<LocalHero>,
    opening_state: Res<OpeningEventState>,
    mut trigger_events: EventWriter<TriggerOpeningEvent>,
) {
    // Trigger opening event for new heroes that haven't seen it
    if local_hero.id.is_some() && local_hero.is_new_hero && !opening_state.seen_opening {
        trigger_events.send(TriggerOpeningEvent);
    }
}

fn on_opening_complete(
    mut events: EventReader<OpeningEventComplete>,
) {
    for event in events.read() {
        info!(
            "Opening event complete! Origin: {:?}, Fame gained: {}",
            event.origin, event.fame_gained
        );
    }
}

fn update_grid(
    mut commands: Commands,
    camera_q: Query<&Transform, With<Camera2d>>,
    mut spawned: ResMut<SpawnedTiles>,
    tiles_q: Query<(Entity, &GridTile)>,
) {
    let Ok(cam_tf) = camera_q.get_single() else { return };

    let cam_tx = (cam_tf.translation.x / TILE_SIZE).round() as i32;
    let cam_ty = (cam_tf.translation.y / TILE_SIZE).round() as i32;

    // Collect tiles that should be visible
    let mut needed: HashSet<(i32, i32)> = HashSet::new();
    for dx in -TILE_RENDER_RADIUS..=TILE_RENDER_RADIUS {
        for dy in -TILE_RENDER_RADIUS..=TILE_RENDER_RADIUS {
            needed.insert((cam_tx + dx, cam_ty + dy));
        }
    }

    // Despawn tiles out of range
    for (entity, tile) in tiles_q.iter() {
        let pos = (tile.tx, tile.ty);
        if !needed.contains(&pos) {
            commands.entity(entity).despawn();
            spawned.0.remove(&pos);
        }
    }

    // Spawn new tiles in range
    for &(tx, ty) in &needed {
        if spawned.0.contains(&(tx, ty)) {
            continue;
        }

        let color = match tile_type(tx, ty) {
            TileType::Grass => Color::srgb(0.08, 0.16, 0.08),
            TileType::Dirt => Color::srgb(0.18, 0.11, 0.05),
        };

        let world_x = tx as f32 * TILE_SIZE;
        let world_y = ty as f32 * TILE_SIZE;

        commands.spawn((
            Sprite {
                color,
                custom_size: Some(Vec2::splat(TILE_SIZE - 1.0)),
                ..default()
            },
            Transform::from_xyz(world_x, world_y, 0.0),
            GridTile { tx, ty },
        ));

        spawned.0.insert((tx, ty));
    }
}
