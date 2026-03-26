mod module_bindings;

use bevy::prelude::*;
use bevy_spacetimedb::{
    DeleteEvent, InsertEvent, StdbConnectedEvent, StdbConnection,
    StdbConnectionErrorEvent, StdbDisconnectedEvent, StdbPlugin,
};
use module_bindings::{
    create_hero_reducer::create_hero, hero_table::HeroTableAccess,
    move_hero_reducer::move_hero, DbConnection, Hero as DbHero,
};
use spacetimedb_sdk::Table;
use spacetimedb_sdk::Identity;
use std::sync::mpsc::Sender;
use std::time::Duration;

// =============================================================================
// Constants
// =============================================================================

const HERO_SIZE: f32 = 32.0;
const MOVE_SPEED: f32 = 200.0;
const SYNC_INTERVAL_MS: u64 = 100; // ~10x per second
const POSITION_THRESHOLD: f32 = 1.0;

const COLOR_BACKGROUND: Color = Color::srgb(0.102, 0.102, 0.18); // #1a1a2e
const COLOR_LOCAL_HERO: Color = Color::srgb(1.0, 0.6, 0.2); // Glowing amber/orange
const COLOR_REMOTE_HERO: Color = Color::srgb(0.0, 0.8, 0.9); // Cyan

// =============================================================================
// App States
// =============================================================================

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    Connecting,
    Playing,
}

// =============================================================================
// Resources
// =============================================================================

#[derive(Resource, Default)]
struct LocalHero {
    id: Option<u64>,
    name: String,
}

#[derive(Resource)]
struct PositionSyncTimer {
    timer: Timer,
    last_synced_pos: Vec2,
}

impl Default for PositionSyncTimer {
    fn default() -> Self {
        Self {
            timer: Timer::new(Duration::from_millis(SYNC_INTERVAL_MS), TimerMode::Repeating),
            last_synced_pos: Vec2::ZERO,
        }
    }
}

#[derive(Resource, Default)]
struct ConnectionStatus {
    connected: bool,
    identity: Option<Identity>,
}

// =============================================================================
// Components
// =============================================================================

#[derive(Component)]
struct Hero {
    id: u64,
}

#[derive(Component)]
struct IsLocalPlayer;

#[derive(Component)]
struct RemotePlayer;

#[derive(Component)]
struct HudText;

#[derive(Component)]
struct ConnectionStatusText;

#[derive(Component)]
struct PositionText;

#[derive(Component)]
struct HeroNameText;

// =============================================================================
// SpacetimeDB Connection Setup
// =============================================================================

fn build_connection(
    send_connected: Sender<StdbConnectedEvent>,
    send_disconnected: Sender<StdbDisconnectedEvent>,
    send_error: Sender<StdbConnectionErrorEvent>,
    _app: &mut App,
) -> DbConnection {
    DbConnection::builder()
        .with_uri("ws://localhost:3000")
        .with_module_name("what-may-become")
        .on_connect(move |_ctx, _identity, _token| {
            send_connected.send(StdbConnectedEvent).unwrap();
        })
        .on_disconnect(move |_ctx, err| {
            send_disconnected
                .send(StdbDisconnectedEvent { err })
                .unwrap();
        })
        .on_connect_error(move |_ctx, err| {
            send_error
                .send(StdbConnectionErrorEvent { err: err.clone() })
                .unwrap();
        })
        .build()
        .expect("Failed to connect to SpacetimeDB")
}

fn register_events(
    plugin: &StdbPlugin<DbConnection>,
    app: &mut App,
    db: &<DbConnection as spacetimedb_sdk::DbContext>::DbView,
    _reducers: &<DbConnection as spacetimedb_sdk::DbContext>::Reducers,
) {
    plugin.on_insert(app, db.hero());
    plugin.on_delete(app, db.hero());
}

// =============================================================================
// Main
// =============================================================================

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "What May Become".into(),
                resolution: (1280., 720.).into(),
                canvas: Some("#game-canvas".into()),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(
            StdbPlugin::<DbConnection>::default()
                .with_connection(build_connection)
                .with_events(register_events),
        )
        .init_state::<AppState>()
        .init_resource::<LocalHero>()
        .init_resource::<PositionSyncTimer>()
        .init_resource::<ConnectionStatus>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            handle_connect.run_if(in_state(AppState::Connecting)),
        )
        .add_systems(
            Update,
            (
                handle_input,
                sync_position,
                spawn_remote_heroes,
                despawn_remote_heroes,
                update_camera,
                update_hud,
            )
                .run_if(in_state(AppState::Playing)),
        )
        .insert_resource(ClearColor(COLOR_BACKGROUND))
        .run();
}

