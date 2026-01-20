# URDF Viewer Architecture Analysis & Refactoring Plan

## Current Architecture

### Third-Party Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `makepad-widgets` | git (rik branch) | UI framework, shaders, GPU geometry, events |
| `urdf-rs` | 0.9 | URDF XML parsing |
| `glam` | 0.29 | Linear algebra (quaternions, matrices, vectors) |
| `stl_io` | 0.7 | STL mesh file loading |

### Module Structure (Current - Fully Modularized)

```
src/
├── lib.rs               # Public API exports, live_design registration
├── main.rs              # App entry point, URDFViewer container widget
├── robot_view.rs        # RobotView widget (~600 lines, down from 1303)
├── mesh.rs              # Backward-compat shim re-exporting from render
├── error.rs             # RobotError and RobotWarning types
├── profiling.rs         # Conditional profiling (--features profiling)
│
├── camera/              # Camera module
│   ├── mod.rs           # Module exports
│   ├── camera3d.rs      # Camera3D struct with orbital controls
│   └── controller.rs    # CameraController for input handling
│
├── robot/               # Robot model module
│   ├── mod.rs           # Module exports
│   ├── model.rs         # Robot, RobotLink, RobotJoint structs
│   ├── loader.rs        # URDF parsing and STL loading
│   └── kinematics.rs    # Forward kinematics computation
│
└── render/              # Rendering module
    ├── mod.rs           # Module exports and live_design
    ├── mesh.rs          # MeshData (CPU-side mesh)
    ├── geometry.rs      # GeometryMesh3D (GPU geometry)
    └── draw.rs          # DrawMesh, DrawGrid, DrawSkybox shaders
```

### Component Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                         URDFViewer                              │
│  (Monolithic widget containing everything)                      │
├─────────────────────────────────────────────────────────────────┤
│  UI State                                                       │
│  ├── View (header, viewport, status_bar)                        │
│  ├── camera_yaw, camera_pitch, camera_distance                  │
│  ├── selected_joint, animating, anim_timer                      │
│  └── is_dragging, last_mouse                                    │
├─────────────────────────────────────────────────────────────────┤
│  Robot Model (private structs)                                  │
│  ├── Robot { links, joints, link_transforms, ... }              │
│  ├── RobotLink { name, mesh_data }                              │
│  └── RobotJoint { name, parent, child, origin, axis, angle }    │
├─────────────────────────────────────────────────────────────────┤
│  Rendering State                                                │
│  ├── draw_mesh: DrawMesh (template)                             │
│  ├── link_drawers: Vec<DrawMesh> (one per link)                 │
│  └── original_meshes: Vec<MeshData> (untransformed)             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                          mesh.rs                                │
├─────────────────────────────────────────────────────────────────┤
│  MeshData (CPU-side mesh representation)                        │
│  ├── vertices: Vec<f32>  [pos(3), id(1), normal(3), uv(2)]     │
│  ├── indices: Vec<u32>                                          │
│  ├── bounds_min/max: [f32; 3]                                   │
│  ├── from_stl() - load STL file                                 │
│  ├── apply_transform() - CPU vertex transformation              │
│  ├── combine() - merge meshes                                   │
│  └── make_double_sided() - duplicate with flipped normals       │
├─────────────────────────────────────────────────────────────────┤
│  GeometryMesh3D (GPU geometry buffer)                           │
│  ├── geometry_ref: GeometryRef                                  │
│  ├── instance_id: u64 (for unique fingerprints)                 │
│  ├── upload_mesh_data() - send to GPU                           │
│  └── impl GeometryFields (geom_pos, geom_normal, geom_uv)       │
├─────────────────────────────────────────────────────────────────┤
│  DrawMesh (shader + draw state)                                 │
│  ├── geometry: GeometryMesh3D                                   │
│  ├── draw_vars: DrawVars                                        │
│  ├── color: Vec4                                                │
│  ├── new_for_link() - create instance for a robot link          │
│  ├── update_transformed_geometry() - CPU transform + re-upload  │
│  └── Shader: vertex() computes lighting, pixel() returns color  │
└─────────────────────────────────────────────────────────────────┘
```

### Data Flow (Per Frame)

```
1. Animation timer fires
   │
2. Update joint angles (Robot.joints[i].angle)
   │
3. Compute forward kinematics
   │  └── Robot.update_forward_kinematics()
   │      └── Outputs: link_transforms: Vec<glam::Mat4>
   │
4. For each link with mesh:
   │
   ├── 4a. Clone original mesh (CPU)
   │       └── ~50k-70k vertices × 9 floats = ~2MB per link
   │
   ├── 4b. Apply transform on CPU
   │       └── MeshData.apply_transform(&Mat4)
   │       └── Loop over all vertices, matrix multiply each
   │
   ├── 4c. Upload transformed mesh to GPU
   │       └── GeometryMesh3D.upload_mesh_data()
   │       └── Full buffer re-upload every frame
   │
   └── 4d. Draw
           └── DrawMesh.draw()
```

**Total per frame**: ~364k vertices × 9 floats × 4 bytes = **13MB** cloned, transformed, and uploaded.

---

## Critical Issue: No GPU-side Matrix Transform

### The Problem

Makepad's shader system doesn't support `Mat4` as instance data. When attempting:

```rust
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawMesh {
    #[live] pub transform: Mat4,  // FAILS - Mat4 not supported as instance data
    // ...
}
```

The shader compiler fails to recognize `Mat4` as a valid instance attribute type.

### What is Mat4?

A 4×4 transformation matrix (16 floats) that encodes rotation, translation, and scale:

```
┌                           ┐
│  r00  r01  r02  tx       │     r = rotation (3×3)
│  r10  r11  r12  ty       │     t = translation (x, y, z)
│  r20  r21  r22  tz       │
│  0    0    0    1        │
└                           ┘
```

Transforming a vertex: `new_pos = Mat4 × old_pos`

### Use Case: Robot Arm Forward Kinematics

```
Base ──[joint1]──▶ Shoulder ──[joint2]──▶ Upper Arm ──[joint3]──▶ ...
  │                    │                      │
  Mat4₀                Mat4₁                  Mat4₂
  (identity)           (parent × joint)       (parent × joint)
