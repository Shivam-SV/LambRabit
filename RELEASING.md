# Releasing lampRabit

Releases are built by GitHub Actions ([.github/workflows/release.yml](.github/workflows/release.yml)) when a version tag is pushed. They land as a **draft** release with installers for every platform and the `latest.json` file the in-app updater reads.

## One-time setup

The updater only installs updates signed with lampRabit's key. The public half is in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`); the private half must never be committed.

1. The key pair was generated on the maintainer's machine at `~/.tauri/lamprabit-updater.key` (private) and `.key.pub` (public), with no password. **Back up the private key somewhere safe** (a password manager). If it's lost, existing installs can't be updated automatically any more.
2. In GitHub → repository → Settings → Secrets and variables → Actions, add:
   - `TAURI_SIGNING_PRIVATE_KEY`: the full contents of `lamprabit-updater.key`.
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: empty (add it anyway, with an empty value), or the password if you regenerate the key with one.

To rotate the key: `npx tauri signer generate -w ~/.tauri/lamprabit-updater.key`, put the new `.pub` contents in `tauri.conf.json`, update the secret. Installs made with the old key won't accept updates signed with the new one.

### Code signing (recommended before a public launch)

Unsigned installers work but trigger warnings: Windows SmartScreen ("unrecognised app") and macOS Gatekeeper (macOS refuses unnotarised apps by default). When certificates are available:

- **Windows:** an OV/EV code-signing certificate; configure `bundle.windows.certificateThumbprint` or Tauri's `signCommand`, and the matching secrets.
- **macOS:** an Apple Developer ID certificate plus notarisation; tauri-action reads `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`.

See https://v2.tauri.app/distribute/sign/ for details.

## Each release

1. Bump the version in **both** `package.json` and `src-tauri/tauri.conf.json` (and `src-tauri/Cargo.toml`) to the same value, for example `0.2.0`.
2. Commit, then tag and push:

   ```bash
   git tag v0.2.0
   git push origin main v0.2.0
   ```

3. Wait for the Release workflow (about 15–20 minutes for all platforms).
4. Open the draft release on GitHub, check the installers, write release notes, then **Publish**. From that moment the in-app updater offers it to existing installs.

## Building locally

```bash
# Windows example; installers end up in src-tauri/target/release/bundle/
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/lamprabit-updater.key)"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
npx tauri build
```

Without the signing key the build still succeeds but warns that updater artifacts can't be signed.
