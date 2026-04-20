# Context Offload Artifact Persistence Plan

1. Add runtime offload types in `agent-runtime/src/context.rs`.
2. Add `ContextOffloadRecord` and conversion support in persistence records.
3. Add `ContextOffloadRepository` trait and exports.
4. Add `context_offloads` schema, validation, and store implementation.
5. Persist payload bytes through the artifact store and prune obsolete offload artifacts on replacement.
6. Cover the slice with runtime and persistence tests before running the full workspace checks.
