# scripts

Run from the repo root.

| script | what it does |
| --- | --- |
| `smoke.sh` | Short real encodes of generated clips (bash; needs ffmpeg). One PASS or FAIL line per case. |
| `bump-version.ps1 <x.y.z>` | Sets the version in package.json, package-lock.json, tauri.conf.json, both Cargo.toml files, Cargo.lock and the README (badge and installer name). |
| `release.ps1 <x.y.z>` | Releases from this PC: builds Windows, publishes the release at once; GitHub Actions then adds macOS and Linux. `-DryRun` prints the notes and the steps and changes nothing. |

## Making a release

```powershell
./scripts/release.ps1 1.0.1 -DryRun   # read the notes it will publish
./scripts/release.ps1 1.0.1
```

Write what changed under `## [Unreleased]` in CHANGELOG.md first: that
section becomes the release notes, on the release page and in the app's
update notice.

`release.ps1`, on this PC (Windows):

1. Checks: on `main`, no other changes in the files it commits, the kpg-anon
   noreply address as git identity, `gh` signed in, the repo public, the two
   signing secrets on GitHub, the local key files, a new tag.
2. Sets the version everywhere and turns `[Unreleased]` into
   `## [x.y.z] - <today>`.
3. Deletes old bundles of that version and builds the signed NSIS installer
   (`npx tauri build`, about 3 minutes) with the MSVC toolchain
   (`stable-x86_64-pc-windows-msvc`, for this build only), checking that it
   is new. Microsoft Defender flagged the GNU (MinGW) builds of 1.0.1 and
   1.0.2 as `Trojan:Win32/Bearfoos.A!ml`; the same source built with MSVC
   was not flagged. Needs Visual Studio Build Tools with the C++ workload
   (`winget install Microsoft.VisualStudio.BuildTools` with
   `--add Microsoft.VisualStudio.Workload.VCTools --includeRecommended`).
4. Scans the installer and the exe with Microsoft Defender (report only);
   a detection stops the release before anything is committed or published.
5. Makes the portable zip (`comfywebm.exe`, which has the WebView2 loader
   inside, and the `portable` file) and a `latest.json` that lists Windows.
6. Commits only its files (`release: vX.Y.Z`), pushes `main`, and runs
   `gh release create`: the tag and a **published** release with the
   installer, its `.sig`, the portable zip and `latest.json`. Windows copies
   can update from that moment.

Publishing starts `.github/workflows/release.yml` (about 15 minutes):

1. `prep` finds the release and what it lacks.
2. `build` makes, and uploads into the release:

   | runner | files |
   | --- | --- |
   | ubuntu-22.04 | `comfywebm_X.Y.Z_amd64.AppImage`, `comfywebm_X.Y.Z_amd64.deb`, `comfywebm-X.Y.Z-1.x86_64.rpm` |
   | macos-latest, Apple silicon | `comfywebm_X.Y.Z_aarch64.dmg`, `comfywebm_aarch64.app.tar.gz` (update) |
   | macos-latest, Intel | `comfywebm_X.Y.Z_x64.dmg`, `comfywebm_x64.app.tar.gz` (update) |
   | windows-latest | only when the release has no installer |

3. `latest` rewrites `latest.json` from the `.sig` files on the release
   with all four platforms (`windows-x86_64`, `linux-x86_64`,
   `darwin-aarch64`, `darwin-x86_64`), and stops if one is missing.

Until `latest` has run, `latest.json` lists Windows only, so Mac and Linux
copies find no update (their check at start stays silent; Check now says the
platform is missing).

If a CI job fails: fix it on `main`, then Actions > release > **Run
workflow** with the tag (for example `v1.0.1`): it builds into that release
again and rewrites `latest.json`. Run without a tag, it builds every
platform as workflow artifacts and touches no release (to check that
everything still builds).

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
