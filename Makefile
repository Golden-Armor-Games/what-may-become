.PHONY: dev-server publish client wasm setup generate

# Start the local SpacetimeDB server
dev-server:
	spacetime start

# Publish the server module to local SpacetimeDB
publish:
	cd server && spacetime publish --server local what-may-become

# Run the native Bevy client
client:
	cargo run -p what-may-become-client

# Build and run the WASM client
wasm:
	cargo build -p what-may-become-client --target wasm32-unknown-unknown --release

# Generate SpacetimeDB client bindings
generate:
	cd server && spacetime generate --lang rust --out-dir ../client/src/module_bindings

# Install dependencies: SpacetimeDB CLI and WASM target
setup:
	@echo "Installing SpacetimeDB CLI..."
	curl -sSf https://install.spacetimedb.com | sh
	@echo "Adding WASM target..."
	rustup target add wasm32-unknown-unknown
	@echo "Setup complete!"