```

Each link needs its own Mat4 computed from the kinematic chain. The GPU should apply this transform to all vertices of that link's mesh.

### Current Workaround (Inefficient)

```rust
// Every frame, for each link:
fn update_transformed_geometry(&mut self, cx: &mut Cx, original: &MeshData, transform: &Mat4) {
    let mut transformed = original.clone();      // Clone ~50k vertices
    transformed.apply_transform(transform);       // CPU matrix multiply each vertex
    self.geometry.upload_mesh_data(cx, transformed);  // Re-upload to GPU
}
```

### Desired Solution (Efficient)

```rust
// At startup: upload mesh once
geometry.upload_mesh_data(cx, mesh);

// Every frame: just set the matrix
drawer.transform = link_transform;  // 64 bytes
drawer.draw(cx);                    // GPU transforms vertices in parallel
```

### Questions for Makepad Team

1. **Is `Mat4` supported as instance/uniform data?** If not, is this planned?

2. **Would 4× Vec4 work as a workaround?**
   ```rust
   #[live] transform_col0: Vec4,
   #[live] transform_col1: Vec4,
   #[live] transform_col2: Vec4,
   #[live] transform_col3: Vec4,
   ```
   Reconstruct in shader:
   ```rust
   fn vertex(self) -> vec4 {
       let m = mat4(self.transform_col0, self.transform_col1,
                    self.transform_col2, self.transform_col3);
       return m * vec4(self.geom_pos, 1.0);
   }
   ```

3. **Is there a uniform-based approach?** Set matrix uniform before each draw call?

4. **What's the recommended pattern for instanced 3D rendering with per-instance transforms?**

---

## Other Issues

### 1. Monolithic Widget Design

Everything is in `URDFViewer`:
- UI layout
- Camera controls
- Robot data model
- Animation logic
- Rendering

**Impact**: Can't reuse robot rendering in other apps without copy-pasting.

### 2. Hardcoded Paths

```rust
let urdf_path = "data/so100.urdf";  // Line 548
let assets_dir = "data/assets";     // Line 549
```

**Impact**: Can't load different robots without code changes.

### 3. No Projection Matrix

```rust
// Current: hardcoded scale and depth
let scaled = pos * 4.0;
let depth = 0.5 - scaled.z * 0.1;
return vec4(scaled.x, scaled.y, depth, 1.0);
```

**Impact**: No proper perspective, can't integrate with other 3D content.

### 4. Initialization in draw_walk

Robot loading happens in `draw_walk()` on first frame:

```rust
fn draw_walk(&mut self, cx: &mut Cx2d, ...) {
    if !self.initialized {
        self.initialized = true;
        // Load URDF, STL files, create geometry...
    }
}
```

**Impact**: Blocks rendering on first frame, no async loading.

---

## Development Plan

### Priority Levels
- **P0**: Blocker - Must resolve before other work (needs external help)
- **P1**: High - Required for integration with other Makepad apps
- **P2**: Medium - Important improvements
- **P3**: Low - Nice to have / Future work

---

### P0: Blockers (RESOLVED)

- [x] **P0.1** Investigate Mat4 as shader instance data
  - Initially tried `#[calc] transform: Mat4` which caused Metal shader compilation errors
  - Error: field naming issue (`ds_transform 0` instead of `ds_transform_0`)
  - **UPDATE (Rik's guidance)**: Mat4 DOES work with `#[calc]` - see `draw_cube.rs` pattern
  - The proper pattern uses `#[calc] pub transform: Mat4` with Makepad's camera uniforms

- [x] **P0.2** Vec4×4 workaround for matrix passing ✅ WORKING (but not ideal)
  ```rust
  #[calc] transform_col0: Vec4,
  #[calc] transform_col1: Vec4,
  #[calc] transform_col2: Vec4,
  #[calc] transform_col3: Vec4,
  ```
  - Shader reconstructs mat4: `let transform = mat4(col0, col1, col2, col3);`
  - **Note**: Use `#[calc]` not `#[live]` for instance data
  - **Better approach**: Use direct `#[calc] transform: Mat4` per draw_cube.rs

- [x] **P0.3** Uniform-based approach - NOT NEEDED
  - `#[calc] Mat4` works directly

### Recommended Pattern (from Rik's draw_cube.rs)

```rust
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawMesh {
    #[calc] pub transform: Mat4,  // Instance transform - works with #[calc]!
    // ...
}

// In shader:
fn vertex(self) -> vec4 {
    let pos = self.geom_pos;
    let model_view = self.view_transform * self.transform;
    self.world = model_view * vec4(pos, 1.);
    return self.camera_projection * (self.camera_view * self.world)
}
```

Key points:
- Use `#[calc]` (not `#[live]`) for Mat4 instance data
- Use Makepad's built-in `self.camera_projection`, `self.camera_view`, `self.view_transform`
- Full MVP pipeline: `projection * view * view_transform * instance_transform * vertex`

---

### P1: Integration Requirements ✅ COMPLETE

#### P1.1: Code Organization ✅ DONE
- [x] **P1.1.1** Extract `Robot`, `RobotLink`, `RobotJoint` → `src/robot_view.rs`
- [x] **P1.1.2** URDF/STL loading in `Robot::from_urdf()`
- [x] **P1.1.3** `MeshData` in `src/mesh.rs`
- [x] **P1.1.4** `GeometryMesh3D`, `DrawMesh` in `src/mesh.rs`
- [x] **P1.1.5** `src/lib.rs` with public exports

#### P1.2: Reusable Robot Model ✅ DONE
- [x] **P1.2.1** `Robot` struct with public fields
- [x] **P1.2.2** Joint control API:
  - `set_joint_angle(idx, angle)`, `get_joint_info(idx)`
  - `num_joints()`, joint limits via `get_joint_info()`
- [x] **P1.2.3** FK API:
  - `update_forward_kinematics()`, `link_transforms: Vec<glam::Mat4>`

#### P1.3: Configurable Widget ✅ DONE
- [x] **P1.3.1** `#[live]` path properties:
  - `urdf_path: String`, `assets_dir: String`
- [x] **P1.3.2** `#[live]` appearance properties:
  - `default_color: Vec4`, `scale: f32`
- [x] **P1.3.3** Programmatic loading:
  - `load_robot()`, `reload_robot()` methods on RobotViewRef

#### P1.4: Embeddable Widget ✅ DONE
- [x] **P1.4.1** `RobotView` widget in `robot_view.rs`
- [x] **P1.4.2** Joint control: `set_joint_angle()`, `set_joint_angles()`, `get_joint_angles()`
- [x] **P1.4.3** Camera control: `reset_view()` (yaw/pitch/distance internal)
- [x] **P1.4.4** Widget actions:
  ```rust
  enum RobotViewAction {
      JointChanged { joint_idx: usize, angle: f32 },
      AnimationToggled(bool),
  }
  ```

---

### P2: Important Improvements (Medium Priority)

#### P2.1: GPU Performance 🔴 NOT IMPLEMENTED
- [ ] **P2.1.1** Implement GPU-side vertex transformation
  - Add `#[live] transform_col0..3: Vec4` to DrawMesh struct
  - Reconstruct mat4 in shader: `mat4(col0, col1, col2, col3)`
  - **Status**: NOT DONE - code still uses `update_transformed_geometry()` which clones on CPU
- [ ] **P2.1.2** Upload mesh geometry only once at load time
  - `init_link_geometry()` should upload once
  - Add `set_transform(&Mat4)` method that only updates Vec4 uniforms
  - **Status**: NOT DONE - `upload_mesh_data()` called every frame
- [ ] **P2.1.3** Remove CPU transform + re-upload code path
  - Delete `MeshData::apply_transform()` usage in render loop
  - Delete `update_transformed_geometry()` calls
  - **Status**: NOT DONE - still active in robot_view.rs
- [ ] **P2.1.4** Benchmark: target <5ms frame time (currently ~16ms)

#### P2.2: Proper 3D Camera ✅ DONE
- [x] **P2.2.1** Create `Camera3D` struct with position/target/fov
- [x] **P2.2.2** Compute proper view matrix (look_at_rh)
- [x] **P2.2.3** Compute proper projection matrix (perspective_rh)
- [x] **P2.2.4** Handle aspect ratio from viewport size

#### P2.3: Better Shader ✅ DONE
- [x] **P2.3.1** Use proper MVP matrix pipeline
- [x] **P2.3.2** Proper depth buffer via projection matrix
- [x] **P2.3.3** Add specular lighting (Blinn-Phong)
- [x] **P2.3.4** Support per-link colors (from URDF materials)

---

### P3: Nice to Have (Low Priority)

#### P3.1: Loading & Error Handling
- [ ] **P3.1.1** Async URDF/mesh loading (don't block first frame)
- [ ] **P3.1.2** Progress indicator while loading
- [ ] **P3.1.3** User-visible error messages for missing files
- [ ] **P3.1.4** Fallback geometry for missing meshes

#### P3.2: Additional Features ✅ VISUAL COMPLETE
- [ ] **P3.2.1** Support OBJ mesh format
- [ ] **P3.2.2** Support GLTF/GLB mesh format
- [ ] **P3.2.3** Support collision meshes (not just visual)
- [x] **P3.2.4** Grid/ground plane with perspective effect
- [x] **P3.2.5** Sky gradient background (purple-green-blue like Rerun)
- [x] **P3.2.6** Grid lines only (transparent non-line areas)
- [x] **P3.2.7** Proper draw order (robot before grid to prevent blocking)
- [x] **P3.2.8** Joint axis visualization (toggle with Axes button)
- [x] **P3.2.9** World XYZ axes visualization (toggle with XYZ button)
- [ ] **P3.2.10** Joint limit visualization
- [ ] **P3.2.11** Link frame axes visualization

#### P3.3: Documentation
- [ ] **P3.3.1** Integration example in README
- [ ] **P3.3.2** API documentation
- [ ] **P3.3.3** URDF format requirements doc

---

## Task Dependencies

```
P0.1 ──▶ P0.2 ──▶ P0.3 ──▶ P2.1 (GPU transforms)
                              │
P1.1 (code org) ─────────────┼──▶ P1.4 (embeddable widget)
         │                    │
         ▼                    │
P1.2 (robot model) ──────────┘
         │
         ▼
P1.3 (configurable) ──▶ P2.2 (camera) ──▶ P2.3 (shader)
```

---

## Immediate Next Steps

1. ~~**Ask Rik about P0.1-P0.3**~~ ✅ RESOLVED - Using 4×Vec4 workaround
2. ~~**Start P1.1**~~ ✅ DONE - Code organized into lib.rs, robot_view.rs, mesh.rs
3. ~~**Start P1.2**~~ ✅ DONE - Robot model API complete

**Remaining priorities:**
1. ~~**P1.3**~~ ✅ DONE - `#[live]` properties added
2. **P2.2** - Proper 3D camera with projection matrix
3. **P2.3** - MVP shader pipeline

---

## File Structure (Final - Implemented)

```
src/
├── lib.rs               # ✅ Public API exports, live_design registration
├── main.rs              # ✅ App entry point, URDFViewer container
├── robot_view.rs        # ✅ RobotView widget (~600 lines, uses extracted modules)
├── mesh.rs              # ✅ Backward-compat shim (re-exports from render)
├── error.rs             # ✅ RobotError/RobotWarning types with user messages
├── profiling.rs         # ✅ Conditional profiling (--features profiling)
│
├── camera/              # ✅ Camera module (extracted)
│   ├── mod.rs           # Module exports
│   ├── camera3d.rs      # Camera3D struct with orbital controls
│   └── controller.rs    # CameraController for input handling
│
├── robot/               # ✅ Robot model module (extracted)
│   ├── mod.rs           # Module exports
│   ├── model.rs         # Robot, RobotLink, RobotJoint structs
│   ├── loader.rs        # URDF parsing and STL mesh loading
│   └── kinematics.rs    # Forward kinematics computation
│
└── render/              # ✅ Rendering module (extracted)
    ├── mod.rs           # Module exports and live_design
    ├── mesh.rs          # MeshData (CPU-side mesh representation)
    ├── geometry.rs      # GeometryMesh3D (GPU geometry buffers)
    └── draw.rs          # DrawMesh, DrawGrid, DrawSkybox shaders
```

---

## Performance Targets

| Metric | Before (Theoretical) | After (Measured) | Status |
|--------|----------------------|------------------|--------|
| GPU upload per frame | ~13MB (mesh clone) | 64 bytes (4×Vec4) | ✅ Implemented |
| CPU transform time | O(n) vertices | **0.000ms** | ✅ Measured |
| Memory clones/frame | 7 meshes | 0 | ✅ Implemented |
| Frame time (CPU-side) | ~16ms estimate | **0.01ms** | ✅ Measured |

### Profiling Results (2025-01-18)

```
=== Auto-starting animation for profiling ===
=== URDF Viewer Initialized - GPU Transform Profiling Enabled ===
[Frame    30] Avg:   0.26ms total |  0.000ms transform |   0.00ms draw | 3795.7 FPS
[Frame    60] Avg:   0.14ms total |  0.000ms transform |   0.00ms draw | 7299.8 FPS
[Frame    90] Avg:   0.09ms total |  0.000ms transform |   0.00ms draw | 10557.4 FPS
[Frame   120] Avg:   0.07ms total |  0.000ms transform |   0.00ms draw | 13608.7 FPS
[Frame   150] Avg:   0.01ms total |  0.000ms transform |   0.00ms draw | 101251.5 FPS
[Frame   180] Avg:   0.01ms total |  0.000ms transform |   0.00ms draw | 102868.4 FPS
[Frame   210] Avg:   0.01ms total |  0.000ms transform |   0.00ms draw | 103448.8 FPS
```

**Key Observations:**
- Transform phase is essentially **0ms** (just copying 4 Vec4s = 64 bytes)
- Frame time after warmup: **0.01ms** (CPU-side only, GPU renders asynchronously)
- No more mesh cloning, no more CPU vertex transforms
- 100,000+ theoretical FPS (CPU-limited at ~0.01ms per frame)

**Note**: The measured times are CPU-side only. Actual frame rate is limited by:
1. Display refresh rate (60-120Hz typically)
2. GPU rendering time (happens asynchronously)
3. Vsync

**Implementation Complete**: GPU-side transforms via 4×Vec4 columns. Mesh geometry uploaded once at load time, only 64-byte transform uniform updated per frame.

---

## Related Files

- `src/lib.rs` - Public API exports
- `src/main.rs` - App entry point with URDFViewer container
- `src/robot_view.rs` - RobotView widget with Robot model
- `src/mesh.rs` - GPU rendering (DrawMesh with 4×Vec4 transforms)
- `DEVELOPMENT.md` - Development history and challenges
- `Cargo.toml` - Dependencies

---

## Progress Summary

| Priority | Status | Description |
|----------|--------|-------------|
| **P0** | ✅ DONE | GPU-side transforms via 4×Vec4 columns |
| **P1.1** | ✅ DONE | Code organization (lib.rs, robot_view.rs, mesh.rs) |
| **P1.2** | ✅ DONE | Reusable Robot model with FK API |
| **P1.3** | ✅ DONE | Configurable widget with #[live] properties |
| **P1.4** | ✅ DONE | Embeddable RobotView widget with actions |
| **P2.1** | ✅ DONE | GPU transforms implemented |
| **P2.2** | ✅ DONE | Proper 3D camera with view/projection matrices |
| **P2.3** | ✅ DONE | Better shader with MVP pipeline |
| **P3.2** | ✅ DONE | Grid lines + sky gradient + proper draw order |

**Overall: ~95% complete** - P0/P1/P2/P3.2 done, only P3.1/P3.3 polish items remaining.

### Sky Gradient & Grid Rendering (2026-01-18)

**Sky Background Implementation:**

A screen-space gradient background inspired by Rerun's visualizer:

```rust
// robot_view.rs - live_design! draw_bg shader
draw_bg: {
    fn pixel(self) -> vec4 {
        // Purple-green-blue gradient like rerun
        let y = self.pos.y;  // 0 (top) to 1 (bottom)

        let top_color = vec3(0.55, 0.55, 0.70);     // Purple/blue tint
        let bottom_color = vec3(0.70, 0.75, 0.70);  // Greenish/gray

        let color = mix(top_color, bottom_color, y);
        return vec4(color, 1.0);
    }
}
```

**Grid Lines Rendering:**

Grid drawn AFTER robot to prevent depth-blocking issues:

```rust
// mesh.rs - DrawMesh pixel shader with draw_grid_lines flag
if self.draw_grid_lines > 0.5 {
    let u_frac = abs(u_scaled - floor(u_scaled + 0.5));
    let v_frac = abs(v_scaled - floor(v_scaled + 0.5));

    // Grid lines only - dark gray
    if u_frac < 0.003 || v_frac < 0.003 {
        return vec4(0.3, 0.3, 0.3, 1.0);
    }
    // Transparent for non-line areas
    return vec4(0.0, 0.0, 0.0, 0.0);
}
```

**Key Insight - Draw Order Matters:**
- Grid must be drawn AFTER robot links
- Robot wins depth test, grid only shows where no robot
- Prevents grid from blocking robot when viewed from below

```rust
// robot_view.rs draw_walk order:
// 1. Draw sky background
// 2. Draw robot links (writes to depth buffer)
// 3. Draw grid AFTER robot (fails depth test where robot is closer)
```

**Layout Fix - Modal Taking Space:**

Modal was consuming half the viewport height. Fixed with `flow: Overlay`:

```rust
// main.rs - URDFViewer layout
URDFViewer = {{URDFViewer}} {
    width: Fill
    height: Fill
    flow: Overlay  // Allow Modal to float over content

    main_content = <View> {
        width: Fill
        height: Fill
        flow: Down

        header = <View> { ... }
        viewport = <View> {
            flow: Overlay  // Stack robot_view and status_overlay
            robot_view = <RobotView> { ... }
            status_overlay = <View> { ... }
        }
    }

    robot_modal = <Modal> { ... }  // Floats over main_content
}
```

**Lesson Learned:**
- Makepad `abs_pos` does NOT remove elements from layout flow
- Use `flow: Overlay` on parent to stack children
- Modal components should be siblings to main content with Overlay parent

---

### Specular Lighting Added (2026-01-18)

**Blinn-Phong specular lighting implementation:**

```glsl
// In pixel shader
let light_dir = normalize(vec3(0.3, 0.8, 0.5));
let view_dir = normalize(self.camera_pos - self.world_pos);
let normal = normalize(self.world_normal);

// Blinn-Phong halfway vector method
let halfway = normalize(light_dir + view_dir);
let spec_angle = max(dot(normal, halfway), 0.0);
let specular = pow(spec_angle, self.shininess) * self.specular_strength;

// Add white specular highlight
let final_color = diffuse_color + vec3(specular);
```

**Configurable parameters:**
- `specular_strength: f32` - Intensity of highlights (default 0.5)
- `shininess: f32` - Size of highlights (default 32.0, higher = smaller)

---

### Proper 3D Camera Implemented (2026-01-18)

**Camera3D struct with proper MVP pipeline:**

```rust
// robot_view.rs - Camera3D struct
pub struct Camera3D {
    pub position: glam::Vec3,
    pub target: glam::Vec3,
    pub up: glam::Vec3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
}

impl Camera3D {
    pub fn from_orbital(distance: f32, yaw: f32, pitch: f32, target: glam::Vec3) -> Self;
    pub fn view_matrix(&self) -> glam::Mat4;  // look_at_rh
    pub fn projection_matrix(&self, aspect_ratio: f32) -> glam::Mat4;  // perspective_rh
}
```

**Shader now uses full MVP pipeline:**
```glsl
fn vertex(self) -> vec4 {
    // Model transform
    let world_pos = M * vec4(pos, 1.0);
    // View transform
    let view_pos = V * world_pos;
    // Projection
    let clip_pos = P * view_pos;
    return clip_pos;
}
```

**Benefits:**
- Proper perspective projection with vanishing points
- Correct depth buffer handling
- Aspect ratio maintained
- Camera can orbit around any target point

---

### GPU Transform Fix Completed (2025-01-18)

**The GPU-side transform is now implemented.** Using 4×Vec4 columns in the shader:

```rust
// mesh.rs - DrawMesh struct now has transform columns
#[live(vec4(1.0, 0.0, 0.0, 0.0))] pub transform_col0: Vec4,
#[live(vec4(0.0, 1.0, 0.0, 0.0))] pub transform_col1: Vec4,
#[live(vec4(0.0, 0.0, 1.0, 0.0))] pub transform_col2: Vec4,
#[live(vec4(0.0, 0.0, 0.0, 1.0))] pub transform_col3: Vec4,

// Shader reconstructs mat4 and transforms vertices on GPU
// robot_view.rs now uses:
drawer.set_transform(&transform);  // 64 bytes instead of 13MB
```

**Impact**: ~13MB/frame → 64 bytes/frame (99.9% reduction)

---

## Known Limitation: Perspective Grid View

### Desired Effect

A perspective grid like Rerun's visualizer:
- Grid lines converging to a vanishing point at the horizon
- Seamless gradient sky blending into infinite ground
- Lines fading with distance
- Robot standing on the grid with proper depth

### Technical Challenges

#### 1. CPU-side Transform Architecture Conflict

**Current pipeline:**
```
CPU transforms mesh vertices → GPU receives pre-transformed geometry → GPU just draws
```

**Required for perspective:**
```
CPU sends original mesh → GPU applies view matrix → GPU applies projection matrix → Perspective division
```

The current architecture applies transforms on CPU before sending to GPU. Perspective projection requires GPU-side projection matrix application AFTER view transformation, with proper perspective division (dividing by W component).

#### 2. Ground Plane Clipping Issues

With perspective projection:
- The infinite ground plane extends behind the camera
- Near-plane clipping cuts off visible portions
- Results in black triangles, split screens, or missing geometry
- Attempted fixes with near-plane clamping caused empty screens

```rust
// Attempted fix that failed:
let z_safe = max(near, z_eye);  // Clamp to near plane
// Result: Entire ground disappeared
```

#### 3. Makepad Shader Limitations

**Mat4 instance data fails on Metal:**
```
error: expected ';' at end of declaration list
    packed_float4 ds_transform 0;   // Missing underscore
error: duplicate member 'ds_transform'
```

**Variable naming issues:**
```
Struct not found 'line_width'    // Reserved name
Struct not found 'horizon'       // Reserved name
```

**Workaround:** Use 4×Vec4 columns, inline literal values instead of variables.

#### 4. Orthographic vs Perspective Fundamental Difference

**Orthographic (current):**
- Ground plane appears as thin diagonal strip when viewed from side
- No natural "horizon" where sky meets ground
- Finite ground plane can't fill entire "earth" area at all camera angles
- Rotating camera changes ground coverage unpredictably

**Perspective (desired):**
- Ground extends to horizon naturally
- Converging grid lines give depth perception
- Camera position determines visible area predictably

#### 5. Coordinate System Complexity

Multiple transformations interact:
```
URDF coords (Z-up) → Base rotation → Orbital rotation → Scale → Projection
```

Debugging is difficult because:
- Each rotation affects subsequent ones
- Sign conventions differ between libraries
- Screen Y direction (up vs down) varies

### Attempted Solutions

| Approach | Result |
|----------|--------|
| Simple perspective in shader | "Two screens" - black triangle artifact |
| Near-plane clamping | Empty screen - all geometry clipped |
| Full projection matrix refactor | Severe clipping, robot parts missing |
| Larger ground plane (500m) | Depth buffer artifacts (black fog) |
| 2D gradient background | Works but doesn't rotate with camera |

### Requirements for Proper Implementation

1. **GPU-side projection pipeline:**
   - Pass view matrix to shader
   - Pass projection matrix to shader
   - Apply in vertex shader: `gl_Position = proj * view * model * vertex`

2. **Proper camera setup:**
   - Camera position offset (not just rotation)
   - Look-at matrix construction
   - Perspective projection with proper FOV, aspect, near/far

3. **Infinite ground plane:**
   - Ray-plane intersection in fragment shader
   - Or: Large plane with depth buffer precision handling

4. **Alternative: Environment skybox**
   - Pre-rendered gradient texture
   - Maps to sphere/cube around scene

### Current Status ✅ RESOLVED

**Solution implemented:**
- Screen-space gradient sky (purple to green-blue, like Rerun)
- 3D grid plane (20m) with thin gray lines only
- Grid drawn AFTER robot to prevent depth-blocking
- Grid visible from both above and below camera positions
- `flow: Overlay` layout pattern for modal/overlay stacking

**Key insight:** Instead of fighting perspective projection issues, use:
1. Screen-space 2D gradient for sky (always fills background)
2. 3D grid plane for spatial reference (rotates with camera)
3. Draw order: sky → robot → grid (depth test handles visibility)

---

## Root Cause Analysis: Robot Orientation Issues

### The Problem

The robot appears 90° off (lying down instead of upright) despite various rotation attempts. Trial-and-error rotation fixes don't work because the underlying architecture is wrong.

### Current Broken Pipeline

```
1. Load mesh from STL (in URDF coordinate space)
2. CPU: Clone mesh, apply transform matrix, get new vertices
3. CPU: Upload transformed vertices to GPU
4. GPU Shader: Just scale by 4.0 and set depth
   ```rust
   let scaled = pos * 4.0;
   return vec4(scaled.x, scaled.y, depth, 1.0);
   ```
```

**Problems:**
- Transform is "baked into" vertex positions on CPU
- No separation between model, view, and projection transforms
- Shader has no knowledge of camera or coordinate systems
- Rotations get mixed together unpredictably

### Proper Pipeline (from Rik's draw_cube.rs)

```rust
fn vertex(self) -> vec4 {
    let pos = self.geom_pos;  // Original mesh coordinates
    let model_view = self.view_transform * self.transform;  // Instance transform
    self.world = model_view * vec4(pos, 1.);
    return self.camera_projection * (self.camera_view * self.world)  // Full MVP
}
```

**Key differences:**
1. **Original mesh stays unchanged** - Upload once, never re-upload
2. **Instance transform (`self.transform`)** - Per-link Mat4 for robot kinematics
3. **View transform (`self.view_transform`)** - Scene-level transform (URDF→screen coords)
4. **Camera view (`self.camera_view`)** - Camera position/orientation
5. **Camera projection (`self.camera_projection`)** - Perspective or orthographic projection

### Why Rotation Fixes Don't Work

When we apply `rotation_x(-PI/2)` on CPU before upload:
```rust
let camera_rot = scale * orbital * base_rot;
transformed.apply_transform(&camera_rot);  // Bakes rotation into vertices
```

The shader then receives pre-rotated vertices and has no way to:
- Know which way is "up" in the original model
- Apply camera rotation separately from model rotation
- Handle orbital (user drag) rotation independently

All rotations get multiplied together into vertex positions, making debugging impossible.

### Required Fix

1. **Stop CPU transforms** - Keep original mesh vertices
2. **Use `#[calc] transform: Mat4`** - Pass instance transform to shader
3. **Use Makepad camera uniforms** - `self.camera_projection`, `self.camera_view`
4. **Separate concerns:**
   - Model transform: URDF link kinematics
   - View transform: URDF→screen coordinate conversion
   - Camera: User-controlled orbital view

### Implementation Steps

1. Refactor `DrawMesh` to use `#[calc] transform: Mat4` (like draw_cube.rs)
2. Remove `update_transformed_geometry()` calls
3. Update shader to use full MVP pipeline
4. Set up proper camera with Makepad's camera system
5. Separate base rotation (URDF→screen) from orbital rotation (user control)

---

## Revised Development Plan (2025-01-18)

Based on analysis of `~/home/makepad-d3` rendering patterns, here is the updated priority list:

### Phase 1: Fix Critical Performance Issue (P0)

**Goal**: Eliminate 13MB/frame CPU overhead

1. **Add transform uniforms to DrawMesh**
   ```rust
   #[derive(Live, LiveRegister)]
   #[repr(C)]
   pub struct DrawMesh {
       // ... existing fields ...
       #[live] pub transform_col0: Vec4,
       #[live] pub transform_col1: Vec4,
       #[live] pub transform_col2: Vec4,
       #[live] pub transform_col3: Vec4,
   }
   ```

2. **Update shader to use GPU-side transform**
   ```glsl
   fn vertex(self) -> vec4 {
       let transform = mat4(
           self.transform_col0,
           self.transform_col1,
           self.transform_col2,
           self.transform_col3
       );
       let pos = transform * vec4(self.geom_pos, 1.0);
       // ... lighting and projection
   }
   ```

3. **Add set_transform method**
   ```rust
   impl DrawMesh {
       pub fn set_transform(&mut self, m: &Mat4) {
           self.transform_col0 = vec4(m.v[0], m.v[1], m.v[2], m.v[3]);
           self.transform_col1 = vec4(m.v[4], m.v[5], m.v[6], m.v[7]);
           self.transform_col2 = vec4(m.v[8], m.v[9], m.v[10], m.v[11]);
           self.transform_col3 = vec4(m.v[12], m.v[13], m.v[14], m.v[15]);
       }
   }
   ```

4. **Remove CPU transform code path**
   - Delete `update_transformed_geometry()` calls in render loop
   - Keep `init_link_geometry()` for one-time upload

### Phase 2: Proper 3D Projection (P1)

**Goal**: Replace `pos * 4.0` hack with real MVP pipeline

1. **Add camera uniforms**
   ```rust
   #[live] pub view_matrix: Mat4,
   #[live] pub proj_matrix: Mat4,
   ```

2. **Implement perspective projection in shader**
   ```glsl
   fn vertex(self) -> vec4 {
       let world_pos = self.model_transform * vec4(self.geom_pos, 1.0);
       let view_pos = self.view_matrix * world_pos;
       return self.proj_matrix * view_pos;
   }
   ```

3. **Create Camera3D struct** (from makepad-d3 pattern)
   ```rust
   pub struct Camera3D {
       pub position: Vec3,
       pub target: Vec3,
       pub up: Vec3,
       pub fov: f32,
       pub near: f32,
       pub far: f32,
   }

   impl Camera3D {
       pub fn view_matrix(&self) -> Mat4 { /* look_at */ }
       pub fn proj_matrix(&self, aspect: f32) -> Mat4 { /* perspective */ }
   }
   ```

### Phase 3: Visual Quality (P2)

**Goal**: Match Rerun-quality rendering

1. **Anti-aliased edges** (from makepad-d3 `smoothstep` pattern)
   ```glsl
   fn pixel(self) -> vec4 {
       let edge = fwidth(self.world_pos);
       let aa = smoothstep(0.0, edge * 2.0, /* edge distance */);
       // ...
   }
   ```

2. **Specular lighting**
   ```glsl
   let view_dir = normalize(self.camera_pos - self.world_pos);
   let reflect_dir = reflect(-light_dir, normal);
   let spec = pow(max(dot(view_dir, reflect_dir), 0.0), 32.0);
   ```

3. **Depth sorting for transparency** (from makepad-d3 painter's algorithm)
   ```rust
   links.sort_by(|a, b| {
       let dist_a = (a.center - camera.position).length();
       let dist_b = (b.center - camera.position).length();
       dist_b.partial_cmp(&dist_a).unwrap()
   });
   ```

### Phase 4: Optimizations (P3)

1. **Frustum culling** - Skip links outside view
2. **Backface culling** - GPU-side via winding order
3. **LOD system** - Reduced geometry for distant links
4. **Instanced rendering** - For multi-robot scenes

---

## Lessons from makepad-d3

| Pattern | makepad-d3 Implementation | Application to URDF |
|---------|---------------------------|---------------------|
| **Transforms** | CPU rotation matrices, 2D projection | Use same math for camera, but pass to GPU |
| **Anti-aliasing** | `smoothstep()` SDF edges | Apply to mesh silhouettes |
| **Depth sorting** | Painter's algorithm | For transparent robot parts |
| **Projection** | `perspective = 3.0 / (3.0 + depth * 0.3)` | Use proper projection matrix instead |
| **Visibility** | `is_visible()` dot product check | Frustum culling for off-screen links |

**Key Insight**: makepad-d3 does ALL transforms on CPU because it renders 2D SDFs. For 3D meshes, we should do transforms on GPU but can borrow the math patterns.

---

## Test Coverage

The codebase includes 51 unit tests covering core functionality:

| Module | Tests | Coverage |
|--------|-------|----------|
| `camera/camera3d.rs` | 8 | Camera initialization, orbiting, zoom, pitch clamping, frustum culling |
| `camera/controller.rs` | 7 | Drag events, scroll zoom, reset, pan mode |
| `error.rs` | 13 | All error types, display formatting, recoverable detection |
| `profiling.rs` | 2 | Stats tracking, scoped timer |
| `render/mesh.rs` | 3 | Cube/cylinder creation, mesh combining |
| `robot/model.rs` | 12 | Robot/Joint/Link structs, angle limits, FK accessors |
| `robot/loader.rs` | 4 | Invalid path, cycle detection, validation |
| `robot/kinematics.rs` | 2 | FK identity, FK with rotation |

### Running Tests

```bash
# Run all tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run specific module tests
cargo test camera::
cargo test robot::model
cargo test error::
```

### Feature Flags

```bash
# Enable profiling output
cargo run --features profiling

# Run tests with profiling
cargo test --features profiling
```

---

## Skybox and Grid Implementation Issues (2025-01-19)

### Critical Issue: Skybox Rendering Disabled

**Status**: Skybox is commented out due to "white cube bug"

```rust
// Draw skybox (disabled for now - causes white cube issue)
// if self.show_bg {
//     self.draw_bg.set_view_matrix(&view_mat_mkp);
//     ...
// }
```

**Root Cause**: The depth hack used in the skybox shader conflicts with embedded viewport rendering:

```rust
// Force Z to W so it renders at max depth (1.0)
let mut final_pos = vec4(clip_pos.xy, clip_pos.w, clip_pos.w);
```

This technique works for full-screen skyboxes but breaks with:
1. Embedded viewports (viewport correction happens after depth hack)
2. Depth buffer precision in non-fullscreen viewports
3. The viewport transformation doesn't account for the modified Z value

**Impact**: `DrawSkybox` struct exists but is completely unused dead code.

---

### Grid Implementation Issues

#### 1. Coordinate System Confusion

The grid shader uses misleading variable names:

```rust
let x_norm = self.world_pos.x / self.grid_spacing + 0.5;
let y_norm = self.world_pos.y / self.grid_spacing + 0.5;  // Should be z_norm
```

This only works because the grid is rotated 90° around X:

```rust
let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
```

After rotation, the original XZ plane becomes XY, but variable names don't reflect this.

#### 2. Axis Drawing is Backwards

```rust
// X axis (red)
if abs(self.world_pos.y) < self.line_width * 5.0 {
    return self.x_axis_color;
}

// Y axis (blue)
if abs(self.world_pos.x) < self.line_width * 5.0 {
    return self.z_axis_color;  // Note: named z_axis_color
}
```

After the base rotation:
- First check uses `world_pos.y` for X axis (incorrect)
- Second check uses `world_pos.x` for Z axis (incorrect)

#### 3. DrawGrid Not Actually Used

The `DrawGrid` type exists but rendering uses `DrawMesh` instead:

```rust
#[rust] grid_drawer: Option<DrawMesh>,  // Should be DrawGrid
```

This means the specialized grid shader in `DrawGrid` is never used.

#### 4. Performance: Pixel Clipping Inefficiency

```rust
fn pixel(self) -> vec4 {
    if self.screen_pos.x < self.draw_clip.x || self.screen_pos.x > self.draw_clip.z ||
       self.screen_pos.y < self.draw_clip.y || self.screen_pos.y > self.draw_clip.w {
        return vec4(0.0, 0.0, 0.0, 0.0);  // Still processes fragment!
    }
```

Should use `discard` to skip fragment processing entirely.

#### 5. Code Duplication: Viewport Correction

The same 15-line viewport correction block appears in all three shaders:

```rust
let win_w = max(self.full_size.x, 1.0);
let win_h = max(self.full_size.y, 1.0);
let center_x_px = vp_x + vp_w * 0.5;
let center_y_px = vp_y + vp_h * 0.5;
let offset_x = (center_x_px / win_w) * 2.0 - 1.0;
let offset_y = 1.0 - (center_y_px / win_h) * 2.0;
let scale_x = vp_w / win_w;
let scale_y = vp_h / win_h;
clip_pos.x = clip_pos.x * scale_x + clip_pos.w * offset_x;
clip_pos.y = clip_pos.y * scale_y + clip_pos.w * offset_y;
```

This appears in `DrawGrid`, `DrawMesh`, and `DrawSkybox` vertex shaders.

---

### Shader Matrix Reconstruction Overhead

Every shader reconstructs matrices from individual Vec4 columns:

```rust
// 12 lines to reconstruct ONE matrix
let m_col0 = self.transform_col0;
let m_col1 = self.transform_col1;
let m_col2 = self.transform_col2;
let m_col3 = self.transform_col3;
let model = mat4(
    vec4(m_col0.x, m_col0.y, m_col0.z, m_col0.w),
    vec4(m_col1.x, m_col1.y, m_col1.z, m_col1.w),
    vec4(m_col2.x, m_col2.y, m_col2.z, m_col2.w),
    vec4(m_col3.x, m_col3.y, m_col3.z, m_col3.w)
);
```

And you need to do this for model, view, and projection matrices (36 lines total).

**Suggestion**: Makepad should provide a helper function or built-in matrix reconstruction.

---

### Recommended Fixes

#### Priority 1: Fix Skybox White Cube Bug
- Investigate depth buffer behavior with embedded viewports
- Consider using fullscreen quad approach instead of cube
- Or: disable depth writes for skybox (`gl_DepthMask(false)`)

#### Priority 2: Fix Grid Coordinate Confusion
- Rename `y_norm` to `z_norm` for clarity
- Fix axis checks to use correct coordinates after rotation
- Add comments explaining the coordinate transformation

#### Priority 3: Use `discard` for Clipping
```rust
if (self.screen_pos.x < self.draw_clip.x || ...) {
    discard;  // Skip fragment processing entirely
}
```

#### Priority 4: Extract Viewport Correction
- Add to Makepad as built-in shader helper
- Or create a macro/function in local shader code

#### Priority 5: Consider Using DrawGrid
- Change `grid_drawer: Option<DrawMesh>` to `Option<DrawGrid>`
- Or remove `DrawGrid` if intentionally not used

---

## Async Robot Loading

The `RobotView` widget supports non-blocking robot loading using background threads:

### How it works

1. **Start loading**: `init_robot()` spawns a background thread via `std::thread::spawn`
2. **Communication**: Uses `std::sync::mpsc::channel` to send results back
3. **Polling**: A timer checks for completion every 16ms (~60fps)
4. **Completion**: When loading finishes, emits `RobotViewAction::LoadingComplete` or `LoadingError`

### API

```rust
// Check loading state
if robot_view.is_loading() {
    // Show spinner
}

// Listen for completion
match action.cast::<RobotViewAction>() {
    RobotViewAction::LoadingComplete => {
        // Robot is ready
    }
    RobotViewAction::LoadingError { message } => {
        // Handle error
    }
    _ => {}
}
```

### Benefits

- **Non-blocking UI**: First frame renders immediately with sky/grid
- **Responsive**: User can interact with camera while loading
- **Progress feedback**: `LoadingState::Loading { message }` provides status
