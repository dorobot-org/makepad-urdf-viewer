# makepad-urdf-player - Development Roadmap

**Document Version**: 1.0
**Date**: 2026-01-19
**Status**: Active Development

---

## Overview

This roadmap documents the critical review findings and prioritized improvement plan for the makepad-urdf-player project. The project is assessed at **7.5/10** overall, with significant technical achievements in GPU optimization but architectural debt that limits maintainability and extensibility.

### Current State Summary

| Aspect | Rating | Notes |
|--------|--------|-------|
| **GPU Performance** | 9/10 | Excellent: 99.9% memory reduction (13MB → 64 bytes) |
| **Code Organization** | 6/10 | Monolithic widget, mixed responsibilities |
| **Error Handling** | 5/10 | Silent failures, blocking I/O |
| **Documentation** | 8/10 | Comprehensive ARCHITECTURE.md, lacking usage examples |
| **Production Readiness** | 6/10 | Needs async loading, error UI, validation |
| **Extensibility** | 5/10 | Tight coupling, hard to extend |

---

## Critical Issues Analysis

### Issue 1: Monolithic Widget Design

**Location**: `src/robot_view.rs` (1304 lines)

**Problem**: The `RobotView` struct violates Single Responsibility Principle by handling:
- UI rendering (shader management, viewport)
- Robot data model (Robot, RobotLink, RobotJoint)
- Camera controls (orbital, pan, zoom)
- Animation state and timing
- Input event handling
- Performance profiling

**Impact**:
- Cannot reuse robot rendering without UI components
- Difficult to test robot logic in isolation
- Camera logic cannot be reused across different views
- High cognitive load for maintenance

**Current Structure**:
```
RobotView {
    // UI: 5 fields
    // Robot: 2 fields
    // Camera: 5 fields
    // Animation: 4 fields
    // Input: 2 fields
    // Rendering: 5 fields
    // Debug: 2 fields
}
```

---

### Issue 2: Blocking I/O on Render Thread

**Location**: `src/robot_view.rs:876-885`

**Problem**: Robot initialization happens synchronously in `draw_walk()`:

```rust
fn draw_walk(&mut self, cx: &mut Cx2d, ...) {
    if !self.initialized {
        self.initialized = true;
        self.init_robot(cx.cx);  // BLOCKING FILE I/O
    }
}
```

**Impact**:
- UI freezes during URDF/STL loading
- No progress feedback to user
- Poor experience for large robots

**Invoked Operations**:
- `std::fs::read_to_string()` - URDF file
- `stl_io::read_stl()` - Multiple STL files
- `MeshData::combine()` - CPU mesh processing

---

### Issue 3: Silent Error Handling

**Location**: `src/robot_view.rs:341-344`

**Problem**: Mesh loading failures are logged but execution continues:

```rust
Err(e) => {
    eprintln!("Warning: Failed to load {}: {}", mesh_path, e);
    // Execution continues - link renders as invisible
}
```

**Impact**:
- Missing meshes render as invisible links
- No user-visible error messages
- Difficult to diagnose missing geometry
- May appear as bugs (disappearing robot parts)

---

### Issue 4: No Input Validation

**Location**: `src/robot_view.rs:235-448` (URDF parsing)

**Problem**: URDF data is not validated after loading:

| Validation Type | Status | Risk |
|----------------|--------|------|
| Joint graph cycles | ❌ Missing | Infinite loops in FK |
| Missing link references | ❌ Missing | Panic at runtime |
| Joint limit sanity | ❌ Missing | `lower > upper` possible |
| Mesh file existence | ⚠️ Partial | Silent failure |
| Coordinate system | ❌ Missing | Garbage transforms |

---

### Issue 5: Hardcoded Configuration

**Location**: `src/robot_view.rs:713-716`

**Problem**: Default robot path is hardcoded:

```rust
if self.urdf_path.is_empty() {
    self.urdf_path = "data/vx300s/vx300s.urdf".to_string();
    self.assets_dir = "data/vx300s".to_string();
}
```

**Impact**:
- Breaks when running from different directories
- Cannot be configured at runtime
- Requires code changes for different robots

---

## Architecture Refactoring Plan

### Target Module Structure

