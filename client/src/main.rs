mod module_bindings;

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
use std::collections::HashSet;

const MOVE_SPEED: f32 = 8.0;
const SYNC_INTERVAL_SECS: f32 = 0.1;
const CAMERA_LERP_SPEED: f32 = 5.0;

const TILE_SIZE: f32 = 2.0;
const TILE_RENDER_RADIUS: i32 = 20;

const COLOR_LOCAL_HERO: Color = Color::srgb(1.0, 0.6, 0.2);
const COLOR_AMBER: Color = Color::srgb(1.0, 0.75, 0.2);

// ─── App State ───────────────────────────────────────────────────────────────

#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
enum AppState {
    #[default]
    NameEntry,
    Connecting,
    Playing,
}

// ─── Grid ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq, Eq)]
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

fn tile_hash(tx: i32, ty: i32, seed: u32) -> u32 {
    ((tx.wrapping_mul(73856093) ^ ty.wrapping_mul(19349663)) as u32).wrapping_add(seed)
}

#[derive(Component)]
struct GridTile {
    tx: i32,
    ty: i32,
}

#[derive(Component)]
struct TileProp;

#[derive(Resource, Default)]
struct SpawnedTiles(HashSet<(i32, i32)>);

// ─── Resources ───────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct PlayerName(String);

#[derive(Resource, Default)]
struct LocalHero {
    id: Option<u64>,
    name: String,
    hero_requested: bool,
    is_new_hero: bool,
    origin: Option<HeroOrigin>,
    seen_opening: bool,
    fame_local: i32,
}

#[derive(Resource)]
struct SyncTimer(Timer);

impl Default for SyncTimer {
    fn default() -> Self {
        Self(Timer::from_seconds(SYNC_INTERVAL_SECS, TimerMode::Repeating))
    }
}

#[derive(Resource, Default)]
struct CursorBlink(f32);

// ─── Hero Origin ─────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeroOrigin {
    Leader,
    Defender,
    Wanderer,
    Survivor,
}

impl HeroOrigin {
    fn as_str(&self) -> &'static str {
        match self {
            HeroOrigin::Leader => "leader",
            HeroOrigin::Defender => "defender",
            HeroOrigin::Wanderer => "wanderer",
            HeroOrigin::Survivor => "survivor",
        }
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
struct MainCamera;

#[derive(Component)]
struct StatusText;

#[derive(Component)]
struct HeroNameText;

#[derive(Component)]
struct PositionText;

#[derive(Component)]
struct OriginText;

#[derive(Component)]
struct NameEntryUI;

#[derive(Component)]
struct NameInputDisplay;

#[derive(Component)]
struct OpeningEventUI;

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    App::new()
        .add_plugins(DefaultPlugins
        .set(AssetPlugin {
            file_path: "client/assets".to_string(),
            ..default()
        })
        .set(WindowPlugin {
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
        .insert_resource(ClearColor(Color::srgb(0.4, 0.6, 0.8)))
        .init_state::<AppState>()
        .init_resource::<PlayerName>()
        .init_resource::<LocalHero>()
        .init_resource::<SyncTimer>()
        .init_resource::<SpawnedTiles>()
        .init_resource::<CursorBlink>()
        .add_systems(Startup, setup)
        .add_systems(OnEnter(AppState::NameEntry), spawn_name_entry_ui)
        .add_systems(OnEnter(AppState::Connecting), on_enter_connecting)
        .add_systems(
            Update,
            handle_name_input.run_if(in_state(AppState::NameEntry)),
        )
        .add_systems(
            Update,
            request_hero.run_if(in_state(AppState::Connecting)),
        )
        .add_systems(
            Update,
            (
                on_connected,
                on_hero_inserted,
                on_hero_deleted,
                update_hud,
                update_grid,
            ),
        )
        .add_systems(
            Update,
            (
                handle_input,
                sync_position,
                smooth_camera_follow,
            ).run_if(in_state(AppState::Playing)),
        )
        .add_systems(
            Update,
            (
                check_show_opening,
                handle_opening_choice,
            ),
        )
        .run();
}

// ─── Setup ───────────────────────────────────────────────────────────────────

fn setup(mut commands: Commands) {
    // 3D Camera - isometric style
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 12.0, 12.0).looking_at(Vec3::ZERO, Vec3::Y),
        Projection::Perspective(PerspectiveProjection {
            fov: 45.0_f32.to_radians(),
            ..default()
        }),
        MainCamera,
    ));

