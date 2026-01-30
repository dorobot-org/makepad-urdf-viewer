# Makepad URDF Viewer

A high-performance 3D URDF robot visualization tool built with [Makepad](https://github.com/makepad/makepad). Features real-time joint control, forward kinematics, and animation playback.

## Features

- **URDF Robot Loading** - Parse URDF files and load STL meshes with automatic fallback for missing files
- **Forward Kinematics** - Real-time joint transform computation supporting revolute, continuous, and fixed joints
- **Interactive Camera** - Orbital camera with smooth mouse drag/scroll controls
- **Joint Control** - Keyboard-based joint selection and manipulation with visual feedback
- **Animation** - Built-in sinusoidal animation and episode playback from parquet files
- **Multiple Robots** - Includes VX300s, SO100, LeKiwi, and iCub robot models
- **Embeddable Widget** - RobotView can be integrated into any Makepad application
- **Visual Options** - Toggle specular lighting, joint axes, and world coordinate axes

## Requirements

- Rust 1.75 or later
- macOS, Linux, or Windows

## Quick Start

```bash
# Clone the repository
git clone https://github.com/user/makepad-urdf-viewer
cd makepad-urdf-viewer

# Run the viewer (first build may take a few minutes)
cargo run

# Or run in release mode for better performance
cargo run --release
```

## Controls

| Input | Action |
|-------|--------|
| Mouse Drag | Orbit camera |
| Shift + Drag | Pan camera |
| Scroll | Zoom in/out |
| Arrow Left/Right | Select joint |
| Arrow Up/Down | Adjust joint angle |
| +/- | Zoom camera |
| A | Toggle animation |
| R | Reset pose and camera |

## Using as a Library

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
makepad-urdf-player = { git = "https://github.com/user/makepad-urdf-viewer" }
```

### Step 1: Register live_design

```rust
impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
        makepad_urdf_player::live_design(cx);
    }
}
```

### Step 2: Add RobotView to your UI

```rust
live_design! {
    use link::theme::*;
    use link::widgets::*;
    use makepad_urdf_player::robot_view::RobotView;

    MyApp = {{MyApp}} {
        robot_viewer = <RobotView> {
            width: Fill
            height: Fill
        }
    }
}
```

### Step 3: Control the widget

```rust
use makepad_urdf_player::robot_view::{RobotViewAction, RobotViewWidgetExt};

// Get widget reference
let robot_view = self.view.robot_view(id!(robot_viewer));

// Load a robot
robot_view.load_robot(cx, "data/vx300s/vx300s.urdf", "data/vx300s");

// Set joint angles (radians)
robot_view.set_joint_angles(cx, &[0.0, 0.5, -0.3, 0.0, 0.0, 0.0]);

// Get current angles
let angles = robot_view.get_joint_angles();

// Toggle features
robot_view.toggle_specular(cx);      // Toggle lighting
robot_view.toggle_joint_axes(cx);    // Show joint axes
robot_view.toggle_world_axes(cx);    // Show XYZ axes

// Reset to default pose
robot_view.reset_view(cx);
```

### Step 4: Handle actions

```rust
fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
    let actions = cx.capture_actions(|cx| {
        self.view.handle_event(cx, event, scope);
    });

    for action in &actions {
        match action.as_widget_action().cast::<RobotViewAction>() {
            RobotViewAction::JointChanged { joint_idx, angle } => {
                // Joint was adjusted
            }
            RobotViewAction::LoadingComplete => {
                // Robot finished loading
            }
            RobotViewAction::LoadingError { message } => {
                // Handle load error
            }
            _ => {}
        }
    }
}
```

### Animation Example

For smooth animation, manage the timer at the parent widget level:

```rust
#[derive(Live, LiveHook, Widget)]
pub struct MyApp {
    #[deref] view: View,
    #[rust] anim_timer: Timer,
    #[rust] anim_step: u64,
}

impl Widget for MyApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        if self.anim_timer.is_event(event).is_some() {
            self.anim_step += 1;
            let robot_view = self.view.robot_view(id!(robot_viewer));
            robot_view.animate_step(cx, self.anim_step);
        }
    }
}