```
src/
├── lib.rs              # Public API exports (existing)
├── main.rs             # Example application (existing)
├── robot/
│   ├── mod.rs          # Robot model module
│   ├── model.rs        # Robot, RobotLink, RobotJoint
│   ├── urdf_loader.rs  # URDF parsing, STL loading
│   └── kinematics.rs   # Forward kinematics, validation
├── render/
│   ├── mod.rs          # Rendering module
│   ├── mesh.rs         # MeshData (move from existing)
│   ├── geometry.rs     # GeometryMesh3D (move from existing)
│   ├── shaders.rs      # DrawMesh, DrawGrid (move from existing)
│   └── renderer.rs     # RobotRenderer - GPU rendering only
├── camera/
│   ├── mod.rs          # Camera module
│   ├── camera3d.rs     # Camera3D (extract from robot_view.rs)
│   └── controls.rs     # Orbit/pan/zoom controls
├── ui/
│   ├── mod.rs          # UI module
│   └── robot_view.rs   # RobotView widget (refactored)
└── utils/
    ├── mod.rs          # Utilities
    ├── profiling.rs    # ProfilingStats (extract)
    └── errors.rs       # Error types
```

---

### Phase 1: Extract Robot Model (P0)

**Goal**: Separate robot data model from UI widget

**Tasks**:

1. **Create `src/robot/model.rs`**
   - Move `Robot`, `RobotLink`, `RobotJoint` structs
   - Add `pub(crate)` visibility for internal use
   - Keep `Robot::from_urdf()` but add validation

2. **Create `src/robot/kinematics.rs`**
   - Extract `update_forward_kinematics()` from Robot
   - Add `validate_joint_graph()` method
   - Add `validate_joint_limits()` method

3. **Create `src/robot/urdf_loader.rs`**
   - Extract URDF parsing logic
   - Return `Result<Robot, RobotLoadError>`
   - Add async loading support

**API Design**:

```rust
// src/robot/mod.rs
pub use model::{Robot, RobotLink, RobotJoint};
pub use urdf_loader::{load_robot, load_robot_async, RobotLoadError};
pub use kinematics::ForwardKinematics;

// src/robot/urdf_loader.rs
pub async fn load_robot_async(
    urdf_path: &Path,
    assets_base: &Path,
    progress: impl Fn(f32)
) -> Result<Robot, RobotLoadError> {
    // Async file loading with progress callbacks
}

// src/robot/kinematics.rs
pub trait ForwardKinematics {
    fn update_fk(&mut self);
    fn validate(&self) -> Result<(), ValidationError>;
}
```

---

### Phase 2: Extract Camera Component (P0)

**Goal**: Make camera logic reusable across different views

**Tasks**:

1. **Create `src/camera/camera3d.rs`**
   - Extract existing `Camera3D` struct
   - Add `from_orbital()` factory method
   - Add `look_at()` method for target changes

2. **Create `src/camera/controls.rs`**
   - Extract orbit/pan/zoom logic from `RobotView`
   - Create `CameraController` component
   - Support mouse and keyboard inputs

**API Design**:

```rust
// src/camera/camera3d.rs
impl Camera3D {
    pub fn from_orbital(distance: f32, yaw: f32, pitch: f32, target: Vec3) -> Self;
    pub fn look_at(&mut self, target: Vec3);
    pub fn orbit(&mut self, delta_yaw: f32, delta_pitch: f32);
    pub fn pan(&mut self, delta_x: f32, delta_y: f32);
    pub fn zoom(&mut self, factor: f32);
}

// src/camera/controls.rs
pub struct CameraController {
    camera: Camera3D,
    state: ControlState,
}

impl CameraController {
    pub fn handle_mouse_drag(&mut self, delta: DVec2, modifiers: KeyModifiers);
    pub fn handle_scroll(&mut self, delta: f64);
    pub fn handle_keyboard(&mut self, key: KeyCode);
}
```

---

### Phase 3: Create Renderer Module (P1)

**Goal**: Separate GPU rendering from UI widget

**Tasks**:

1. **Move `MeshData` to `src/render/mesh.rs`**
2. **Move `GeometryMesh3D` to `src/render/geometry.rs`**
3. **Create `src/render/renderer.rs`**

```rust
// src/render/renderer.rs
pub struct RobotRenderer {
    link_drawers: Vec<DrawMesh>,
    axis_drawers: Vec<DrawMesh>,
    grid_drawer: Option<DrawMesh>,
}

impl RobotRenderer {
    pub fn new(robot: &Robot, template: &DrawMesh) -> Result<Self, RenderInitError>;
    pub fn update_transforms(&mut self, robot: &Robot, camera: &Camera3D);
    pub fn render(&mut self, cx: &mut Cx2d, viewport: Rect);
    pub fn set_viewport_clip(&mut self, clip: Vec4, window: Vec2);
}
```

---

