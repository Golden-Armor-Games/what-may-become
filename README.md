# What May Become

A hero-based real-time strategy RPG by **Golden Armor Games**.

## Vision

What May Become is a browser-accessible multiplayer RPG where every choice matters. Players pick a realm and race, build their legacy through combat and leadership, and face permanent consequences—including permadeath.

### Core Pillars

- **Meaningful Choices**: Your town is attacked. Do you lead, cower, run, or defend? Each choice shapes your story and closes other paths.
- **Fame System**: Rise from local nobody to godly legend through four tiers of recognition.
- **Permadeath**: Death is permanent. Resurrection requires magic, divine intervention, or aid from higher-level players.
- **Skill Mastery**: Cast 10,000 fireballs and yours will be far more devastating than someone who cast 100.
- **Army Management**: Divide your forces, sacrifice your own strength to empower devoted followers.
- **Strategic Holdings**: Keeps and strongholds provide resource benefits—but there are limited slots in the world.
- **Living World**: Wounded troops survive combat and can teach, work towns, or gather resources.
- **Limited Communication**: Town Criers and message birds maintain tension and fear of the unknown.
- **Community Storytelling**: Submit quests and stories through an approval pipeline.

### Target Platform

Browser-based via WASM for maximum accessibility—not just high-end gaming PCs.

## Tech Stack

- **Client**: [Bevy](https://bevyengine.org/) (Rust game engine) with WASM support
- **Backend**: [SpacetimeDB](https://spacetimedb.com/) (real-time database + server logic as Reducers)
- **Integration**: `bevy_spacetimedb` crate

## Project Structure

```
what-may-become/
├── Cargo.toml             # Workspace root
├── client/                # Bevy game client
│   ├── Cargo.toml
│   └── src/main.rs
├── server/                # SpacetimeDB module
│   ├── Cargo.toml
│   └── src/lib.rs
└── docs/
    └── vision.md          # Full game design vision
```

## Setup

### Prerequisites

- Rust (latest stable)
- SpacetimeDB CLI

### Install Dependencies

```bash
make setup
```

This installs the SpacetimeDB CLI and adds the WASM target.

### Running Locally

1. Start the SpacetimeDB server:
   ```bash
   make dev-server
   ```

2. Publish the server module:
   ```bash
   make publish
   ```

3. Run the client:
   ```bash
   make client
   ```

### Building for Browser

```bash
make wasm
```

## Development

See [docs/vision.md](docs/vision.md) for the full game design document.

## License

MIT License - Golden Armor Games