    // Ambient light
    commands.insert_resource(AmbientLight {
        color: Color::WHITE,
        brightness: 400.0,
    });

    // Directional light (sun)
    commands.spawn((
        DirectionalLight {
            color: Color::WHITE,
            illuminance: 10000.0,
            shadows_enabled: true,
            ..default()
        },
        Transform::from_xyz(4.0, 8.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // HUD (hidden until connected)
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
                Text::new("Status: —"),
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

// ─── Name Entry ──────────────────────────────────────────────────────────────

fn spawn_name_entry_ui(mut commands: Commands) {
    // Dark overlay
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.95)),
        GlobalTransform::from_translation(Vec3::new(0.0, 0.0, 10.0)),
        NameEntryUI,
    )).with_children(|parent| {
        // Centered panel
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(40.0)),
                ..default()
            },
            NameEntryUI,
        )).with_children(|panel| {
            // Title
            panel.spawn((
                Text::new("WHAT MAY BECOME"),
                TextFont { font_size: 48.0, ..default() },
                TextColor(COLOR_AMBER),
                Node { margin: UiRect::bottom(Val::Px(20.0)), ..default() },
                NameEntryUI,
            ));

            // Subtitle
            panel.spawn((
                Text::new("Enter your name, traveler."),
                TextFont { font_size: 20.0, ..default() },
                TextColor(Color::WHITE),
                Node { margin: UiRect::bottom(Val::Px(30.0)), ..default() },
                NameEntryUI,
            ));

            // Input box
            panel.spawn((
                Node {
                    padding: UiRect::axes(Val::Px(20.0), Val::Px(10.0)),
                    border: UiRect::all(Val::Px(2.0)),
                    min_width: Val::Px(300.0),
                    justify_content: JustifyContent::Center,
                    margin: UiRect::bottom(Val::Px(20.0)),
                    ..default()
                },
                BorderColor(COLOR_AMBER),
                BackgroundColor(Color::srgba(0.1, 0.1, 0.1, 0.8)),
                NameEntryUI,
            )).with_children(|input_box| {
                input_box.spawn((
                    Text::new("_"),
                    TextFont { font_size: 24.0, ..default() },
                    TextColor(Color::WHITE),
                    NameInputDisplay,
                    NameEntryUI,
                ));
            });

            // Instructions
            panel.spawn((
                Text::new("Press ENTER to begin"),
                TextFont { font_size: 14.0, ..default() },
                TextColor(Color::srgb(0.5, 0.5, 0.5)),
                NameEntryUI,
            ));
        });
    });
}