### Phase 4: Refactor RobotView Widget (P1)

**Goal**: RobotView becomes a thin UI wrapper

**After Refactoring**:

```rust
// src/ui/robot_view.rs
#[derive(Live, LiveHook, Widget)]
pub struct RobotView {
    // UI only
    #[redraw] draw_bg: DrawSkybox,
    #[walk] walk: Walk,
    #[layout] layout: Layout,

    // Components (not inline)
    #[rust] renderer: RobotRenderer,
    #[rust] camera_ctrl: CameraController,
    #[rust] robot: Option<Robot>,

    // UI state only
    #[rust] selected_joint: usize,
    #[rust] show_joint_axes: bool,
    #[rust] show_world_axes: bool,
    #[rust] load_state: LoadState,
}

// Reduced from 25+ fields to ~10 fields
// Each component handles its own responsibilities
```

---

## Performance Improvements

### P0: Async Robot Loading

**Current**:
```
User selects robot
    ↓
[UI FREEZES] - blocking file I/O
    ↓
Robot appears (~1-3 seconds later)
```

**Target**:
```
User selects robot
    ↓
[Show loading spinner with progress]
    ↓
Robot loads incrementally
    ↓
[Smooth transition to loaded state]
```

**Implementation**:

```rust
// src/robot/urdf_loader.rs
pub enum LoadState {
    Empty,
    Loading {
        robot_name: String,
        progress: f32,
        current_file: String,
    },
    Ready(Robot),
    Failed { robot_name: String, error: String },
}

// src/ui/robot_view.rs
#[rust] load_state: LoadState,

fn draw_walk(&mut self, cx: &mut Cx2d, ...) {
    match &self.load_state {
        LoadState::Loading { progress, current_file, .. } => {
            // Draw loading indicator
            self.draw_loading_indicator(cx, *progress, current_file);
        }
        LoadState::Failed { error, .. } => {
            // Draw error message with retry button
            self.draw_error_message(cx, error);
        }
        LoadState::Ready(robot) => {
            // Normal rendering
        }
    }
}
```

---

### P1: Frustum Culling

**Goal**: Skip rendering off-screen links

**Implementation**:

```rust
// src/render/renderer.rs
fn is_visible(bounds: (Vec3, Vec3), view_proj: Mat4) -> bool {
    // Transform bounds to clip space
    let min_proj = view_proj * bounds.0.extend(1.0);
    let max_proj = view_proj * bounds.1.extend(1.0);

    // Check if any corner is inside view frustum
    // (-1, -1, -1) to (1, 1, 1) in NDC
    let corners = [
        (min_proj.x, min_proj.y, min_proj.z),
        (max_proj.x, min_proj.y, min_proj.z),
        // ... all 8 corners
    ];

    corners.iter().any(|&(x, y, z)| {
        x >= -1.0 && x <= 1.0 && y >= -1.0 && y <= 1.0 && z >= -1.0 && z <= 1.0
    })
}

pub fn render(&mut self, cx: &mut Cx2d, robot: &Robot, camera: &Camera3D) {
    let view_proj = camera.view_matrix() * camera.projection_matrix();

    for (link, drawer) in robot.links.iter().zip(self.link_drawers.iter_mut()) {
        if !is_visible(link.bounds, view_proj) {
            continue; // Skip off-screen links
        }
        drawer.draw(cx);
    }
}
```

**Expected Benefit**: 30-50% reduction in draw calls for typical views

---

### P2: Conditional Profiling

**Current**: Profiling always enabled

**Target**: Only enable when needed

```rust
// Cargo.toml
[features]
default = []
profiling = []

// src/utils/profiling.rs
#[cfg(feature = "profiling")]
pub fn record_frame(...);

#[cfg(not(feature = "profiling"))]
pub fn record_frame(...) {}  // No-op

// Usage
cargo run --features profiling  # Enable profiling
cargo run                        # Production build (no overhead)
```

---

## Error Handling Improvements

### Error Type Hierarchy