// =============================================================================
// Systems
// =============================================================================

fn setup(mut commands: Commands) {
    // Spawn 2D camera
    commands.spawn(Camera2d);

    // Spawn HUD
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(10.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(5.0),
                ..default()
            },
            HudText,
        ))
        .with_children(|parent| {
            // Connection status
            parent.spawn((
                Text::new("Status: Connecting..."),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                ConnectionStatusText,
            ));

            // Hero name
            parent.spawn((
                Text::new("Hero: -"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(COLOR_LOCAL_HERO),
                HeroNameText,
            ));

            // Position
            parent.spawn((
                Text::new("Position: (0, 0)"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgb(0.7, 0.7, 0.7)),
                PositionText,
            ));
        });

    info!("What May Become - Client initialized");
}

fn handle_connect(
    mut commands: Commands,
    mut next_state: ResMut<NextState<AppState>>,
    mut local_hero: ResMut<LocalHero>,
    mut connection_status: ResMut<ConnectionStatus>,
    mut connected_events: EventReader<StdbConnectedEvent>,
    mut hero_inserts: EventReader<InsertEvent<DbHero>>,
    conn: Res<StdbConnection<DbConnection>>,
) {
    // Handle connection event
    for _event in connected_events.read() {
        connection_status.connected = true;
        connection_status.identity = conn.try_identity();
        info!("Connected to SpacetimeDB with identity: {:?}", connection_status.identity);
    }

    if !connection_status.connected {
        return;
    }

    // Check if we already have a hero
    let my_identity = connection_status.identity;

    // Check for hero inserts (either existing or newly created)
    for event in hero_inserts.read() {
        let hero = &event.row;
        if Some(hero.player_identity) == my_identity && hero.is_alive {
            local_hero.id = Some(hero.id);
            local_hero.name = hero.name.clone();

            // Spawn local player sprite
            commands.spawn((
                Sprite {
                    color: COLOR_LOCAL_HERO,
                    custom_size: Some(Vec2::splat(HERO_SIZE)),
                    ..default()
                },
                Transform::from_xyz(hero.x, hero.y, 1.0),
                Hero { id: hero.id },
                IsLocalPlayer,
            ));

            info!("Local hero spawned: {} (id: {})", hero.name, hero.id);
            next_state.set(AppState::Playing);
            return;
        }
    }

    // If connected but no hero yet, check existing heroes in DB
    if local_hero.id.is_none() {
        let existing_hero = conn
            .db()
            .hero()
            .iter()
            .find(|h| Some(h.player_identity) == my_identity && h.is_alive);

        if let Some(hero) = existing_hero {
            local_hero.id = Some(hero.id);
            local_hero.name = hero.name.clone();

            // Spawn local player sprite
            commands.spawn((
                Sprite {
                    color: COLOR_LOCAL_HERO,
                    custom_size: Some(Vec2::splat(HERO_SIZE)),
                    ..default()
                },
                Transform::from_xyz(hero.x, hero.y, 1.0),
                Hero { id: hero.id },
                IsLocalPlayer,
            ));

            info!("Existing hero found: {} (id: {})", hero.name, hero.id);
            next_state.set(AppState::Playing);
        } else {
            // Create a new hero
            let _ = conn.reducers().create_hero("Hero".to_string());
            info!("Creating new hero...");
        }
    }
}

fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut query: Query<&mut Transform, With<IsLocalPlayer>>,
) {
    let Ok(mut transform) = query.get_single_mut() else {
        return;
    };

    let mut direction = Vec2::ZERO;

    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp) {
        direction.y += 1.0;
    }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown) {
        direction.y -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft) {
        direction.x -= 1.0;
    }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) {
        direction.x += 1.0;
    }

    if direction != Vec2::ZERO {
        direction = direction.normalize();
        let delta = direction * MOVE_SPEED * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.y += delta.y;
    }
}

