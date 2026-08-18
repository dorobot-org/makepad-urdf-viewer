//! COLLADA (`.dae`) meshes.
//!
//! Half the robot descriptions in the wild ship `.dae` rather than `.stl`,
//! and the reason is materials: STL carries geometry and nothing else, so a
//! URDF using it has to name a colour for every link by hand. COLLADA brings
//! the mesh's own. Refusing the format meant refusing those robots, and
//! converting them to STL to get them open threw away the very thing they were
//! chosen for.
//!
//! This reads the subset robot exporters actually emit, checked against the
//! UR5 visual meshes from ros-industrial/universal_robot (Blender 3.2.2,
//! Z_UP, metres, `<triangles>` already triangulated, one block per material).
//! What it deliberately does not do: skinning, animation, transform stacks
//! inside the visual scene, `<polylist>` with n-gons. A robot link is a rigid
//! lump of triangles, and anything richer belongs to a scene format rather
//! than to a mesh slot in a URDF.
//!
//! Submeshes are merged. The renderer draws one geometry per link and colours
//! it from the URDF, so several material groups have nowhere separate to go;
//! the first material's diffuse is carried out on [`MeshData::color`] so a
//! link with no colour of its own can fall back to the mesh's.

use std::collections::HashMap;

use crate::render::mesh::MeshData;

/// Parse a COLLADA document into one merged mesh.
pub fn from_dae_str(text: &str) -> Result<MeshData, String> {
    let doc = roxmltree::Document::parse(text)
        .map_err(|e| format!("COLLADA is not well-formed XML: {e}"))?;
    let root = doc.root_element();

    // `<unit meter="0.001"/>` is how an exporter says millimetres. URDF is
    // metres, so anything else has to be scaled or the robot arrives 1000x.
    let unit = root
        .descendants()
        .find(|n| n.has_tag_name("unit"))
        .and_then(|n| n.attribute("meter"))
        .and_then(|v| v.parse::<f32>().ok())
        .unwrap_or(1.0);
    // Z_UP matches URDF and the STL path, so it passes through untouched.
    // Y_UP is rotated into it rather than left for the caller to notice as a
    // robot lying on its side.
    let y_up = root
        .descendants()
        .find(|n| n.has_tag_name("up_axis"))
        .and_then(|n| n.text())
        .map(|t| t.trim() == "Y_UP")
        .unwrap_or(false);

    let colors = effect_colors(&root);
    let mut out = MeshData::default();
    let mut first_color = None;
    let mut any = false;

    for mesh in root.descendants().filter(|n| n.has_tag_name("mesh")) {
        // id -> the floats it holds. `<accessor stride>` is what says whether
        // a source is xyz or uv; a fixed 3 would misread texture coordinates.
        let mut sources: HashMap<String, (Vec<f32>, usize)> = HashMap::new();
        for src in mesh.children().filter(|n| n.has_tag_name("source")) {
            let Some(id) = src.attribute("id") else { continue };
            let Some(arr) = src.descendants().find(|n| n.has_tag_name("float_array")) else {
                continue;
            };
            let vals: Vec<f32> = arr
                .text()
                .unwrap_or_default()
                .split_ascii_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            let stride = src
                .descendants()
                .find(|n| n.has_tag_name("accessor"))
                .and_then(|a| a.attribute("stride"))
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(3);
            sources.insert(id.to_string(), (vals, stride.max(1)));
        }

        // `<vertices>` is an indirection: triangles reference it as VERTEX and
        // it names the real POSITION source. Skipping the hop reads normals as
        // positions.
        let mut vertex_alias: HashMap<String, String> = HashMap::new();
        for v in mesh.children().filter(|n| n.has_tag_name("vertices")) {
            let Some(id) = v.attribute("id") else { continue };
            if let Some(inp) = v
                .children()
                .find(|n| n.has_tag_name("input") && n.attribute("semantic") == Some("POSITION"))
            {
                if let Some(src) = inp.attribute("source") {
                    vertex_alias.insert(id.to_string(), src.trim_start_matches('#').to_string());
                }
            }
        }

        for tris in mesh.children().filter(|n| n.has_tag_name("triangles")) {
            let mut pos_src = None;
            let mut nrm_src = None;
            let mut pos_off = 0usize;
            let mut nrm_off = usize::MAX;
            let mut stride = 1usize;
            for inp in tris.children().filter(|n| n.has_tag_name("input")) {
                let sem = inp.attribute("semantic").unwrap_or("");
                let raw = inp.attribute("source").unwrap_or("").trim_start_matches('#');
                let off = inp
                    .attribute("offset")
                    .and_then(|o| o.parse::<usize>().ok())
                    .unwrap_or(0);
                stride = stride.max(off + 1);
                match sem {
                    "VERTEX" => {
                        pos_src = Some(vertex_alias.get(raw).cloned().unwrap_or_else(|| raw.into()));
                        pos_off = off;
                    }
                    "NORMAL" => {
                        nrm_src = Some(raw.to_string());
                        nrm_off = off;
                    }
                    _ => {}
                }
            }
            let Some(pos_src) = pos_src.and_then(|k| sources.get(&k)) else { continue };
            let nrm = nrm_src.and_then(|k| sources.get(&k));

            let p: Vec<usize> = tris
                .children()
                .find(|n| n.has_tag_name("p"))
                .and_then(|n| n.text())
                .unwrap_or_default()
                .split_ascii_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();

            if first_color.is_none() {
                first_color = tris.attribute("material").and_then(|m| colors.get(m).copied());
            }

            // De-indexed: COLLADA indexes position and normal separately, and
            // a GPU vertex buffer cannot. Welding them back would need a hash
            // per (p,n) pair and buys nothing here — the STL path is already
            // fully de-indexed for the same reason.
            let base = (out.vertices.len() / crate::render::mesh::FLOATS_PER_VERTEX) as u32;
            let mut n = 0u32;
            for tri in p.chunks(stride * 3) {
                if tri.len() < stride * 3 {
                    break;
                }
                for k in 0..3 {
                    let vi = tri[k * stride + pos_off];
                    let (pv, ps) = pos_src;
                    if vi * ps + 2 >= pv.len() + 1 && vi * ps + 2 > pv.len() - 1 {
                        continue;
                    }
                    let mut pos = [pv[vi * ps], pv[vi * ps + 1], pv[vi * ps + 2]];
                    let mut nor = [0.0f32, 0.0, 1.0];
                    if let (Some((nv, ns)), true) = (nrm, nrm_off != usize::MAX) {
                        let ni = tri[k * stride + nrm_off];
                        if ni * ns + 2 < nv.len() {
                            nor = [nv[ni * ns], nv[ni * ns + 1], nv[ni * ns + 2]];
                        }
                    }
                    if unit != 1.0 {
                        pos = [pos[0] * unit, pos[1] * unit, pos[2] * unit];
                    }
                    if y_up {
                        pos = [pos[0], -pos[2], pos[1]];
                        nor = [nor[0], -nor[2], nor[1]];
                    }
                    out.push_vertex_pub(pos, nor, 0.0);
                    n += 1;
                }
            }
            out.indices.extend((0..n).map(|i| base + i));
            any = true;
        }
    }

    if !any || out.indices.is_empty() {
        return Err("COLLADA file has no triangles this loader understands".into());
    }
    out.color = first_color;
    out.recompute_bounds();
    Ok(out)
}

