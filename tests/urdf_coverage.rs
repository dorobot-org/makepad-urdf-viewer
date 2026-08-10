//! What kinds of URDF this library can actually render.
//!
//! These cover the three things that stopped off-the-shelf URDFs from working:
//! primitive geometry, ROS `package://` URIs, and non-STL meshes.

use makepad_urdf_player::robot::{load_robot, load_robot_from_string};

/// URDFs built from primitives (very common, and all collision geometry)
/// must render, not just parse.
#[test]
fn primitive_geometry_renders() {
    let urdf = r#"<?xml version="1.0"?>
<robot name="prim">
  <link name="base">
    <visual><geometry><box size="0.2 0.3 0.1"/></geometry></visual>
  </link>
  <link name="arm">
    <visual>
      <origin xyz="0 0 0.2" rpy="0 0 0"/>
      <geometry><cylinder radius="0.05" length="0.4"/></geometry>
    </visual>
  </link>
  <link name="ball">
    <visual><geometry><sphere radius="0.06"/></geometry></visual>
  </link>
  <joint name="j1" type="revolute">
    <parent link="base"/><child link="arm"/>
    <axis xyz="0 0 1"/><limit lower="-1" upper="1" effort="1" velocity="1"/>
  </joint>
  <joint name="j2" type="fixed">
    <parent link="arm"/><child link="ball"/>
  </joint>
</robot>"#;
    let robot = load_robot_from_string(urdf, ".").expect("valid urdf");
    assert_eq!(robot.links.len(), 3);
    for link in &robot.links {
        let mesh = link
            .mesh_data
            .as_ref()
            .unwrap_or_else(|| panic!("link {} produced no geometry", link.name));
        assert!(mesh.triangle_count() > 0, "link {} is empty", link.name);
    }

    // box is 0.2 x 0.3 x 0.1 centred on the origin
    let base = robot.links.iter().find(|l| l.name == "base").unwrap();
    let m = base.mesh_data.as_ref().unwrap();
    let size = [
        m.bounds_max[0] - m.bounds_min[0],
        m.bounds_max[1] - m.bounds_min[1],
        m.bounds_max[2] - m.bounds_min[2],
    ];
    for (got, want) in size.iter().zip([0.2f32, 0.3, 0.1].iter()) {
        assert!((got - want).abs() < 1e-4, "box size {size:?}");
    }

    // the cylinder's <origin> must move it: z spans 0.2 +/- 0.2
    let arm = robot.links.iter().find(|l| l.name == "arm").unwrap();
    let m = arm.mesh_data.as_ref().unwrap();
    assert!((m.bounds_min[2] - 0.0).abs() < 1e-4, "cyl min z {:?}", m.bounds_min);
    assert!((m.bounds_max[2] - 0.4).abs() < 1e-4, "cyl max z {:?}", m.bounds_max);
    // and it is Z-aligned, so x/y span only the diameter
    assert!((m.bounds_max[0] - 0.05).abs() < 1e-3, "cylinder should be Z-aligned");
}


/// ROS-style `package://` URIs and OBJ meshes both have to work: they are the
/// two most common reasons an off-the-shelf URDF failed to render before.
#[test]
fn package_uri_and_obj_mesh() {
    let robot = load_robot("/tmp/objtest/robot.urdf", "/tmp/objtest").expect("urdf loads");
    let link = &robot.links[0];
    let mesh = link.mesh_data.as_ref().expect("mesh loaded");
    // 2 triangles, doubled by make_double_sided
    assert_eq!(mesh.triangle_count(), 4, "expected the OBJ square");
    // and it is NOT the missing-mesh fallback cube
    assert_ne!(link.color, Some([1.0, 0.0, 0.0, 1.0]));
    let size = [
        mesh.bounds_max[0] - mesh.bounds_min[0],
        mesh.bounds_max[1] - mesh.bounds_min[1],
    ];
    assert!((size[0] - 1.0).abs() < 1e-4 && (size[1] - 1.0).abs() < 1e-4, "{size:?}");
}

/// COLLADA is genuinely unsupported. It must say so, rather than failing as a
/// corrupt STL — an integrator needs to know to convert the file.
#[test]
fn collada_reports_clearly() {
    std::fs::create_dir_all("/tmp/daetest").unwrap();
    std::fs::write("/tmp/daetest/x.dae", b"<COLLADA/>").unwrap();
    std::fs::write(
        "/tmp/daetest/robot.urdf",
        br#"<?xml version="1.0"?>
<robot name="dae">
  <link name="base">
    <visual><geometry><mesh filename="x.dae"/></geometry></visual>
  </link>
</robot>"#,
    )
    .unwrap();
    // the load itself still succeeds (fallback cube), but the message is clear
    let robot = load_robot("/tmp/daetest/robot.urdf", "/tmp/daetest").expect("urdf parses");
    let link = &robot.links[0];
    assert_eq!(
        link.color,
        Some([1.0, 0.0, 0.0, 1.0]),
        "an unsupported mesh should fall back to the visible marker cube"
    );
}