fn handle_name_input(
    mut player_name: ResMut<PlayerName>,
    mut cursor_blink: ResMut<CursorBlink>,
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    mut display_q: Query<&mut Text, With<NameInputDisplay>>,
    mut next_state: ResMut<NextState<AppState>>,
    name_entry_q: Query<Entity, With<NameEntryUI>>,
    mut commands: Commands,
) {
    // Update cursor blink
    cursor_blink.0 += time.delta_secs();
    let show_cursor = (cursor_blink.0 * 2.0) as i32 % 2 == 0;

    // Handle character input via KeyCode mapping
    let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);

    // Letters A-Z
    let letter_keys = [
        (KeyCode::KeyA, 'a'), (KeyCode::KeyB, 'b'), (KeyCode::KeyC, 'c'),
        (KeyCode::KeyD, 'd'), (KeyCode::KeyE, 'e'), (KeyCode::KeyF, 'f'),
        (KeyCode::KeyG, 'g'), (KeyCode::KeyH, 'h'), (KeyCode::KeyI, 'i'),
        (KeyCode::KeyJ, 'j'), (KeyCode::KeyK, 'k'), (KeyCode::KeyL, 'l'),
        (KeyCode::KeyM, 'm'), (KeyCode::KeyN, 'n'), (KeyCode::KeyO, 'o'),
        (KeyCode::KeyP, 'p'), (KeyCode::KeyQ, 'q'), (KeyCode::KeyR, 'r'),
        (KeyCode::KeyS, 's'), (KeyCode::KeyT, 't'), (KeyCode::KeyU, 'u'),
        (KeyCode::KeyV, 'v'), (KeyCode::KeyW, 'w'), (KeyCode::KeyX, 'x'),
        (KeyCode::KeyY, 'y'), (KeyCode::KeyZ, 'z'),
    ];

    for (key, c) in letter_keys {
        if keyboard.just_pressed(key) && player_name.0.len() < 20 {
            if shift {
                player_name.0.push(c.to_ascii_uppercase());
            } else {
                player_name.0.push(c);
            }
        }
    }

    // Numbers 0-9
    let digit_keys = [
        (KeyCode::Digit0, '0'), (KeyCode::Digit1, '1'), (KeyCode::Digit2, '2'),
        (KeyCode::Digit3, '3'), (KeyCode::Digit4, '4'), (KeyCode::Digit5, '5'),
        (KeyCode::Digit6, '6'), (KeyCode::Digit7, '7'), (KeyCode::Digit8, '8'),
        (KeyCode::Digit9, '9'),
    ];

    for (key, c) in digit_keys {
        if keyboard.just_pressed(key) && player_name.0.len() < 20 {
            player_name.0.push(c);
        }
    }

    // Space
    if keyboard.just_pressed(KeyCode::Space) && player_name.0.len() < 20 {
        player_name.0.push(' ');
    }

    // Handle backspace
    if keyboard.just_pressed(KeyCode::Backspace) {
        player_name.0.pop();
    }

    // Handle enter
    if keyboard.just_pressed(KeyCode::Enter) && player_name.0.len() >= 2 {
        // Despawn name entry UI
        for entity in name_entry_q.iter() {
            commands.entity(entity).despawn_recursive();
        }
        next_state.set(AppState::Connecting);
        return;
    }

    // Update display
    if let Ok(mut text) = display_q.get_single_mut() {
        let cursor = if show_cursor { "_" } else { "" };
        **text = format!("{}{}", player_name.0, cursor);
        if player_name.0.is_empty() && show_cursor {
            **text = "_".to_string();
        }
    }
}

// ─── Connection ──────────────────────────────────────────────────────────────

fn on_enter_connecting(mut status_q: Query<&mut Text, With<StatusText>>) {
    if let Ok(mut text) = status_q.get_single_mut() {
        **text = "Status: Connecting...".into();
    }
}

fn request_hero(
    conn: Option<Res<StdbConnection<DbConnection>>>,
    player_name: Res<PlayerName>,
    mut local_hero: ResMut<LocalHero>,
) {
    let Some(conn) = conn else { return };
    if local_hero.hero_requested { return; }
    if !conn.is_active() { return; }

    local_hero.hero_requested = true;
    let name = if player_name.0.is_empty() { "Hero".to_string() } else { player_name.0.clone() };
    info!("Requesting create_hero with name: {}", name);
    let _ = conn.reducers().create_hero(name);
}

fn on_connected(
    mut events: EventReader<StdbConnectedEvent>,
    conn: Option<Res<StdbConnection<DbConnection>>>,
    mut status_q: Query<&mut Text, With<StatusText>>,
) {
    let Some(conn) = conn else { return };

    for _ in events.read() {
        info!("Connected to SpacetimeDB! Identity: {:?}", conn.try_identity());

        conn.subscribe()
            .on_applied(|_ctx| info!("Hero subscription applied"))
            .on_error(|_ctx, err| error!("Subscription error: {err}"))
            .subscribe(["SELECT * FROM hero"]);

        if let Ok(mut text) = status_q.get_single_mut() {
            **text = "Status: Connected".into();
        }
    }
}