```rust
// src/utils/errors.rs
#[derive(Debug, thiserror::Error)]
pub enum RobotError {
    #[error("Failed to read URDF file '{path}': {source}")]
    UrdfReadError { path: String, #[source] std::io::Error },

    #[error("Failed to parse URDF: {0}")]
    UrdfParseError(#[from] urdf_rs::Error),

    #[error("Failed to load mesh '{path}': {reason}")]
    MeshLoadError { path: String, reason: String },

    #[error("Invalid URDF: {0}")]
    ValidationError(String),

    #[error("Joint graph contains cycle: {0}")]
    CycleError(String),

    #[error("Joint '{joint}' references missing link '{link}'")]
    MissingLinkError { joint: String, link: String },
}

// User-friendly error messages
impl RobotError {
    pub fn user_message(&self) -> String {
        match self {
            RobotError::MeshLoadError { path, .. } => {
                format!("Could not load robot part: {}\n\nPlease check the file exists and is a valid STL file.", path)
            }
            // ... other cases
        }
    }

    pub fn is_recoverable(&self) -> bool {
        matches!(self, RobotError::MeshLoadError { .. })
    }
}
```

---

### UI Error Display

```rust
// src/ui/robot_view.rs

#[derive(Live, Widget)]
pub struct ErrorDisplay {
    #[live] error_message: String,
    #[live] show_retry: bool,
}

impl ErrorDisplay {
    pub fn show_error(cx: &mut Cx, error: &RobotError) {
        let ui_error = ErrorDisplay {
            error_message: error.user_message(),
            show_retry: error.is_recoverable(),
            // ...
        };
        // Display error overlay
    }
}
```

---

## Missing Feature Roadmap

### Short Term (0-3 months)

| Feature | Priority | Est. Effort | Status |
|---------|----------|-------------|--------|
| Async robot loading | P0 | 2 weeks | ❌ Not started |
| Error UI overlay | P0 | 1 week | ❌ Not started |
| URDF validation | P0 | 1 week | ❌ Not started |
| Architecture refactor | P1 | 4 weeks | ❌ Not started |
| Frustum culling | P1 | 3 days | ❌ Not started |

### Medium Term (3-6 months)

| Feature | Priority | Est. Effort | Status |
|---------|----------|-------------|--------|
| OBJ format support | P2 | 1 week | ❌ Not started |
| GLTF format support | P2 | 2 weeks | ❌ Not started |
| Pose save/load | P2 | 1 week | ❌ Not started |
| Undo/Redo system | P2 | 2 weeks | ❌ Not started |
| Multi-robot instances | P2 | 3 weeks | ❌ Not started |

### Long Term (6-12 months)

| Feature | Priority | Est. Effort | Status |
|---------|----------|-------------|--------|
| Collision mesh visualization | P3 | 2 weeks | ❌ Not started |
| Trajectory playback | P3 | 3 weeks | ❌ Not started |
| IK (inverse kinematics) | P3 | 6 weeks | ❌ Not started |
| Physics integration | P3 | 8 weeks | ❌ Not started |
| VR/AR support | P3 | 12 weeks | ❌ Not started |

---

## Testing Strategy

### Unit Tests

```rust
// src/robot/kinematics.rs Tests
#[cfg(test)]
mod tests {
    #[test]
    fn test_fk_simple_chain() {
        let robot = Robot::simple_chain(3); // 3 links
        robot.set_joint_angle(0, PI/2);
        robot.update_fk();

        // Verify end effector position
        let end_effector = robot.link_transforms[2];
        assert!((end_effector.w_axis.x - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fk_cycle_detection() {
        let robot = Robot::with_cycle(); // Invalid robot
        let result = robot.validate();
        assert!(matches!(result, Err(ValidationError::CycleError(_))));
    }
}
```

### Integration Tests

```rust
// tests/robot_loading.rs
#[test]
fn test_load_vx300s() {
    let robot = Robot::from_urdf("data/vx300s/vx300s.urdf", "data/vx300s")
        .expect("Failed to load VX300s");

    assert_eq!(robot.num_joints(), 7);
    assert!(!robot.links.iter().any(|l| l.mesh_data.is_none()));
}

#[test]
fn test_load_missing_mesh_fails() {
    let result = Robot::from_urdf("data/invalid/missing.urdf", "data/invalid");
    assert!(result.is_err());
}
```

---

## Migration Guide

### For Existing Users

**Before** (Monolithic API):

```rust
// Old way - everything in RobotView
let robot_view = ui.robot_view(id!(robot));
robot_view.load_robot(cx, "data/robot.urdf", "data/meshes");
robot_view.set_joint_angle(cx, 0, 0.5);
```

**After** (Modular API):

```rust
// New way - separate concerns
use makepad_urdf_player::robot::{Robot, load_robot_async};
use makepad_urdf_player::camera::CameraController;
use makepad_urdf_player::render::RobotRenderer;

// Load robot asynchronously
let robot = load_robot_async("data/robot.urdf", "data/meshes", |p| {
    println!("Loading: {}%", p * 100.0);
}).await?;

// Create renderer
let renderer = RobotRenderer::new(&robot, &draw_template)?;

// Create camera controller
let mut camera = CameraController::new(Camera3D::from_orbital(3.0, 0.5, 0.3, Vec3::ZERO));

// Use in widget
let robot_view = ui.robot_view(id!(robot));
robot_view.set_renderer(renderer);
robot_view.set_camera(camera);
```

