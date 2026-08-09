//! Headless load check for the Redbank III models (no GUI needed):
//!     cargo test --test redbank_load

use makepad_urdf_player::robot::load_robot;

#[test]
fn unit_loads_with_all_meshes() {
    let robot = load_robot("data/redbank/redbank_unit.urdf", "data/redbank")
        .expect("unit urdf should load");
    assert_eq!(robot.links.len(), 3, "cap + drum + lens");
    for link in &robot.links {
        assert!(link.mesh_data.is_some(), "link {} has no mesh", link.name);
        // the loader marks missing meshes with a pure-red fallback cube
        assert_ne!(link.color, Some([1.0, 0.0, 0.0, 1.0]),
                   "link {} fell back to the missing-mesh cube", link.name);
    }
}

#[test]
fn array_loads_with_32_twist_joints() {
    let robot = load_robot("data/redbank/redbank_array.urdf", "data/redbank")
        .expect("array urdf should load");
    assert_eq!(robot.links.len(), 1 + 32 * 2, "plate + 32 x (body + lens)");
    for link in &robot.links {
        assert!(link.mesh_data.is_some(), "link {} has no mesh", link.name);
        assert_ne!(link.color, Some([1.0, 0.0, 0.0, 1.0]),
                   "link {} fell back to the missing-mesh cube", link.name);
    }
}
