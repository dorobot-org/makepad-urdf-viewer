use makepad_urdf_player::robot::{load_any, scan_folder, ModelKind};
use std::fs;

fn setup() -> String {
    let root = "/tmp/scan_demo";
    let _ = fs::remove_dir_all(root);
    fs::create_dir_all(format!("{root}/armbot/meshes")).unwrap();
    fs::create_dir_all(format!("{root}/loose")).unwrap();
    // a robot folder: one urdf plus its parts
    fs::write(format!("{root}/armbot/arm.urdf"), br#"<?xml version="1.0"?>
<robot name="arm"><link name="l"><visual><geometry><box size="1 1 1"/></geometry></visual></link></robot>"#).unwrap();
    fs::write(format!("{root}/armbot/meshes/part_a.stl"), b"x").unwrap();
    fs::write(format!("{root}/armbot/meshes/part_b.stl"), b"x").unwrap();
    // a standalone mesh, not next to any urdf
    fs::write(format!("{root}/loose/widget.obj"),
        b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
    fs::write(format!("{root}/notes.txt"), b"ignored").unwrap();
    // a URDF at the top level must not hide meshes in unrelated siblings
    fs::write(format!("{root}/top.urdf"), br#"<?xml version="1.0"?>
<robot name="top"><link name="l"><visual><geometry><sphere radius="1"/></geometry></visual></link></robot>"#).unwrap();
    root.to_string()
}

/// Opening a folder should surface the robots, not bury them under their own
/// part meshes.
#[test]
fn scan_lists_models_and_hides_robot_parts() {
    let root = setup();
    let found = scan_folder(&root, 2);
    let names: Vec<&str> = found.iter().map(|m| m.relative.as_str()).collect();
    assert!(names.contains(&"armbot/arm.urdf"), "{names:?}");
    assert!(names.contains(&"loose/widget.obj"), "{names:?}");
    assert!(!names.iter().any(|n| n.contains("part_a")),
            "a robot's own meshes should not be listed: {names:?}");
    assert!(!names.iter().any(|n| n.ends_with(".txt")));
    // URDFs sort first
    assert_eq!(found[0].kind, ModelKind::Urdf);
    // a URDF at the scan root must not suppress meshes in sibling folders
    assert!(names.contains(&"loose/widget.obj"),
            "a top-level URDF wrongly hid an unrelated mesh: {names:?}");
}

/// A bare mesh opens as a one-link model, so mesh-only folders work.
#[test]
fn bare_mesh_opens_as_model() {
    let root = setup();
    let robot = load_any(format!("{root}/loose/widget.obj")).expect("obj opens");
    assert_eq!(robot.links.len(), 1);
    assert_eq!(robot.root_link, "widget");
    assert!(robot.links[0].mesh_data.as_ref().unwrap().triangle_count() > 0);
}

/// Depth is bounded, so pointing at a huge tree does not walk forever.
#[test]
fn scan_respects_depth() {
    let root = setup();
    let top = scan_folder(&root, 0);
    assert_eq!(top.len(), 1, "depth 0 sees only the top level: {top:?}");
    assert_eq!(top[0].relative, "top.urdf");
    assert!(scan_folder(&root, 1).len() > top.len(), "deeper scan finds more");
}
