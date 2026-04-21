# Execution And Test Cleanup Plan

1. Extract delegate background execution from `execution/chat.rs` into a dedicated module and keep public behavior unchanged.
2. Extract inbox wake-up execution from `execution/chat.rs` into a dedicated module and keep the same wake-up semantics.
3. Move scenario-heavy config/records/scheduler tests into dedicated sibling test modules.
4. Run:
   - `cargo fmt --all`
   - `cargo clippy --workspace --all-targets --all-features -- -D warnings`
   - `cargo test --workspace --all-features`
   - `cargo build -p agentd`
   - `cargo build --release -p agentd`
