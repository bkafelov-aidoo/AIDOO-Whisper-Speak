# Whisper Speak source separation

Separated on 2026-10-07 following the user's explicit naming: Lite is the small dictation app; Speak is the AI assistant.

- Parent committed source: `ec6f3d522ed3c0e2e39405676e6b33f3a9a2d838`, branch `codex/gpt-live-aidoo-assistant`.
- Standalone committed subtree: `663464c61962a16cf7015583f3b33bb4bd4bb101`.
- Current tracked modifications and non-ignored untracked files from the original `AIDOO Whisper Lite/` folder are included in the migration snapshot. They are not replaced by the older committed tree.
- Repository: https://github.com/bkafelov-aidoo/AIDOO-Whisper-Speak (private).
- Local checkout: `products/Whisper Speak` under the original parent folder.

Application code and runtime/release configuration are copied unchanged. Existing internal package/product names, bundle identifier `app.aidoo.whisper-lite`, storage and Keychain names remain legacy identities. This deliberately preserves existing installation/permission behavior. Rebranding those identities and installing Lite and Speak side by side requires a separately authorized migration and installer acceptance test.

`WORKING-METHODS.md` is carried over unchanged, including candidate/confirmed status and installer references. Installers and build caches stay in the original parent checkout's `AIDOO Whisper Lite/` folder. A referenced `release/...` path in that historical ledger therefore resolves against that preserved folder until explicitly migrated. No existing artifact/tag is replaced and no candidate is promoted.

Ignored caches, audio, diagnostics, `.env`, signing credentials and release artifacts are excluded from Git publication. No new DMG, application/AI launch or clinical request was performed for this separation.

Migration checks: all 173 selected original source files are accounted for; they are byte-identical in Speak except the explicit README identity header. Original source SHA-256 fingerprint: `5db31af3d6d66d28677316ff5f42c36ee805688eeea84b0d4938c40135af7a92`. The original tracked-change diff remains `dd455bdc3628f96e55530767f042fc5372cf528160155261e72e5e1b15e209c5`. New standalone instructions, the copied domain context and this separation note are additional documentation, not behavioral changes.

TypeScript, all 94 frontend regression tests, source layout (118 files), localization (89 cases), release configuration, macOS dependency boundary, wake-word assets, website validation, production frontend build, Rust formatting and diff checks pass in this independent folder. Native/installed clinical behavior is not newly exercised; the acceptance limitations in the ledger remain authoritative.
