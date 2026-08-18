//! COLLADA parsing, checked against a real robot mesh rather than a
//! hand-written fixture: `ur5_base.dae` is the UR5 base link from
//! ros-industrial/universal_robot, exported by Blender 3.2.2. Hand-made
//! COLLADA proves the happy path and nothing about what exporters emit,
//! which is where the format's difficulty actually lives.
use makepad_urdf_player::render::mesh::FLOATS_PER_VERTEX;
use makepad_urdf_player::robot::collada::from_dae_str;

const UR5_BASE: &str = include_str!("fixtures/ur5_base.dae");

#[test]
fn parses_a_real_robot_mesh() {
    let m = from_dae_str(UR5_BASE).expect("UR5 base parses");
    assert!(!m.indices.is_empty(), "no triangles");
    assert_eq!(m.indices.len() % 3, 0, "indices are not whole triangles");
    assert_eq!(m.vertices.len() % FLOATS_PER_VERTEX, 0, "ragged vertex buffer");
    assert_eq!(
        m.vertices.len() / FLOATS_PER_VERTEX,
        m.indices.len(),
        "de-indexed: one vertex per index"
    );
}

#[test]
fn the_base_link_is_the_size_a_ur5_base_link_is() {
    // Metres, and a UR5 base is a ~0.15 m puck. This is the assertion that
    // catches a missed `<unit>` scale or a Z_UP/Y_UP mix-up, both of which
    // parse cleanly and produce a robot that is wrong by 1000x or lying down.
    let m = from_dae_str(UR5_BASE).unwrap();
    let d = [
        m.bounds_max[0] - m.bounds_min[0],
        m.bounds_max[1] - m.bounds_min[1],
        m.bounds_max[2] - m.bounds_min[2],
    ];
    for (i, v) in d.iter().enumerate() {
        assert!(*v > 0.01 && *v < 1.0, "axis {i} spans {v} m, not a UR5 base");
    }
    // A base is wider than it is tall.
    assert!(d[2] < d[0].max(d[1]), "taller than wide: axis mix-up");
}

#[test]
fn normals_are_unit_length() {
    let m = from_dae_str(UR5_BASE).unwrap();
    let mut checked = 0;
    for v in m.vertices.chunks(FLOATS_PER_VERTEX).take(500) {
        let n = (v[4] * v[4] + v[5] * v[5] + v[6] * v[6]).sqrt();
        assert!((n - 1.0).abs() < 0.05, "normal length {n}");
        checked += 1;
    }
    assert!(checked > 0);
}

#[test]
fn it_brings_its_material_colour() {
    // The whole reason robot packages ship .dae instead of .stl.
    let m = from_dae_str(UR5_BASE).unwrap();
    let c = m.color.expect("no diffuse colour recovered");
    for k in 0..4 {
        assert!((0.0..=1.0).contains(&c[k]), "colour {k} = {} out of range", c[k]);
    }
}

#[test]
fn junk_is_refused_rather_than_half_parsed() {
    assert!(from_dae_str("not xml at all").is_err());
    assert!(from_dae_str("<COLLADA></COLLADA>").is_err());
}