/// material symbol -> diffuse rgba, resolved through `<instance_effect>`.
fn effect_colors(root: &roxmltree::Node) -> HashMap<String, [f32; 4]> {
    let mut fx: HashMap<String, [f32; 4]> = HashMap::new();
    for e in root.descendants().filter(|n| n.has_tag_name("effect")) {
        let Some(id) = e.attribute("id") else { continue };
        if let Some(c) = e
            .descendants()
            .find(|n| n.has_tag_name("diffuse"))
            .and_then(|d| d.children().find(|n| n.has_tag_name("color")))
            .and_then(|c| c.text())
        {
            let v: Vec<f32> = c.split_ascii_whitespace().filter_map(|x| x.parse().ok()).collect();
            if v.len() >= 3 {
                fx.insert(id.into(), [v[0], v[1], v[2], *v.get(3).unwrap_or(&1.0)]);
            }
        }
    }
    let mut out = HashMap::new();
    for m in root.descendants().filter(|n| n.has_tag_name("material")) {
        let Some(id) = m.attribute("id") else { continue };
        if let Some(url) = m
            .children()
            .find(|n| n.has_tag_name("instance_effect"))
            .and_then(|i| i.attribute("url"))
        {
            if let Some(c) = fx.get(url.trim_start_matches('#')) {
                // `<triangles material=>` names a binding symbol, which
                // exporters overwhelmingly set to the material id with a
                // suffix. Register both so either resolves.
                out.insert(id.to_string(), *c);
                out.insert(format!("{id}-material"), *c);
            }
        }
    }
    out
}