---

## Success Metrics

### Technical Metrics

| Metric | Current | Target | Deadline |
|--------|---------|--------|----------|
| First-frame freeze | ~2 seconds | <100ms | Q1 2026 |
| Lines per file | 1304 (max) | <500 | Q2 2026 |
| Test coverage | 0% | >60% | Q2 2026 |
| Compilation time | N/A | <30s | Q3 2026 |

### User Experience Metrics

| Metric | Current | Target |
|--------|---------|--------|
| Time to first robot | 2-3s | <500ms |
| Error recovery | Restart required | In-app retry |
| Configuration | Code changes | Runtime UI |
| Documentation | ARCHITECTURE only | Full API docs |

---

## Dependencies & Risks

### External Dependencies

| Crate | Version | Risk | Mitigation |
|-------|---------|------|------------|
| `makepad-widgets` | git (rik branch) | **HIGH** - API changes | Pin to specific commit, track upstream |
| `urdf-rs` | 0.9 | Medium - Last update 2023 | Fork if unmaintained |
| `glam` | 0.29 | Low - Active development | Track stable releases |
| `stl_io` | 0.7 | Medium - Limited features | Add OBJ/GLTF support |

### Technical Risks

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| Makepad API breaks | High | Medium | Abstract behind traits |
| Metal shader issues | High | Low | Test on Apple Silicon |
| Performance regression | Medium | Low | Benchmark suite |
| Contributor burnout | High | Medium | Clear tasks, good docs |

---

## Contribution Guidelines

### Code Standards

1. **Maximum file length**: 500 lines
2. **Function complexity**: Cyclomatic complexity < 10
3. **Error handling**: Use `Result<T, E>`, never silent failures
4. **Documentation**: All public APIs must have doc examples
5. **Testing**: New features require tests

### Pull Request Process

1. Create issue for feature/bug
2. Fork and branch from `develop`
3. Write tests first (TDD)
4. Implement with `cargo clippy` clean
5. Update documentation
6. PR with description and tests passing

---

## Release Plan

### v0.2.0 - Architecture Refactor (Q1 2026)

- [ ] Extract robot model module
- [ ] Extract camera module
- [ ] Create renderer module
- [ ] Refactor RobotView widget
- [ ] Update documentation

### v0.3.0 - UX Improvements (Q2 2026)

- [ ] Async robot loading
- [ ] Error UI overlay
- [ ] Progress indicators
- [ ] URDF validation
- [ ] Configuration UI

### v0.4.0 - Performance (Q3 2026)

- [ ] Frustum culling
- [ ] LOD system
- [ ] Instanced rendering
- [ ] Profiling improvements

### v1.0.0 - Production Ready (Q4 2026)

- [ ] Complete test suite
- [ ] API documentation
- [ ] Integration examples
- [ ] Multi-robot support
- [ ] Pose save/load

---

## Appendix

### A. File-by-File Action Items

| File | Action | Priority |
|------|--------|----------|
| `src/robot_view.rs` | Split into 4 modules | P0 |
| `src/mesh.rs` | Move to render/ module | P1 |
| `src/lib.rs` | Update exports | P1 |
| `ARCHITECTURE.md` | Update for new structure | P2 |
| `README.md` | Add usage examples | P2 |

### B. Performance Profiling Results

**Baseline (Current)**:
- Frame time: 0.01ms CPU-side
- Memory per frame: 64 bytes
- Transform time: 0.000ms
- First frame freeze: ~2000ms

**Target (After Optimizations)**:
- Frame time: <0.01ms CPU-side
- Memory per frame: 64 bytes (maintained)
- Transform time: 0.000ms (maintained)
- First frame freeze: <100ms

### C. Related Projects

| Project | URL | Relevance |
|---------|-----|-----------|
| Makepad | https://github.com/makepad/makepad | UI framework |
| urdf-rs | https://github.com/openrr/urdf-rs | URDF parsing |
| Rerun | https://github.com/rerun-io/rerun | Visualization reference |
| makepad-d3 | https://github.com/... | Rendering patterns |

---

**Document Maintainer**: Development Team
**Last Updated**: 2026-01-19
**Next Review**: 2026-02-19