fn on_hero_inserted(
    mut events: EventReader<InsertEvent<Hero>>,
    conn: Option<Res<StdbConnection<DbConnection>>>,
    mut local_hero: ResMut<LocalHero>,
    mut commands: Commands,
    mut name_q: Query<&mut Text, With<HeroNameText>>,
    mut next_state: ResMut<NextState<AppState>>,
    asset_server: Res<AssetServer>,
) {
    let Some(conn) = conn else { return };
    let my_identity = conn.try_identity();

    for event in events.read() {
        let hero = &event.row;
        info!("Hero insert: id={} name={} owner={:?}", hero.id, hero.name, hero.player_identity);

        let is_local = Some(hero.player_identity) == my_identity;

        // Map SpacetimeDB (x, y) to world (x, 0, z)
        let world_x = hero.x;
        let world_z = hero.y;

        // Choose model based on local/remote
        let model_path = if is_local {
            "models/blade.glb#Scene0"
        } else {
            "models/banner-red.glb#Scene0"
        };

        let mut entity = commands.spawn((
            SceneRoot(asset_server.load(model_path)),
            Transform::from_xyz(world_x, 0.0, world_z)
                .with_scale(Vec3::splat(0.8)),
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

            // If not a new hero, mark opening as seen
            if !is_new {
                local_hero.seen_opening = true;
            }

            info!("Local hero spawned: {} (id={}, is_new={})", hero.name, hero.id, is_new);

            // Transition to Playing state (opening event will show if needed)
            next_state.set(AppState::Playing);
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

// ─── Opening Event ───────────────────────────────────────────────────────────

fn check_show_opening(
    local_hero: Res<LocalHero>,
    opening_q: Query<Entity, With<OpeningEventUI>>,
    mut commands: Commands,
) {
    // Only show if hero exists, is new, hasn't seen opening, and UI not already spawned
    if local_hero.id.is_some()
        && local_hero.is_new_hero
        && !local_hero.seen_opening
        && local_hero.origin.is_none()
        && opening_q.is_empty()
    {
        spawn_opening_event(&mut commands);
    }
}

fn spawn_opening_event(commands: &mut Commands) {
    // Full-screen overlay
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(40.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.85)),
        GlobalTransform::from_translation(Vec3::new(0.0, 0.0, 5.0)),
        OpeningEventUI,
    )).with_children(|parent| {
        // Top flavor text
        parent.spawn((
            Text::new("The year is uncertain. The realm stirs with unease.\nYou wake to screaming. Your village is under attack."),
            TextFont { font_size: 20.0, ..default() },
            TextColor(Color::srgb(0.9, 0.9, 0.9)),
            Node {
                margin: UiRect::bottom(Val::Px(40.0)),
                ..default()
            },
            OpeningEventUI,
        ));

        // Choice panel
        parent.spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(30.0)),
                border: UiRect::all(Val::Px(3.0)),
                ..default()
            },
            BorderColor(COLOR_AMBER),
            BackgroundColor(Color::srgba(0.08, 0.06, 0.04, 0.95)),
            OpeningEventUI,
        )).with_children(|panel| {
            // Panel title
            panel.spawn((
                Text::new("YOUR TOWN IS UNDER ATTACK"),
                TextFont { font_size: 28.0, ..default() },
                TextColor(Color::srgb(0.9, 0.4, 0.2)),
                Node { margin: UiRect::bottom(Val::Px(25.0)), ..default() },
                OpeningEventUI,
            ));

            // Choice buttons
            spawn_opening_choice(panel, "[L]", "LEAD", "Rally your people. Face the threat head-on.");
            spawn_opening_choice(panel, "[D]", "DEFEND", "Hold the gates. Protect those who cannot fight.");
            spawn_opening_choice(panel, "[R]", "RUN", "Flee into the forest. Live to fight another day.");
            spawn_opening_choice(panel, "[C]", "COWER", "Hide and pray. Survive, but earn nothing.");
        });
    });
}

fn spawn_opening_choice(parent: &mut ChildBuilder, key: &str, action: &str, description: &str) {
    parent.spawn((
        Node {
            margin: UiRect::bottom(Val::Px(12.0)),
            ..default()
        },
        OpeningEventUI,
    )).with_children(|row| {
        // Key hint
        row.spawn((
            Text::new(format!("{} ", key)),
            TextFont { font_size: 18.0, ..default() },
            TextColor(Color::srgb(0.6, 0.6, 0.6)),
            OpeningEventUI,
        ));
        // Action word
        row.spawn((
            Text::new(format!("{:<8}", action)),
            TextFont { font_size: 18.0, ..default() },
            TextColor(COLOR_AMBER),
            OpeningEventUI,
        ));
        // Description
        row.spawn((
            Text::new(format!("— {}", description)),
            TextFont { font_size: 16.0, ..default() },
            TextColor(Color::srgb(0.75, 0.72, 0.65)),
            OpeningEventUI,
        ));
    });
}

