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

// ---- regressions from the adversarial review ------------------------------

fn wrap(mesh_inner: &str, extra: &str) -> String {
    format!(
        r##"<?xml version="1.0"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  {extra}
  <library_geometries><geometry id="g"><mesh>{mesh_inner}</mesh></geometry></library_geometries>
</COLLADA>"##
    )
}

const TRI_INPUTS: &str = r##"<input semantic="VERTEX" source="#v" offset="0"/>"##;

fn positions(vals: &str, count: usize, extra_accessor: &str) -> String {
    format!(
        r##"<source id="p"><float_array id="pa" count="{count}">{vals}</float_array>
           <technique_common><accessor source="#pa" count="{c}" stride="3"{extra_accessor}>
           <param name="X" type="float"/><param name="Y" type="float"/><param name="Z" type="float"/>
           </accessor></technique_common></source>
           <vertices id="v"><input semantic="POSITION" source="#p"/></vertices>"##,
        c = count / 3
    )
}

#[test]
fn five_float_array_with_index_one_does_not_panic() {
    // vi*ps+2 == pv.len() was accepted by the old guard and panicked.
    let dae = wrap(
        &format!(
            "{}<triangles count=\"1\">{}<p>0 1 0</p></triangles>",
            positions("0 0 0 1 1", 5, ""),
            TRI_INPUTS
        ),
        "",
    );
    // Must not panic. The damaged triangle is dropped whole, so no geometry
    // survives and the parse reports that rather than emitting fragments.
    let r = from_dae_str(&dae);
    assert!(r.is_err() || r.unwrap().indices.len() % 3 == 0);
}

#[test]
fn empty_float_array_does_not_underflow() {
    let dae = wrap(
        &format!(
            "{}<triangles count=\"1\">{}<p>0 0 0</p></triangles>",
            positions("", 0, ""),
            TRI_INPUTS
        ),
        "",
    );
    let _ = from_dae_str(&dae); // must not panic
}

#[test]
fn a_bad_corner_drops_its_whole_triangle() {
    // Old behaviour skipped one corner: 5 vertices emitted, triangles
    // regrouped across the damage. Now: the bad triangle vanishes, the good
    // one survives intact.
    let dae = wrap(
        &format!(
            "{}<triangles count=\"2\">{}<p>0 1 99 0 1 2</p></triangles>",
            positions("0 0 0  1 0 0  0 1 0", 9, ""),
            TRI_INPUTS
        ),
        "",
    );
    let m = from_dae_str(&dae).expect("good triangle survives");
    assert_eq!(m.indices.len(), 3, "exactly the valid triangle");
}

#[test]
fn huge_indices_fail_bounds_not_wrap() {
    let big = usize::MAX / 3;
    let dae = wrap(
        &format!(
            "{}<triangles count=\"1\">{}<p>{big} 0 1</p></triangles>",
            positions("0 0 0  1 0 0  0 1 0", 9, ""),
            TRI_INPUTS
        ),
        "",
    );
    let r = from_dae_str(&dae);
    assert!(r.is_err() || r.unwrap().indices.is_empty());
}

#[test]
fn accessor_offset_skips_padding() {
    // First float is padding; offset=1 must skip it. Reading it as X shifts
    // every tuple.
    let dae = wrap(
        &format!(
            "{}<triangles count=\"1\">{}<p>0 1 2</p></triangles>",
            positions("99 0 0 0  1 0 0  0 1 0", 10, r##" offset="1""##),
            TRI_INPUTS
        ),
        "",
    );
    let m = from_dae_str(&dae).expect("offset accessor parses");
    // Vertex 0 must be (0,0,0) — not (99,0,0).
    assert!(m.vertices[0].abs() < 1e-6, "padding leaked in as X: {}", m.vertices[0]);
}

#[test]
fn non_numeric_index_is_an_error_not_a_shift() {
    let dae = wrap(
        &format!(
            "{}<triangles count=\"1\">{}<p>0 x 2</p></triangles>",
            positions("0 0 0  1 0 0  0 1 0", 9, ""),
            TRI_INPUTS
        ),
        "",
    );
    assert!(from_dae_str(&dae).is_err());
}

#[test]
fn material_binding_symbol_resolves_through_instance_material() {
    // symbol "S" != material id "M": legal, and the whole point of
    // <bind_material>.
    let extra = r##"
  <library_effects><effect id="e"><profile_COMMON><technique sid="t"><lambert>
    <diffuse><color>0.2 0.4 0.6 1</color></diffuse>
  </lambert></technique></profile_COMMON></effect></library_effects>
  <library_materials><material id="M"><instance_effect url="#e"/></material></library_materials>
  <library_visual_scenes><visual_scene id="vs"><node id="n">
    <instance_geometry url="#g"><bind_material><technique_common>
      <instance_material symbol="S" target="#M"/>
    </technique_common></bind_material></instance_geometry>
  </node></visual_scene></library_visual_scenes>"##;
    let dae = wrap(
        &format!(
            "{}<triangles material=\"S\" count=\"1\">{}<p>0 1 2</p></triangles>",
            positions("0 0 0  1 0 0  0 1 0", 9, ""),
            TRI_INPUTS
        ),
        extra,
    );
    let m = from_dae_str(&dae).expect("parses");
    let c = m.color.expect("symbol resolved to the material colour");
    assert!((c[0] - 0.2).abs() < 1e-5 && (c[2] - 0.6).abs() < 1e-5);
}
