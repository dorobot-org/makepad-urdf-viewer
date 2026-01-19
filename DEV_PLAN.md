# Development Plan: makepad-urdf-player

**Based on**: roadmap-glm.md analysis
**Created**: 2025-01-19
**Target**: Production-ready v1.0

---

## Executive Summary

This plan prioritizes the critical issues identified in the roadmap into **4 sprints** over **8 weeks**. Focus is on architectural debt first (enables all other improvements), then UX, then performance.

---

## Sprint 1: Foundation (Weeks 1-2)

### Goal: Extract core modules to enable parallel development

#### 1.1 Extract Camera Module
**Files**: New `src/camera/` directory
**Effort**: 2 days

```
src/camera/
├── mod.rs           # pub use exports
├── camera3d.rs      # Camera3D struct (from robot_view.rs:44-120)
└── controller.rs    # CameraController for input handling
```

**Tasks**:
- [ ] Create `Camera3D` struct with orbital parameters
- [ ] Extract `from_orbital()`, `orbit()`, `pan()`, `zoom()` methods
- [ ] Create `CameraController` wrapping Camera3D + input state
- [ ] Add pitch clamping (prevent gimbal lock)
- [ ] Add smooth animation support (lerp-based)
- [ ] Port ray intersection from makepad-d3 (for future picking)

**API**:
```rust
pub struct Camera3D {
    pub distance: f64,
    pub yaw: f64,
    pub pitch: f64,
    pub target: DVec3,
    pub fov: f64,
    pub near: f64,
    pub far: f64,
}

impl Camera3D {
    pub fn from_orbital(distance: f64, yaw: f64, pitch: f64) -> Self;
    pub fn orbit(&mut self, delta_yaw: f64, delta_pitch: f64);
    pub fn pan(&mut self, delta_x: f64, delta_y: f64);
    pub fn zoom(&mut self, factor: f64);
    pub fn view_matrix(&self) -> Mat4;
    pub fn projection_matrix(&self, aspect: f64) -> Mat4;
}
```

#### 1.2 Extract Robot Model Module
**Files**: New `src/robot/` directory
**Effort**: 3 days

```
src/robot/
├── mod.rs           # pub use exports
├── model.rs         # Robot, RobotLink, RobotJoint structs
├── loader.rs        # URDF parsing + STL loading
└── kinematics.rs    # Forward kinematics
```

**Tasks**:
- [ ] Move `Robot`, `RobotLink`, `RobotJoint` to `model.rs`
- [ ] Move URDF parsing to `loader.rs`
- [ ] Move FK computation to `kinematics.rs`
- [ ] Add `RobotLoadError` enum with user-friendly messages
- [ ] Add basic validation (missing links, joint limits)

**API**:
```rust
// src/robot/loader.rs
pub fn load_robot(urdf_path: &Path, assets_dir: &Path) -> Result<Robot, RobotLoadError>;

// src/robot/model.rs
impl Robot {
    pub fn set_joint_angle(&mut self, index: usize, angle: f64);
    pub fn get_joint_angle(&self, index: usize) -> f64;
    pub fn joint_count(&self) -> usize;
}

// src/robot/kinematics.rs
pub trait ForwardKinematics {
    fn update_fk(&mut self);
}
```

#### 1.3 Create Error Types
**File**: `src/error.rs`
**Effort**: 0.5 days

```rust
#[derive(Debug, thiserror::Error)]
pub enum RobotError {
    #[error("Failed to read URDF: {path}")]
    UrdfReadError { path: String, source: std::io::Error },

    #[error("Failed to parse URDF: {0}")]
    UrdfParseError(String),

    #[error("Missing mesh file: {path}")]
    MeshNotFound { path: String },

    #[error("Invalid joint reference: {joint} -> {link}")]
    InvalidJointReference { joint: String, link: String },
}

impl RobotError {
    pub fn user_message(&self) -> String { /* friendly message */ }
}
```

---

## Sprint 2: Renderer Separation (Weeks 3-4)

### Goal: Decouple GPU rendering from UI widget

#### 2.1 Create Render Module
**Files**: Reorganize `src/mesh.rs` into `src/render/`
**Effort**: 3 days

```
src/render/
├── mod.rs           # pub use exports
├── mesh.rs          # MeshData (existing)
├── geometry.rs      # GeometryMesh3D (existing)
├── shaders.rs       # DrawMesh, DrawGrid, DrawSkybox
└── renderer.rs      # RobotRenderer (new)
```

**Tasks**:
- [ ] Move `MeshData` to `render/mesh.rs`
- [ ] Move `GeometryMesh3D` to `render/geometry.rs`
- [ ] Move `DrawMesh`, `DrawGrid` to `render/shaders.rs`
- [ ] Create `RobotRenderer` struct

