# Makepad URDF Player

A Makepad-based URDF robot viewer with embeddable RobotView widget.

## Features

- Load and render URDF robots with STL meshes
- Forward kinematics with joint angle control
- Orbit camera (drag to rotate, scroll to zoom)
- Keyboard controls for joint manipulation
- Animation support

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
makepad-urdf-player = { path = "makepad-urdf-player" }
```

## Usage

### 1. Register live_design

In your app's `LiveRegister` implementation, register the library's live_design **after** makepad_widgets:

```rust
impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
        makepad_urdf_player::live_design(cx);  // Register after dependencies
    }
}
```

### 2. Import in live_design!

```rust
live_design! {
    use link::theme::*;
    use link::widgets::*;

    // Import RobotView from library
    use makepad_urdf_player::robot_view::RobotView;

    MyApp = {{MyApp}} {
        robot_viewer = <RobotView> {}
    }
}
```

### 3. Import the WidgetExt trait

```rust
use makepad_urdf_player::robot_view::{RobotViewAction, RobotViewWidgetExt};
```

### 4. Control the widget

```rust
// Get widget reference
let robot_view = self.view.robot_view(id!(robot_viewer));

// Load a robot
robot_view.load_robot(cx, "path/to/robot.urdf", "path/to/meshes");

// Set joint angles
robot_view.set_joint_angles(cx, &[0.0, 0.5, -0.3, 0.0, 0.0, 0.0]);

// Get current angles
let angles = robot_view.get_joint_angles();

// Reset to default pose
robot_view.reset_view(cx);
```

## API Reference

### RobotViewRef Methods

| Method | Description |
|--------|-------------|
| `load_robot(cx, urdf_path, assets_dir)` | Load URDF robot from file |
| `set_joint_angles(cx, &[f32])` | Set all joint angles at once |
| `get_joint_angles() -> Vec<f32>` | Get current joint angles |
| `set_joint_angle(cx, idx, angle)` | Set single joint angle |
| `animate_step(cx, step)` | Advance animation by one frame |
| `toggle_animation(cx) -> bool` | Toggle internal animation |
| `reset_view(cx)` | Reset camera and joints to defaults |
| `get_selected_joint() -> usize` | Get currently selected joint index |
| `get_joint_info(idx) -> Option<(name, angle, lower, upper)>` | Get joint metadata |

### RobotViewAction

Actions emitted by the widget:

```rust
pub enum RobotViewAction {
    None,
    JointChanged { joint_idx: usize, angle: f32 },
    AnimationToggled(bool),
}
```

Handle actions:

```rust
for action in &actions {
    match action.as_widget_action().cast::<RobotViewAction>() {
        RobotViewAction::JointChanged { joint_idx, angle } => {
            println!("Joint {} changed to {}", joint_idx, angle);
        }
        RobotViewAction::AnimationToggled(animating) => {
            println!("Animation: {}", animating);
        }
        _ => {}
    }
}
```

## Keyboard Controls

| Key | Action |
|-----|--------|
| Arrow Left/Right | Select joint |
| Arrow Up/Down | Adjust selected joint angle |
| R | Reset pose and camera |

## Animation

For reliable animation, manage the timer at the parent widget level:

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

        // Handle timer at parent level
        if self.anim_timer.is_event(event).is_some() {
            self.anim_step += 1;
            let robot_view = self.view.robot_view(id!(robot_viewer));
            robot_view.animate_step(cx, self.anim_step);
            self.redraw(cx);
        }
    }
}

// Start animation
self.anim_timer = cx.start_interval(0.033);  // ~30 FPS

// Stop animation
self.anim_timer = Timer::default();
```

## Example

See `src/main.rs` for a complete example application.

## File Structure

```
src/
├── lib.rs         # Library exports
├── robot_view.rs  # RobotView widget
├── mesh.rs        # 3D mesh shader
└── main.rs        # Example app
```

## License

MIT
