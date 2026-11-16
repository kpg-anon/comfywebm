# scripts

Run from the repo root.

| script | what it does |
| --- | --- |
| `smoke.sh` | Short real encodes of generated clips (bash; needs ffmpeg). One PASS or FAIL line per case. |
| `bump-version.ps1 <x.y.z>` | Sets the version in package.json, package-lock.json, tauri.conf.json, both Cargo.toml files, Cargo.lock and the README (badge and installer name). |
| `release.ps1 <x.y.z>` | Publishes a release that installed copies get as an in-app update. `-DryRun` prints the notes and the steps and changes nothing. |

## Making a release

```powershell
./scripts/release.ps1 1.0.0 -DryRun   # read the notes it will publish
./scripts/release.ps1 1.0.0
```

`release.ps1`:

1. Stops unless you are on `main`, `git config user.email` is the kpg-anon
   noreply address, the GitHub repo is **public**, the tag does not exist and
   the updater key files are there.
2. Sets the version everywhere (`bump-version.ps1`).
3. In CHANGELOG.md, when there is no `## [x.y.z]` section yet, renames
   `## [Unreleased]` to `## [x.y.z] - <today>` and leaves an empty
   `## [Unreleased]` above it. That section is the release notes, in the
   GitHub release and in the app's update notice. Edit it before releasing.
4. Builds the NSIS installer with `npx tauri build`, signed with the updater
   key.
5. Writes `target/release/latest.json` with the version, the notes, the date
   and the installer's URL and signature.
6. Commits only the files it changed (`release: vX.Y.Z`), tags, pushes `main`
   and the tag, and creates the GitHub release with the installer and
   `latest.json`.

## How installed copies update

The app reads
`https://github.com/kpg-anon/comfywebm/releases/latest/download/latest.json`
(`plugins.updater.endpoints` in `src-tauri/tauri.conf.json`), at start and
from Settings > Look > Updates > Check now. When its version is newer, the
notice shows the notes. Update now downloads the installer, checks its
signature against `plugins.updater.pubkey`, runs it in passive mode and
restarts the app.

The download has no sign-in, so this only works while the repo is public. A
private repo answers 404 and the app says "Could not fetch a valid release
JSON from the remote" on Check now (the check at start stays silent).

## Updater key

Two gitignored files in the repo root hold the secret. Never commit them, and
keep a copy somewhere safe: without them no update can be published that
existing installs accept.

- `.tauri-updater.key`: the private key.
- `.tauri-updater.pass`: its passphrase. It must not be empty (PowerShell 5.1
  drops empty environment variables for child processes, and the bundler then
  asks for the passphrase).

The public key is `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`. A
new pair is made with

```powershell
npx tauri signer generate -w .tauri-updater.key -p <passphrase> --ci
```

and the new public key goes into tauri.conf.json. Copies installed with the
old key then cannot update to a build signed with the new one.

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