fn handle_opening_choice(
    mut local_hero: ResMut<LocalHero>,
    keyboard: Res<ButtonInput<KeyCode>>,
    opening_q: Query<Entity, With<OpeningEventUI>>,
    mut commands: Commands,
) {
    // Only handle if we have a hero that hasn't seen the opening
    if local_hero.id.is_none() || local_hero.seen_opening {
        return;
    }

    let choice = if keyboard.just_pressed(KeyCode::KeyL) {
        Some((HeroOrigin::Leader, 10))
    } else if keyboard.just_pressed(KeyCode::KeyD) {
        Some((HeroOrigin::Defender, 5))
    } else if keyboard.just_pressed(KeyCode::KeyR) {
        Some((HeroOrigin::Wanderer, 0))
    } else if keyboard.just_pressed(KeyCode::KeyC) {
        Some((HeroOrigin::Survivor, 0))
    } else {
        None
    };

    if let Some((origin, fame)) = choice {
        local_hero.origin = Some(origin);
        local_hero.fame_local += fame;
        local_hero.seen_opening = true;

        // Despawn opening event UI
        for entity in opening_q.iter() {
            commands.entity(entity).despawn_recursive();
        }

        info!("Opening choice made: {:?}, fame gained: {}", origin, fame);
    }
}

// ─── Gameplay Systems ────────────────────────────────────────────────────────

fn handle_input(
    keyboard: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    local_hero: Res<LocalHero>,
    mut query: Query<&mut Transform, With<IsLocalPlayer>>,
) {
    // Block input during opening event
    if !local_hero.seen_opening {
        return;
    }

    let Ok(mut transform) = query.get_single_mut() else { return };

    // Movement in XZ plane
    let mut dir = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) || keyboard.pressed(KeyCode::ArrowUp)    { dir.z -= 1.0; }
    if keyboard.pressed(KeyCode::KeyS) || keyboard.pressed(KeyCode::ArrowDown)  { dir.z += 1.0; }
    if keyboard.pressed(KeyCode::KeyA) || keyboard.pressed(KeyCode::ArrowLeft)  { dir.x -= 1.0; }
    if keyboard.pressed(KeyCode::KeyD) || keyboard.pressed(KeyCode::ArrowRight) { dir.x += 1.0; }

    if dir != Vec3::ZERO {
        let delta = dir.normalize() * MOVE_SPEED * time.delta_secs();
        transform.translation.x += delta.x;
        transform.translation.z += delta.z;
    }
}

fn sync_position(
    time: Res<Time>,
    mut timer: ResMut<SyncTimer>,
    conn: Option<Res<StdbConnection<DbConnection>>>,
    local_hero: Res<LocalHero>,
    query: Query<&Transform, With<IsLocalPlayer>>,
) {
    if !timer.0.tick(time.delta()).just_finished() { return }
    let Some(conn) = conn else { return };
    let Ok(transform) = query.get_single() else { return };
    let Some(id) = local_hero.id else { return };

    // Map world (x, z) to SpacetimeDB (x, y)
    let x = transform.translation.x;
    let y = transform.translation.z;
    let _ = conn.reducers().move_hero(id, x, y);
}

fn smooth_camera_follow(
    time: Res<Time>,
    hero_q: Query<&Transform, (With<IsLocalPlayer>, Without<MainCamera>)>,
    mut camera_q: Query<&mut Transform, With<MainCamera>>,
) {
    let Ok(hero_transform) = hero_q.get_single() else { return };
    let Ok(mut camera_transform) = camera_q.get_single_mut() else { return };

    // Follow hero in XZ, keep Y fixed at 12.0
    let target = Vec3::new(
        hero_transform.translation.x,
        12.0,
        hero_transform.translation.z + 12.0, // Offset Z to keep isometric view
    );

    let lerp_factor = CAMERA_LERP_SPEED * time.delta_secs();
    camera_transform.translation = camera_transform.translation.lerp(target, lerp_factor.min(1.0));

    // Keep looking at hero position
    let look_target = Vec3::new(
        hero_transform.translation.x,
        0.0,
        hero_transform.translation.z,
    );
    camera_transform.look_at(look_target, Vec3::Y);
}

fn update_hud(
    local_hero: Res<LocalHero>,
    mut pos_q: Query<&mut Text, (With<PositionText>, Without<OriginText>, Without<HeroNameText>)>,
    mut origin_q: Query<&mut Text, (With<OriginText>, Without<PositionText>, Without<HeroNameText>)>,
    mut name_q: Query<&mut Text, (With<HeroNameText>, Without<PositionText>, Without<OriginText>)>,
    hero_q: Query<&Transform, With<IsLocalPlayer>>,
) {
    if local_hero.id.is_none() { return }

    // Update hero name
    if let Ok(mut text) = name_q.get_single_mut() {
        **text = format!("Hero: {}", local_hero.name);
    }

    // Update position (show x, z as x, y for user)
    if let Ok(transform) = hero_q.get_single() {
        if let Ok(mut text) = pos_q.get_single_mut() {
            **text = format!("Pos: ({:.0}, {:.0})", transform.translation.x, transform.translation.z);
        }
    }

    // Update origin/fame
    if let Ok(mut text) = origin_q.get_single_mut() {
        if let Some(origin) = local_hero.origin {
            **text = format!("Origin: {} | Fame: {}", origin.as_str(), local_hero.fame_local);
        }
    }
}