**RobotRenderer API**:
```rust
pub struct RobotRenderer {
    link_drawers: Vec<DrawMesh>,
    grid_drawer: DrawGrid,
    axis_drawers: Vec<DrawMesh>,
    clip_rect: Vec4,
    window_size: Vec2,
}

impl RobotRenderer {
    pub fn new(cx: &mut Cx, robot: &Robot) -> Result<Self, RenderError>;
    pub fn set_clip_rect(&mut self, rect: Vec4, window: Vec2);
    pub fn update_transforms(&mut self, robot: &Robot, camera: &Camera3D);
    pub fn draw(&mut self, cx: &mut Cx2d, rect: Rect);
}
```

#### 2.2 Refactor RobotView Widget
**File**: `src/robot_view.rs` (reduce from 1304 to ~400 lines)
**Effort**: 2 days

**Before** (current):
```rust
pub struct RobotView {
    // 25+ fields mixing UI, robot, camera, rendering, animation
}
```

**After** (refactored):
```rust
pub struct RobotView {
    // UI
    #[deref] view: View,
    #[redraw] draw_bg: DrawSkybox,

    // Components
    #[rust] robot: Option<Robot>,
    #[rust] renderer: Option<RobotRenderer>,
    #[rust] camera: CameraController,

    // UI State only
    #[rust] selected_joint: usize,
    #[rust] show_axes: bool,
    #[rust] load_state: LoadState,
}
```

#### 2.3 Fix Viewport Clipping Properly
**Effort**: 1 day

Move clipping logic from shader hack to renderer:
```rust
impl RobotRenderer {
    pub fn set_viewport(&mut self, rect: Rect, window_size: Vec2) {
        self.clip_rect = vec4(rect.pos.x, rect.pos.y,
                              rect.pos.x + rect.size.x,
                              rect.pos.y + rect.size.y);
        self.window_size = vec2(window_size.x, window_size.y);

        // Update all drawers
        for drawer in &mut self.link_drawers {
            drawer.set_draw_clip(self.clip_rect);
            drawer.set_window_size(self.window_size);
        }
    }
}
```

---

## Sprint 3: UX Improvements (Weeks 5-6)

### Goal: Smooth loading, error feedback, better controls

#### 3.1 Loading State UI
**Effort**: 2 days

```rust
pub enum LoadState {
    Empty,
    Loading { progress: f32, current_file: String },
    Ready,
    Failed { error: String, can_retry: bool },
}

// In draw_walk:
match &self.load_state {
    LoadState::Loading { progress, current_file } => {
        self.draw_loading_overlay(cx, *progress, current_file);
    }
    LoadState::Failed { error, can_retry } => {
        self.draw_error_overlay(cx, error, *can_retry);
    }
    _ => { /* normal render */ }
}
```

#### 3.2 Async Robot Loading
**Effort**: 3 days

```rust
// src/robot/loader.rs
pub async fn load_robot_async(
    urdf_path: &Path,
    assets_dir: &Path,
    on_progress: impl Fn(f32, &str),
) -> Result<Robot, RobotError> {
    on_progress(0.0, "Reading URDF...");
    let urdf_content = tokio::fs::read_to_string(urdf_path).await?;

    on_progress(0.1, "Parsing URDF...");
    let urdf = urdf_rs::read_from_string(&urdf_content)?;

    let mesh_count = /* count meshes */;
    for (i, mesh_path) in mesh_paths.iter().enumerate() {
        on_progress(0.1 + 0.9 * (i as f32 / mesh_count as f32),
                    &format!("Loading {}", mesh_path));
        // Load mesh...
    }

    Ok(robot)
}
```

#### 3.3 URDF Validation
**Effort**: 2 days

```rust
// src/robot/validation.rs
pub fn validate_robot(robot: &Robot) -> Result<(), Vec<ValidationWarning>> {
    let mut warnings = Vec::new();

    // Check for cycles in joint graph
    if let Some(cycle) = detect_cycle(&robot.joints) {
        return Err(ValidationError::CycleDetected(cycle));
    }

    // Check joint limits
    for joint in &robot.joints {
        if joint.lower_limit > joint.upper_limit {
            warnings.push(ValidationWarning::InvalidLimits(joint.name.clone()));
        }
    }

    // Check for missing mesh files
    for link in &robot.links {
        if link.mesh_path.is_some() && link.mesh_data.is_none() {
            warnings.push(ValidationWarning::MissingMesh(link.name.clone()));
        }
    }

    if warnings.is_empty() { Ok(()) } else { Err(warnings) }
}
```

#### 3.4 Camera Improvements
**Effort**: 1 day

- [ ] Add smooth camera animation (lerp between states)
- [ ] Add camera presets (front, side, top, isometric)
- [ ] Add "fit to view" auto-framing
- [ ] Improve scroll zoom (exponential, not linear)

---

