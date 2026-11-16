# scripts

Run from the repo root.

| script | what it does |
| --- | --- |
| `smoke.sh` | Short real encodes of generated clips (bash; needs ffmpeg). One PASS or FAIL line per case. |
| `bump-version.ps1 <x.y.z>` | Sets the version in package.json, package-lock.json, tauri.conf.json, both Cargo.toml files, Cargo.lock and the README (badge and installer name). |
| `release.ps1 <x.y.z>` | Starts a release: version, changelog section, commit, tag, push. `-DryRun` prints the notes and the steps and changes nothing. |

## Making a release

```powershell
./scripts/release.ps1 1.0.0 -DryRun   # read the notes it will publish
./scripts/release.ps1 1.0.0
```

1. `release.ps1` stops unless you are on `main`, the files it commits have no
   other changes, `git config user.email` is the kpg-anon noreply address, the
   GitHub repo is **public**, the two signing secrets exist on GitHub and the
   tag is new. It sets the version everywhere, renames `## [Unreleased]` in
   CHANGELOG.md to `## [x.y.z] - <today>` (edit that section first: it is what
   users read), commits those files only (`release: vX.Y.Z`), tags `vX.Y.Z`
   and pushes main and the tag.
2. The tag starts `.github/workflows/release.yml` (Actions > release, about 15
   minutes). It checks that the tag matches the version, takes the CHANGELOG
   section as the release notes, and builds:

   File names (from a build-only run, 2026-11-16):

   | runner | files |
   | --- | --- |
   | windows-latest | `comfywebm_X.Y.Z_x64-setup.exe` (NSIS), `comfywebm_X.Y.Z_x64-portable.zip` |
   | ubuntu-22.04 | `comfywebm_X.Y.Z_amd64.AppImage`, `comfywebm_X.Y.Z_amd64.deb`, `comfywebm-X.Y.Z-1.x86_64.rpm` |
   | macos-latest, Apple silicon | `comfywebm_X.Y.Z_aarch64.dmg`, the app as `.app.tar.gz` (update) |
   | macos-latest, Intel | `comfywebm_X.Y.Z_x64.dmg`, the app as `.app.tar.gz` (update) |

   A first job creates one **draft** release; `tauri-apps/tauri-action` signs
   the update files with the secrets and uploads into it; the Windows job adds
   the portable zip. When all four builds are done, a last job writes
   `latest.json` from the `.sig` files on the draft (`windows-x86_64`,
   `linux-x86_64`, `darwin-aarch64`, `darwin-x86_64`) and stops if one is
   missing.
3. Open the draft on the Releases page, check it, press **Publish release**.
   Only then is it `releases/latest`, which installed copies read.

Actions > release > **Run workflow** builds all platforms without a release;
the bundles are kept as artifacts of that run. Use it to check that a change
still builds everywhere.

## How installed copies update

The app reads
`https://github.com/kpg-anon/comfywebm/releases/latest/download/latest.json`
(`plugins.updater.endpoints` in `src-tauri/tauri.conf.json`), at start and
from Settings > Look > Updates > Check now. When its version is newer, a
notice shows the notes. Update now downloads the file for this system, checks
its signature against `plugins.updater.pubkey`, installs it and restarts.

- Windows: the NSIS installer runs in passive mode.
- macOS: the `.app.tar.gz` replaces the app.
- Linux: only an AppImage can replace itself (the AppImage runtime sets
  `APPIMAGE`). A `.deb` or `.rpm` install shows "Open the release page"
  instead (`update_mode` in `src-tauri/src/lib.rs`).
- Windows portable (a `portable` file beside the exe): also "Open the release
  page"; the user unzips the new version over the folder.

The download has no sign-in, so this only works while the repo is public.

## Updater key

The key pair signs the update files; installed copies accept only files
signed with it.

- `.tauri-updater.key` (the private key) and `.tauri-updater.pass` (its
  passphrase) are in the repo root, gitignored. Keep a copy somewhere safe.
- GitHub needs both as repo secrets for the release workflow:
  ```sh
  gh secret set TAURI_SIGNING_PRIVATE_KEY < .tauri-updater.key
  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD < .tauri-updater.pass
  ```
- The public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`.

A new pair (`npx tauri signer generate -w .tauri-updater.key -p <passphrase> --ci`,
then the new public key into tauri.conf.json and both secrets updated) means
copies installed with the old key cannot update to builds signed with the new
one.

## macOS signing

The macOS builds are signed ad hoc (`signingIdentity: "-"` in
`src-tauri/tauri.macos.conf.json`), not with an Apple Developer ID, so they
are not notarized. Gatekeeper then says the app "cannot be opened" or "is
damaged" on first start; the README says how to open it anyway. A Developer ID
($99 a year) would remove that; it needs `APPLE_CERTIFICATE`,
`APPLE_SIGNING_IDENTITY` and notarization secrets for tauri-action.

## Testing an update locally

1. Build a signed installer with a higher version, without changing files:
   ```sh
   TAURI_SIGNING_PRIVATE_KEY="$(cat .tauri-updater.key)" \
   TAURI_SIGNING_PRIVATE_KEY_PASSWORD="$(cat .tauri-updater.pass)" \
   npx tauri build --config '{"version":"1.0.1"}'
   ```
2. Put the installer and a `latest.json` (version, notes, the `.sig` file's
   text as `signature`, `url` pointing at the server below) in a folder and
   serve it: `python -m http.server 8765 --bind 127.0.0.1`.
3. Start the dev app against it:
   ```sh
   npx tauri dev --config '{"plugins":{"updater":{"endpoints":["http://127.0.0.1:8765/latest.json"],"dangerousInsecureTransportProtocol":true}}}'
   ```
   The notice shows after about 4 seconds. In the devtools console,
   `__cwmUpdater.update.download()` downloads and checks the signature
   without installing; `__previewUpdate()` shows the notice with sample notes
   and no server.

Update now would run the test installer and install it for real.
