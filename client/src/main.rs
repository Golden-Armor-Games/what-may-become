use bevy::prelude::*;

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
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // Spawn 2D camera
    commands.spawn(Camera2d);

    // TODO: Initialize SpacetimeDB connection
    // - Connect to local or remote SpacetimeDB instance
    // - Subscribe to Player, Hero, Town, Keep tables
    // - Handle connection state changes

    // TODO: Spawn player entity
    // - Create player sprite/mesh
    // - Attach components for movement, health, etc.
    // - Sync position with SpacetimeDB Hero table

    // TODO: Input handling
    // - WASD/Arrow keys for movement
    // - Mouse click for target selection
    // - Keyboard shortcuts for abilities/actions

    info!("What May Become - Client initialized");
}

// TODO: SpacetimeDB connection system
// fn connect_to_server(/* ... */) {
//     // Use bevy_spacetimedb to establish connection
//     // Handle authentication
//     // Set up table subscriptions
// }

// TODO: Player movement system
// fn handle_movement(/* ... */) {
//     // Read input
//     // Update local position
//     // Call move_hero reducer
// }

// TODO: Sync system
// fn sync_entities(/* ... */) {
//     // Receive updates from SpacetimeDB
//     // Update local entity transforms
//     // Handle new/removed entities
// }