// Start animation (~30 FPS)
self.anim_timer = cx.start_interval(0.033);

// Stop animation
self.anim_timer = Timer::default();
```

## API Reference

### RobotViewRef Methods

| Method | Description |
|--------|-------------|
| `load_robot(cx, urdf_path, assets_dir)` | Load URDF robot from file |
| `reload_robot(cx, urdf_path, assets_dir)` | Reload with new robot |
| `set_joint_angles(cx, &[f32])` | Set all joint angles (radians) |
| `get_joint_angles() -> Vec<f32>` | Get current joint angles |
| `set_joint_angle(cx, idx, angle)` | Set single joint angle |
| `animate_step(cx, step)` | Advance animation by one frame |
| `toggle_animation(cx) -> bool` | Toggle internal animation |
| `reset_view(cx)` | Reset camera and joints |
| `toggle_specular(cx) -> bool` | Toggle specular lighting |
| `toggle_joint_axes(cx) -> bool` | Toggle joint axis visualization |
| `toggle_world_axes(cx) -> bool` | Toggle world XYZ axes |
| `get_selected_joint() -> usize` | Get selected joint index |
| `get_joint_info(idx)` | Get joint name, angle, limits |
| `load_episode(cx, path)` | Load episode from parquet file |
| `toggle_episode(cx) -> bool` | Toggle episode playback |
| `is_loading() -> bool` | Check if robot is loading |
| `is_loaded() -> bool` | Check if robot is ready |

### RobotViewAction

```rust
pub enum RobotViewAction {
    None,
    JointChanged { joint_idx: usize, angle: f32 },
    AnimationToggled(bool),
    LoadingStarted,
    LoadingComplete,
    LoadingError { message: String },
}
```

## Project Structure

```
src/
├── lib.rs              # Library exports
├── main.rs             # Example application
├── robot_view.rs       # RobotView widget (main API)
├── error.rs            # Error types
├── episode.rs          # Parquet episode loader
├── profiling.rs        # Performance profiling
├── camera/
│   ├── camera3d.rs     # 3D orbital camera
│   └── controller.rs   # Input handling
├── robot/
│   ├── model.rs        # Robot, Link, Joint types
│   ├── loader.rs       # URDF parser
│   └── kinematics.rs   # Forward kinematics
└── render/
    ├── mesh.rs         # MeshData (CPU)
    ├── geometry.rs     # GPU geometry
    └── draw.rs         # Shaders (DrawMesh, DrawGrid)

data/
├── vx300s/             # ViperX 300 6-DOF robot
├── so100/              # SO-ARM100 robot
└── lekiwi/             # Mobile manipulator
```

## Building and Running

```bash
# Run the application (debug mode)
cargo run

# Run in release mode (recommended for better performance)
cargo run --release

# Build only (debug)
cargo build

# Build only (release)
cargo build --release

# Run with profiling output
cargo run --features profiling

# Run tests
cargo test

# Check code without building
cargo check

# Format code
cargo fmt

# Run clippy lints
cargo clippy
```

## Included Robots

| Robot | Description | Joints |
|-------|-------------|--------|
| VX300s | ViperX 300 6-DOF arm (ALOHA) | 6 |
| SO100 | SO-ARM100 robot | 6 |
| LeKiwi | Mobile manipulator | 9 |

## Adding Your Own Robot

1. Place your URDF file and STL meshes in a directory under `data/`
2. Update the mesh paths in the URDF to be relative to the assets directory
3. Load via: `robot_view.load_robot(cx, "data/myrobot/robot.urdf", "data/myrobot")`

## Dependencies

| Crate | Purpose |
|-------|---------|
| [makepad-widgets](https://github.com/makepad/makepad) | UI framework and GPU rendering |
| [urdf-rs](https://crates.io/crates/urdf-rs) | URDF XML parsing |
| [glam](https://crates.io/crates/glam) | Linear algebra (vectors, matrices, quaternions) |
| [stl_io](https://crates.io/crates/stl_io) | STL mesh file loading |
| [parquet](https://crates.io/crates/parquet) | Episode data loading (LeRobot format) |

## License

MIT