## Sprint 4: Polish & Performance (Weeks 7-8)

### Goal: Production-ready quality

#### 4.1 Frustum Culling
**Effort**: 2 days

```rust
impl RobotRenderer {
    fn is_link_visible(&self, link: &RobotLink, view_proj: &Mat4) -> bool {
        let bounds = link.world_bounds();
        // Transform AABB corners to clip space
        // Return true if any corner inside frustum
    }

    pub fn draw(&mut self, cx: &mut Cx2d, robot: &Robot, camera: &Camera3D) {
        let view_proj = camera.view_projection_matrix();

        for (link, drawer) in robot.links.iter().zip(&mut self.link_drawers) {
            if !self.is_link_visible(link, &view_proj) {
                continue; // Skip off-screen links
            }
            drawer.draw(cx);
        }
    }
}
```

#### 4.2 Conditional Profiling
**Effort**: 0.5 days

```toml
# Cargo.toml
[features]
default = []
profiling = []
```

```rust
#[cfg(feature = "profiling")]
macro_rules! profile_scope {
    ($name:expr) => { /* timing code */ };
}

#[cfg(not(feature = "profiling"))]
macro_rules! profile_scope {
    ($name:expr) => {}; // no-op
}
```

#### 4.3 Testing
**Effort**: 3 days

```rust
// tests/robot_loading.rs
#[test]
fn test_load_vx300s() {
    let robot = load_robot("data/vx300s/vx300s.urdf", "data/vx300s").unwrap();
    assert_eq!(robot.joint_count(), 7);
}

#[test]
fn test_fk_identity_at_zero() {
    let mut robot = load_robot("data/vx300s/vx300s.urdf", "data/vx300s").unwrap();
    robot.update_fk();
    // End effector should be at known position
}

#[test]
fn test_invalid_urdf_returns_error() {
    let result = load_robot("data/invalid.urdf", "data/");
    assert!(result.is_err());
}
```

#### 4.4 Documentation
**Effort**: 2 days

- [ ] Update ARCHITECTURE.md for new module structure
- [ ] Add API documentation with examples
- [ ] Add README usage examples
- [ ] Document shader clipping approach

---

## File Structure After Refactoring

```
src/
├── lib.rs                 # Public API exports
├── error.rs               # Error types
├── camera/
│   ├── mod.rs
│   ├── camera3d.rs        # Camera3D struct
│   └── controller.rs      # CameraController
├── robot/
│   ├── mod.rs
│   ├── model.rs           # Robot, RobotLink, RobotJoint
│   ├── loader.rs          # URDF + STL loading
│   ├── kinematics.rs      # Forward kinematics
│   └── validation.rs      # URDF validation
├── render/
│   ├── mod.rs
│   ├── mesh.rs            # MeshData
│   ├── geometry.rs        # GeometryMesh3D
│   ├── shaders.rs         # DrawMesh, DrawGrid, DrawSkybox
│   └── renderer.rs        # RobotRenderer
└── ui/
    ├── mod.rs
    └── robot_view.rs      # RobotView widget (~400 lines)
```

---

## Milestones & Deliverables

| Week | Milestone | Deliverable |
|------|-----------|-------------|
| 2 | Sprint 1 Complete | Camera + Robot modules extracted |
| 4 | Sprint 2 Complete | Renderer separated, RobotView slim |
| 6 | Sprint 3 Complete | Async loading, error UI, validation |
| 8 | Sprint 4 Complete | Tests, docs, performance optimized |

---

## Risk Mitigation

| Risk | Mitigation |
|------|------------|
| Makepad API changes | Pin to specific commit, abstract behind traits |
| Breaking existing functionality | Keep old code paths until new ones tested |
| Scope creep | Strict sprint boundaries, defer nice-to-haves |
| Performance regression | Benchmark before/after each sprint |

---

## Success Criteria

### Technical
- [ ] robot_view.rs < 500 lines
- [ ] All modules have < 400 lines per file
- [ ] Test coverage > 50%
- [ ] No blocking I/O on render thread
- [ ] Clipping works without shader hacks

### User Experience
- [ ] Loading shows progress indicator
- [ ] Errors display user-friendly messages
- [ ] Camera controls feel smooth
- [ ] First render < 500ms (with loading indicator)

---

## Quick Wins (Can Do Anytime)

1. **Add camera smoothing** - 2 hours, big UX improvement
2. **Add "Fit to View" button** - 1 hour
3. **Fix joint limit validation** - 30 minutes
4. **Add keyboard shortcuts help** - 30 minutes
5. **Remove hardcoded default path** - 15 minutes

---

## Not In Scope (Future Versions)

- IK (inverse kinematics)
- Physics simulation
- Collision visualization
- Trajectory playback
- VR/AR support
- Multi-robot scenes
- GLTF/OBJ format support

These are v2.0+ features per the roadmap.
