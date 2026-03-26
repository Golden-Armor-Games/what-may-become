use spacetimedb::{Identity, ReducerContext, Table, Timestamp};

// =============================================================================
// Tables
// =============================================================================

/// Represents a connected player session
#[spacetimedb::table(name = player, public)]
pub struct Player {
    #[primary_key]
    pub identity: Identity,
    pub username: String,
    pub online: bool,
    pub last_seen: Timestamp,
}

/// A hero controlled by a player - the main playable character
#[spacetimedb::table(name = hero, public)]
pub struct Hero {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub player_identity: Identity,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub health: i32,
    pub max_health: i32,
    // Fame levels - progress from local nobody to godly legend
    pub fame_local: u32,
    pub fame_city: u32,
    pub fame_realm: u32,
    pub fame_godly: u32,
    pub is_alive: bool,
}

/// A town owned by a player - can be worked, defended, and upgraded
#[spacetimedb::table(name = town, public)]
pub struct Town {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner_identity: Identity,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub fortification_level: u32,
}

/// A strategic stronghold - limited number in the world, provides resource bonuses
#[spacetimedb::table(name = keep, public)]
pub struct Keep {
    #[primary_key]
    #[auto_inc]
    pub id: u64,
    pub owner_identity: Option<Identity>,
    pub name: String,
    pub x: f32,
    pub y: f32,
    pub fortification_level: u32,
    pub resource_bonus: f32,
}

// =============================================================================
// Reducers
// =============================================================================

/// Called when a client connects to the database
#[spacetimedb::reducer(client_connected)]
pub fn on_connect(ctx: &ReducerContext) {
    let identity = ctx.sender;

    // Check if player already exists
    if ctx.db.player().identity().find(identity).is_some() {
        // Update existing player to online
        if let Some(mut player) = ctx.db.player().identity().find(identity) {
            player.online = true;
            player.last_seen = ctx.timestamp;
            ctx.db.player().identity().update(player);
        }
    } else {
        // Create new player record
        ctx.db.player().insert(Player {
            identity,
            username: format!("Player_{}", &identity.to_hex()[..8]),
            online: true,
            last_seen: ctx.timestamp,
        });
    }

    log::info!("Player connected: {}", identity.to_hex());
}

/// Called when a client disconnects from the database
#[spacetimedb::reducer(client_disconnected)]
pub fn on_disconnect(ctx: &ReducerContext) {
    let identity = ctx.sender;

    if let Some(mut player) = ctx.db.player().identity().find(identity) {
        player.online = false;
        player.last_seen = ctx.timestamp;
        ctx.db.player().identity().update(player);
    }

    log::info!("Player disconnected: {}", identity.to_hex());
}

/// Create a new hero for the calling player
#[spacetimedb::reducer]
pub fn create_hero(ctx: &ReducerContext, name: String) -> Result<(), String> {
    let identity = ctx.sender;

    // Verify player exists
    if ctx.db.player().identity().find(identity).is_none() {
        return Err("Player not found. Connect first.".to_string());
    }

    // Check if player already has a living hero (permadeath - one hero at a time)
    let existing_hero = ctx
        .db
        .hero()
        .iter()
        .find(|h| h.player_identity == identity && h.is_alive);

    if existing_hero.is_some() {
        return Err("You already have a living hero. Permadeath rules apply.".to_string());
    }

    // Create the hero
    ctx.db.hero().insert(Hero {
        id: 0, // auto_inc will assign
        player_identity: identity,
        name: name.clone(),
        x: 0.0,
        y: 0.0,
        health: 100,
        max_health: 100,
        fame_local: 0,
        fame_city: 0,
        fame_realm: 0,
        fame_godly: 0,
        is_alive: true,
    });

    log::info!("Hero '{}' created for player {}", name, identity.to_hex());
    Ok(())
}

/// Move a hero to a new position
#[spacetimedb::reducer]
pub fn move_hero(ctx: &ReducerContext, hero_id: u64, new_x: f32, new_y: f32) -> Result<(), String> {
    let identity = ctx.sender;

    let mut hero = ctx
        .db
        .hero()
        .id()
        .find(hero_id)
        .ok_or("Hero not found")?;

    // Verify ownership
    if hero.player_identity != identity {
        return Err("You do not own this hero".to_string());
    }

    // Verify hero is alive
    if !hero.is_alive {
        return Err("Cannot move a dead hero".to_string());
    }

    // Update position
    hero.x = new_x;
    hero.y = new_y;
    ctx.db.hero().id().update(hero);

    Ok(())
}
