# URDF Rerun-style Viewer Development Notes

This document details the development process, challenges encountered, and solutions implemented for the SO100 robot arm URDF viewer in Makepad.

## Goal

Replicate the animated URDF demo from Rerun's `animated_urdf` example using Makepad's rendering system. The SO100 robot arm should display with:
- All parts properly connected (no visual gaps at joints)
- Correct forward kinematics animation
- Orange color like the Rerun demo
- Proper 3D depth rendering

## Key Technical Challenges

### 1. Shader Z-Coordinate Handling

**Problem**: Initially, robot parts appeared "disconnected but moving in a coordinated way" - the FK was working but parts had visible gaps.

**Root Cause**: The vertex shader was setting Z=0 for all vertices:
```rust
return vec4(scaled.x, scaled.y, 0.0, 1.0);  // Z forced to 0!
```

This flattened all geometry to a 2D plane. Without depth information, overlapping parts would overwrite each other based on draw order rather than actual 3D occlusion.

**Solution**: Include proper Z values for depth testing:
```rust
let depth = 0.5 - scaled.z * 0.1;
return vec4(scaled.x, scaled.y, depth, 1.0);
```

**Note on depth range**: Using `-scaled.z * 0.25` caused "black fog" (parts disappearing) because depth values went out of the valid [0,1] range. The formula `0.5 - scaled.z * 0.1` keeps values centered around 0.5 with small deviation, avoiding clipping issues.

### 2. Matrix Layout Mismatch

**Problem**: Transforms appeared incorrect despite mathematically correct FK computation.

**Investigation**:
- Makepad's `Mat4` appeared to be row-major based on its `invert()` function: `a[0]=a00, a[1]=a01, a[4]=a10`
- Glam's `Mat4` is column-major: `to_cols_array()` returns `[m00, m10, m20, m30, m01...]`

**Discovery**: The `apply_transform()` function in mesh.rs actually expects column-major data:
```rust
// Matrix multiply: m * [px, py, pz, 1]
// Indices suggest column-major: v[0..4] = col0, v[4..8] = col1
let new_x = m.v[0] * px + m.v[4] * py + m.v[8] * pz + m.v[12];
let new_y = m.v[1] * px + m.v[5] * py + m.v[9] * pz + m.v[13];
let new_z = m.v[2] * px + m.v[6] * py + m.v[10] * pz + m.v[14];
```

**Solution**: Use glam's `to_cols_array()` directly without conversion:
```rust
fn glam_to_makepad(m: glam::Mat4) -> Mat4 {
    Mat4 { v: m.to_cols_array() }
}
```

### 3. Forward Kinematics Implementation

**Approach**: Match the Rerun example exactly using glam quaternions:

```rust
// From Rerun's animated_urdf example:
let rotation = glam::Quat::from_euler(
    glam::EulerRot::XYZ,
    joint.origin.rpy[0], joint.origin.rpy[1], joint.origin.rpy[2],
) * glam::Quat::from_axis_angle(axis, dynamic_angle);
```

Our implementation:
```rust
let origin_rotation = glam::Quat::from_euler(
    glam::EulerRot::XYZ,
    joint.origin_rpy.x, joint.origin_rpy.y, joint.origin_rpy.z,
);
let joint_rotation = glam::Quat::from_axis_angle(joint.axis, joint.angle);
let rotation = origin_rotation * joint_rotation;
let joint_transform = glam::Mat4::from_rotation_translation(rotation, joint.origin_xyz);
let child_transform = parent_transform * joint_transform;
```

### 4. Coordinate System Conversion

**Problem**: URDF uses Z-up convention, but screen coordinates typically use Y-up.

**Solution**: Apply a -90° rotation around X axis before orbital camera rotation:
```rust
let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
let orbital_rot = glam::Mat4::from_euler(glam::EulerRot::YXZ, cam_yaw, cam_pitch, 0.0);
let camera_rot = orbital_rot * base_rot;
```

### 5. Visual Origin Handling

**Consideration**: URDF visual elements can have their own origin offset relative to the link frame.

**Implementation**: Check for non-identity visual origins and apply them to mesh vertices:
```rust
if vis_xyz != glam::Vec3::ZERO || vis_rpy != glam::Vec3::ZERO {
    let vis_rot = glam::Quat::from_euler(glam::EulerRot::XYZ, vis_rpy.x, vis_rpy.y, vis_rpy.z);
    let vis_transform = glam::Mat4::from_rotation_translation(vis_rot, vis_xyz);
    mesh.apply_transform(&makepad_transform);
}
```

For the SO100 URDF, visual elements have identity origins, so this doesn't apply.

### 6. CPU-side Mesh Transformation

**Approach**: Rather than passing transform matrices to the shader (which had compilation issues), we transform mesh vertices on the CPU each frame:

```rust
pub fn update_transformed_geometry(&mut self, cx: &mut Cx, original_mesh: &MeshData, transform: &Mat4) {
    let mut transformed = original_mesh.clone();
    transformed.apply_transform(transform);
    self.geometry.upload_mesh_data(cx, transformed);
}
```

This keeps the original mesh data intact and allows re-transformation each frame.

## Failed Approaches

1. **Single combined mesh**: Tried combining all link meshes into one draw call - resulted in nothing rendering due to geometry buffer issues.

2. **Shader-side Mat4 instance data**: Makepad's shader compiler had issues with Mat4 as instance data.

3. **Depth value `-scaled.z * 0.25`**: Caused "black fog" - values went out of valid depth range.

4. **Row-major matrix conversion**: Unnecessary - the apply_transform function already expects column-major.

## Current Status

The viewer works with:
- All parts connected and moving together
- Proper FK animation matching Rerun behavior
- Orange color
- Orbital camera with mouse drag
- Keyboard controls for joint angles

**Known Issue**: Some mesh interpenetration (穿模) when parts move through each other. This is a depth precision issue that would require more sophisticated handling (e.g., depth bias, better depth range mapping).

## Architecture

```
main.rs
├── Robot struct
│   ├── links: Vec<RobotLink>      # Mesh data per link
│   ├── joints: Vec<RobotJoint>    # Joint parameters
│   ├── link_transforms: Vec<Mat4> # FK results
│   └── update_forward_kinematics()
├── URDFViewer widget
│   ├── link_drawers: Vec<DrawMesh>   # One drawer per link
│   ├── original_meshes: Vec<MeshData> # Untransformed mesh data
│   └── draw_walk() # Transform & render each frame
└── Animation loop (timer-based, matching Rerun formula)

mesh.rs
├── MeshData
│   ├── from_stl()
│   ├── apply_transform()
│   └── make_double_sided()
├── GeometryMesh3D (GPU geometry buffer)
└── DrawMesh (shader + drawing)
```

## References

- Rerun animated_urdf example: `examples/rust/animated_urdf/src/main.rs`
- SO100 URDF: `data/so100.urdf`
- Makepad DrawCube for geometry patterns