fn update_grid(
    mut commands: Commands,
    camera_q: Query<&Transform, With<MainCamera>>,
    mut spawned: ResMut<SpawnedTiles>,
    tiles_q: Query<(Entity, &GridTile)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
) {
    let Ok(cam_tf) = camera_q.get_single() else { return };

    // Get camera look-at point (approximate center of view)
    let cam_center_x = cam_tf.translation.x;
    let cam_center_z = cam_tf.translation.z - 12.0; // Account for camera offset

    let cam_tx = (cam_center_x / TILE_SIZE).round() as i32;
    let cam_tz = (cam_center_z / TILE_SIZE).round() as i32;

    // Collect tiles that should be visible
    let mut needed: HashSet<(i32, i32)> = HashSet::new();
    for dx in -TILE_RENDER_RADIUS..=TILE_RENDER_RADIUS {
        for dz in -TILE_RENDER_RADIUS..=TILE_RENDER_RADIUS {
            needed.insert((cam_tx + dx, cam_tz + dz));
        }
    }

    // Despawn tiles out of range
    for (entity, tile) in tiles_q.iter() {
        let pos = (tile.tx, tile.ty);
        if !needed.contains(&pos) {
            commands.entity(entity).despawn_recursive();
            spawned.0.remove(&pos);
        }
    }

    // Create tile mesh (flat box)
    let tile_mesh = meshes.add(Cuboid::new(1.9, 0.1, 1.9));

    // Materials for grass and dirt
    let grass_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.25, 0.10),
        perceptual_roughness: 0.9,
        ..default()
    });
    let dirt_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.18, 0.08),
        perceptual_roughness: 0.95,
        ..default()
    });

    // Spawn new tiles in range
    for &(tx, tz) in &needed {
        if spawned.0.contains(&(tx, tz)) {
            continue;
        }

        let tt = tile_type(tx, tz);
        let material = match tt {
            TileType::Grass => grass_material.clone(),
            TileType::Dirt => dirt_material.clone(),
        };

        let world_x = tx as f32 * TILE_SIZE;
        let world_z = tz as f32 * TILE_SIZE;

        // Spawn tile mesh
        commands.spawn((
            Mesh3d(tile_mesh.clone()),
            MeshMaterial3d(material),
            Transform::from_xyz(world_x, -0.05, world_z),
            GridTile { tx, ty: tz },
        )).with_children(|parent| {
            // Optionally spawn a prop
            let prop_hash = tile_hash(tx, tz, 12345);
            let prop_chance = prop_hash % 100;

            // Determine rotation (0, 90, 180, or 270 degrees)
            let rotation_index = tile_hash(tx, tz, 67890) % 4;
            let rotation = Quat::from_rotation_y((rotation_index as f32) * std::f32::consts::FRAC_PI_2);

            let prop_path: Option<&str> = match tt {
                TileType::Dirt => {
                    if prop_chance < 5 {
                        Some("models/road.glb#Scene0")
                    } else if prop_chance < 10 {
                        Some("models/road-corner.glb#Scene0")
                    } else {
                        None
                    }
                }
                TileType::Grass => {
                    if prop_chance < 3 {
                        Some("models/tree.glb#Scene0")
                    } else if prop_chance < 5 {
                        Some("models/rock-small.glb#Scene0")
                    } else if prop_chance < 6 {
                        Some("models/tree-high.glb#Scene0")
                    } else {
                        None
                    }
                }
            };

            if let Some(path) = prop_path {
                parent.spawn((
                    SceneRoot(asset_server.load(path)),
                    Transform::from_xyz(0.0, 0.05, 0.0)
                        .with_rotation(rotation)
                        .with_scale(Vec3::splat(0.5)),
                    TileProp,
                ));
            }
        });

        spawned.0.insert((tx, tz));
    }
}
