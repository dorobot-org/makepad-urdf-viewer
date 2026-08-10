//! Headless load check for the Redbank III models (no GUI needed):
//!     cargo test --test redbank_load

use makepad_urdf_player::robot::{load_robot, ForwardKinematics, Robot};

/// The loader substitutes `MeshData::test_cube(0.05)` when an STL is missing
/// or corrupt. It only paints that cube red when the URDF gave no material,
/// and the Redbank URDFs all specify colours — so colour proves nothing here.
/// The cube is 12 triangles (24 double-sided) inside a 0.05 m box; every real
/// part has thousands of triangles, so size is what actually separates them.
fn assert_no_fallback_meshes(robot: &Robot) {
    for link in &robot.links {
        let mesh = link
            .mesh_data
            .as_ref()
            .unwrap_or_else(|| panic!("link {} has no mesh at all", link.name));
        let tris = mesh.indices.len() / 3;
        assert!(
            tris > 100,
            "link {} has only {} triangles — this is the missing-mesh fallback cube",
            link.name,
            tris
        );
        let size = glam::Vec3::from(mesh.bounds_max) - glam::Vec3::from(mesh.bounds_min);
        assert!(
            size.max_element() > 0.06,
            "link {} spans only {:?} — fallback cube (0.05 m)",
            link.name,
            size
        );
    }
}

#[test]
fn unit_loads_with_all_meshes() {
    let robot = load_robot("data/redbank/redbank_unit.urdf", "data/redbank")
        .expect("unit urdf should load");
    assert_eq!(robot.links.len(), 3, "cap + drum + lens");
    assert_no_fallback_meshes(&robot);
}

#[test]
fn array_loads_with_32_twist_joints() {
    let robot = load_robot("data/redbank/redbank_array.urdf", "data/redbank")
        .expect("array urdf should load");
    assert_eq!(robot.links.len(), 1 + 32 * 2, "plate + 32 x (body + lens)");
    assert_no_fallback_meshes(&robot);
    let movable = robot.joints.iter().filter(|j| j.is_movable()).count();
    assert_eq!(movable, 32, "one bayonet twist per unit");
}

/// The fallback path must actually trip the guard above — otherwise the two
/// tests are vacuous, which is exactly how they passed before.
#[test]
fn fallback_mesh_is_detected() {
    let urdf = r#"<?xml version="1.0"?>
<robot name="broken">
  <link name="base">
    <visual>
      <geometry><mesh filename="does_not_exist.stl" scale="0.001 0.001 0.001"/></geometry>
      <material name="m"><color rgba="0.2 0.2 0.2 1.0"/></material>
    </visual>
  </link>
</robot>"#;
    let robot = makepad_urdf_player::robot::load_robot_from_string(urdf, "data/redbank")
        .expect("urdf itself is valid; only the mesh is missing");
    let result = std::panic::catch_unwind(|| assert_no_fallback_meshes(&robot));
    assert!(
        result.is_err(),
        "a missing STL must fail the mesh check (the URDF colour hides it)"
    );
}

/// Framing/orbit uses world-space bounds; link-local boxes put the pivot off
/// the model for anything with translated joints.
#[test]
fn world_bounds_account_for_joint_transforms() {
    let mut robot = load_robot("data/redbank/redbank_array.urdf", "data/redbank")
        .expect("array urdf should load");
    ForwardKinematics::update(&mut robot);

    let (lmin, lmax) = robot.bounds();
    let (wmin, wmax) = robot.bounds_world();

    // the 32 units are spread over the ~0.96 x 0.48 m plate, so the world box
    // has to be taller than any single link's local box
    assert!(
        (wmax.z - wmin.z) > (lmax.z - lmin.z) + 0.01,
        "world bounds {:?}..{:?} should exceed local {:?}..{:?}",
        wmin,
        wmax,
        lmin,
        lmax
    );
}

/// The widget must not hard-code any model: a library consumer gets an empty
/// scene until it asks for something. Guarding this in a test because the
/// viewer previously auto-loaded a Redbank URDF on first draw, which made the
/// crate unusable for anyone else.
#[test]
fn library_ships_no_default_model() {
    let src = include_str!("../src/robot_view.rs");
    let body = src
        .split("fn ensure_initialized")
        .nth(1)
        .expect("ensure_initialized should exist");
    let body = &body[..body.find("\n    }").unwrap_or(body.len())];
    let lower = body.to_lowercase();
    // a hard-coded model would appear as a quoted path, e.g. "foo/bar.urdf"
    assert!(
        !lower.contains(".urdf\""),
        "ensure_initialized must not name a URDF file: {body}"
    );
    for needle in ["redbank", "so100"] {
        assert!(
            !lower.contains(needle),
            "ensure_initialized must not reference a specific model ({needle})"
        );
    }
}
