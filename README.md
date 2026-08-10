# Makepad URDF Viewer

An embeddable 3D URDF robot viewer built on [Makepad](https://github.com/makepad/makepad).
The crate is a **library first** — `RobotView` is a widget you drop into your own
Makepad app — with a small demo binary and an example that show how to drive it.

Runs on desktop (Metal/OpenGL) and in the browser (WebAssembly/WebGL).

![the widget](docs/screenshot.png)

## Features

- **URDF + STL loading**, with a visible fallback when a mesh is missing
- **Forward kinematics** for revolute, continuous and fixed joints
- **Orbit / pan / zoom camera** that pivots on the model, with near-vertical pitch
- **Joint control** from the host (`set_joint_angles`) or the keyboard
- **Daylight environment**: screen-space sky dome, ground plane and a grid that
  runs to the horizon, all themable from the DSL
- **Draggable key light** — alt+drag places the sun anywhere on the sphere
- **Geometry sharing** — repeated links (e.g. a 32-unit array) upload one buffer
- **No built-in model**: the widget shows whatever the host asks for

## Requirements

- Rust 1.75+
- **A sibling checkout of the patched makepad fork.** Upstream makepad `dev`
  cannot currently run 3D apps on WebAssembly — its own `examples/box3d` crashes
  — so `Cargo.toml` carries a `[patch]` pointing at `../makepad-dev-patched`.
  See [Patched makepad](#patched-makepad) below. Drop the patch once the fixes
  land upstream.

## Quick start

```bash
cargo run --release                 # demo app: Redbank III unit, 4x8 array, SO-100
cargo run --release --example embed -- data/so100.urdf data
```

## Controls

| Input | Action |
|---|---|
| Drag | Orbit (pivots on the model) |
| Shift+drag / right-drag / middle-drag | Pan |
| **Alt+click / alt+drag** | Place and move the sun |
| Wheel | Zoom |
| ←/→ | Select joint |
| ↑/↓ | Move the selected joint |
| A | Toggle animation |
| R | Reset pose and camera |
| L | Toggle the light |

Alt is deliberately the light's own modifier, so orbit, pan and the joint keys
keep working while the light is on.

## Using it in your app

Add the dependency (plus the `[patch]` from [below](#patched-makepad)):

```toml
[dependencies]
makepad-urdf-player = { git = "https://github.com/dorobot-org/makepad-urdf-viewer" }
```

### 1. Register the script module

```rust
impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        makepad_xr::script_mod(vm);
        makepad_urdf_player::script_mod(vm);   // the widget's shaders + DSL types
        self::script_mod(vm)
    }
}
```

### 2. Place the widget

```rust
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    load_all_resources() do #(App::script_component(vm)){
        ui: Root{ main_window := Window{ body +: {
            viewer := mod.widgets.RobotView{
                // optional: load declaratively. Omit and call load_robot() instead.
                urdf: "data/so100.urdf"
                assets: "data"

                // optional: theme the environment
                show_grid: true
                sky_horizon: #xFFFBFB
                sky_zenith:  #xFAE5E7
                ground_color: #xFFFFC5
                grid_color:  #x6B6A3D
            }
        }}}
    }
}
```

### 3. Drive it

Everything goes through `RobotViewRef`, which handles the borrow for you:

```rust
use makepad_urdf_player::robot_view::RobotViewWidgetRefExt;

let viewer = self.ui.robot_view(cx, ids!(viewer));

viewer.load_robot(cx, "data/so100.urdf", "data")?;  // or "embedded" assets
viewer.set_joint_angles(cx, &[0.0, 0.5, -0.3, 0.0, 0.0, 0.0]);
let angles = viewer.joint_angles();
viewer.reset_view(cx);
viewer.set_light_on(cx, true);
viewer.set_light_angles(cx, 0.8, 0.6);   // azimuth, elevation (radians)
```

### 4. React to what it reports

```rust
fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
    let viewer = self.ui.robot_view(cx, ids!(viewer));
    if let Some((links, joints)) = viewer.loaded(actions) {
        log!("{links} links, {joints} movable joints");
    }
    if let Some((path, err)) = viewer.load_failed(actions) {
        error!("{path}: {err}");
    }
}
```

`examples/embed.rs` is a complete, runnable version of all of the above,
including driving the joints every frame — the hook a teleop UI or a dataset
player would use.

## API reference

### `RobotViewRef`

| Method | Description |
|---|---|
| `load_robot(cx, urdf, assets_dir) -> Result<(), String>` | Load a model; `assets_dir` may be `"embedded"` |
| `clear_robot(cx)` | Drop the current model |
| `loaded(actions) -> Option<(usize, usize)>` | `(links, movable_joints)` on the frame a load succeeded |
| `load_failed(actions) -> Option<(String, String)>` | `(path, error)` on the frame a load failed |
| `set_joint_angles(cx, &[f32])` | Pose the movable joints, in URDF order, **unclamped** |
| `joint_angles() -> Vec<f32>` | Current movable-joint angles |
| `movable_joint_count() -> usize` | How many joints `set_joint_angles` expects |
| `reset_view(cx)` | Re-frame the camera and reset the pose |
| `is_light_on()` / `set_light_on(cx, bool)` | The sun |
| `set_light_angles(cx, yaw, pitch)` | Aim it (radians) |
| `is_animating()` / `set_animating(cx, bool)` | Built-in joint sweep |

`RobotView` itself additionally exposes `robot()` for direct access to the
loaded `Robot` (links, joints, transforms).

### DSL properties

| Property | Default | Description |
|---|---|---|
| `urdf`, `assets` | `""` | Load on first draw. Empty = start empty |
| `show_grid` | `true` | Draw the ground grid |
| `sky_horizon`, `sky_zenith` | pale pink | Sky gradient endpoints |
| `ground_color` | `#xFFFFC5` | Ground fill |
| `grid_color` | `#x6B6A3D` | Grid lines |

### Actions

```rust
pub enum RobotViewAction {
    None,
    Loaded { links: usize, movable_joints: usize },
    LoadFailed { path: String, error: String },
}
```

## WebAssembly

```bash
cargo makepad wasm --no-threads build -p makepad-urdf-player --release
```

`--no-threads` avoids the COOP/COEP headers, so the output can be served from
static hosting. Do **not** pass `--strip` — it miscompiles this build.

The browser has no filesystem, so meshes are resolved through a virtual asset
registry. Embed them in the binary and register them before the first load:

```rust
use makepad_urdf_player::robot::set_virtual_assets;

let mut m = std::collections::HashMap::new();
m.insert("my_robot.urdf", include_bytes!("../data/my_robot.urdf").as_slice());
m.insert("base.stl",      include_bytes!("../data/base.stl").as_slice());
set_virtual_assets(m);
// then: viewer.load_robot(cx, "my_robot.urdf", "embedded")
```

Meshes resolve by **basename**, so URDF paths like `meshes/base.stl` find the
`base.stl` entry.

## Patched makepad

Upstream `dev` cannot run 3D apps on wasm today. The sibling fork carries four
fixes this widget needs:

1. `windows[id_zero]` indexed before any window exists (panic on web init)
2. `ToWasmPaintDirty` unwrapping a `main_pass_id` that can be `None`
3. render-to-texture passes overwriting `camera_projection` even with
   `keep_camera_matrix` set — the "black viewport" cause
4. offscreen passes getting no depth attachment

Plus a widened camera pitch clamp (±1.45 → ±1.55 rad) for near-vertical views.
All are upstreamable; remove the `[patch]` block from `Cargo.toml` when they land.

## Features

| Feature | Default | Effect |
|---|---|---|
| `episode` | off | LeRobot parquet playback (pulls `arrow`/`parquet`, native only) |
| `profiling` | off | Performance output |

## Project structure

```
src/
├── lib.rs           # exports + script_mod registration
├── main.rs          # demo app (Redbank III unit / 4x8 array / SO-100)
├── robot_view.rs    # the RobotView widget — the library's main surface
├── error.rs         # error and warning types
├── episode.rs       # parquet episode loader (feature = "episode")
├── mesh.rs          # MeshData (CPU-side geometry)
├── profiling.rs
├── robot/
│   ├── model.rs     # Robot / Link / Joint, bounds, joint setters
│   ├── loader.rs    # URDF + STL loading, virtual asset registry
│   └── kinematics.rs
└── render/
    ├── mesh.rs
    └── draw.rs      # DrawRobotMesh, DrawGridPlane, DrawSceneComposite (sky)

examples/embed.rs    # minimal integration
data/                # demo models: Redbank III, SO-100
tests/               # headless load + framing checks
```

## Adding your own robot

1. Put the URDF and its STLs anywhere on disk.
2. `viewer.load_robot(cx, "path/to/robot.urdf", "path/to/assets")`.

Mesh paths inside the URDF resolve against the assets directory, by basename if
an exact path miss occurs. `scale` on `<mesh>` is honoured.

## Notes for integrators

- **Y-up vs Z-up**: URDF is Z-up, the scene camera is Y-up. The conversion is
  applied per-instance inside the widget; you always work in URDF coordinates.
- **`set_joint_angles` does not clamp.** It is the playback path, so recorded
  data renders exactly as recorded. The keyboard controls clamp to URDF limits.
- **Framing uses world-space bounds**, so models with translated joints orbit
  about themselves rather than an offset point.

## Dependencies

| Crate | Purpose |
|---|---|
| makepad-widgets / makepad-xr | UI framework, GPU rendering, camera |
| [urdf-rs](https://crates.io/crates/urdf-rs) | URDF parsing |
| [glam](https://crates.io/crates/glam) | Linear algebra |
| [stl_io](https://crates.io/crates/stl_io) | STL loading |
| [parquet](https://crates.io/crates/parquet) / arrow | Episode playback (optional) |

## License

MIT
