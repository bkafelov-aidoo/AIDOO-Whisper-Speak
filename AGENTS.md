# Whisper Speak

This independent repository owns the voice/AI assistant and AIDOO record workflows. The user assigned this product name on 2026-10-07. Its internal application identity still uses the legacy Lite name; read `docs/PRODUCT-SEPARATION.md` before changing release identity or migrating installed data.

## Protected working methods

Before changing a flow recorded in `docs/WORKING-METHODS.md`, read the complete ledger. An explicit user request naming a protected flow authorizes a candidate change; adjacent work does not.

Keep the last user-confirmed baseline recoverable. Only an explicit user statement that a candidate works promotes it. On promotion, update the ledger and run `scripts/capture-working-baseline.py` in this repository. Preserve confirmed tags and referenced installers.

After a user-requested rollback, restore only the affected flow from its confirmed source. Existing candidates retain their recorded acceptance status after migration.
