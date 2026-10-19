# Devcontainer

VS Code environment for the whole project: Ubuntu 24.04 with ffmpeg,
numpy/Pillow for the Python detector spike, Node for the app's front end, Rust
(rustup) for `fpv-rs`, and the WebKitGTK/GStreamer libraries the Tauri app
builds and plays video against — plus Xvfb and xdotool to drive its window
headless. The library is SQLite (bundled by rusqlite), so there is no database
service to start.

## Why not Docker Compose

`devcontainer.json` builds the `Dockerfile` directly. Docker Compose v5 always
builds through `buildx bake`, and bake refuses to read the Dockerfile the Dev
Containers extension generates in a temporary directory outside the build
context:

```
additional privileges requested: pass "--allow=fs.read=/private/var/folders/.../Dockerfile-with-features"
```

`COMPOSE_BAKE=false` no longer works around it. Setting
`BUILDX_BAKE_ENTITLEMENTS_FS=0` does, but it has to be in VS Code's
environment on every machine. A plain build does not go through bake at all.

## Footage

Copy the `PICT####.AVI` files into `./media` (gitignored), or uncomment the
bind mount in `devcontainer.json` and rebuild the container.