fn sync_position(
    time: Res<Time>,
    mut sync_timer: ResMut<PositionSyncTimer>,
    local_hero: Res<LocalHero>,
    conn: Res<StdbConnection<DbConnection>>,
    query: Query<&Transform, With<IsLocalPlayer>>,
) {
    sync_timer.timer.tick(time.delta());

    if !sync_timer.timer.just_finished() {
        return;
    }

    let Some(hero_id) = local_hero.id else {
        return;
    };

    let Ok(transform) = query.get_single() else {
        return;
    };

    let current_pos = Vec2::new(transform.translation.x, transform.translation.y);
    let distance = current_pos.distance(sync_timer.last_synced_pos);

    if distance > POSITION_THRESHOLD {
        let _ = conn.reducers().move_hero(hero_id, current_pos.x, current_pos.y);
        sync_timer.last_synced_pos = current_pos;
    }
}

fn spawn_remote_heroes(
    mut commands: Commands,
    local_hero: Res<LocalHero>,
    mut hero_inserts: EventReader<InsertEvent<DbHero>>,
    existing_heroes: Query<&Hero>,
) {
    for event in hero_inserts.read() {
        let hero = &event.row;

        // Skip local hero
        if Some(hero.id) == local_hero.id {
            continue;
        }

        // Skip if already spawned
        if existing_heroes.iter().any(|h| h.id == hero.id) {
            continue;
        }

        // Skip dead heroes
        if !hero.is_alive {
            continue;
        }

        // Spawn remote player
        commands.spawn((
            Sprite {
                color: COLOR_REMOTE_HERO,
                custom_size: Some(Vec2::splat(HERO_SIZE)),
                ..default()
            },
            Transform::from_xyz(hero.x, hero.y, 0.5),
            Hero { id: hero.id },
            RemotePlayer,
        ));

        info!("Remote hero spawned: {} (id: {})", hero.name, hero.id);
    }
}

fn despawn_remote_heroes(
    mut commands: Commands,
    mut hero_deletes: EventReader<DeleteEvent<DbHero>>,
    heroes: Query<(Entity, &Hero), With<RemotePlayer>>,
) {
    for event in hero_deletes.read() {
        let deleted_id = event.row.id;

        for (entity, hero) in heroes.iter() {
            if hero.id == deleted_id {
                commands.entity(entity).despawn();
                info!("Remote hero despawned (id: {})", deleted_id);
            }
        }
    }
}

fn update_camera(
    player_query: Query<&Transform, With<IsLocalPlayer>>,
    mut camera_query: Query<&mut Transform, (With<Camera2d>, Without<IsLocalPlayer>)>,
) {
    let Ok(player_transform) = player_query.get_single() else {
        return;
    };

    let Ok(mut camera_transform) = camera_query.get_single_mut() else {
        return;
    };

    camera_transform.translation.x = player_transform.translation.x;
    camera_transform.translation.y = player_transform.translation.y;
}

fn update_hud(
    connection_status: Res<ConnectionStatus>,
    local_hero: Res<LocalHero>,
    player_query: Query<&Transform, With<IsLocalPlayer>>,
    mut status_text: Query<&mut Text, (With<ConnectionStatusText>, Without<HeroNameText>, Without<PositionText>)>,
    mut name_text: Query<&mut Text, (With<HeroNameText>, Without<ConnectionStatusText>, Without<PositionText>)>,
    mut pos_text: Query<&mut Text, (With<PositionText>, Without<ConnectionStatusText>, Without<HeroNameText>)>,
) {
    // Update connection status
    if let Ok(mut text) = status_text.get_single_mut() {
        let status = if connection_status.connected {
            "Connected"
        } else {
            "Connecting..."
        };
        **text = format!("Status: {}", status);
    }

    // Update hero name
    if let Ok(mut text) = name_text.get_single_mut() {
        let name = if local_hero.name.is_empty() {
            "-"
        } else {
            &local_hero.name
        };
        **text = format!("Hero: {}", name);
    }

    // Update position
    if let Ok(transform) = player_query.get_single() {
        if let Ok(mut text) = pos_text.get_single_mut() {
            **text = format!(
                "Position: ({:.0}, {:.0})",
                transform.translation.x, transform.translation.y
            );
        }
    }
}
