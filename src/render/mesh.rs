//! Mesh data structures for 3D rendering
//!
//! Provides mesh loading from STL files and primitive generation.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Vertex data layout: pos(3) + id(1) + normal(3) + uv(2) = 9 floats per vertex
pub const FLOATS_PER_VERTEX: usize = 9;

/// Mesh data loaded from STL or generated procedurally
#[derive(Clone, Debug, Default)]
pub struct MeshData {
    /// Interleaved vertex data: pos(3), id(1), normal(3), uv(2)
    pub vertices: Vec<f32>,
    /// Triangle indices
    pub indices: Vec<u32>,
    /// Minimum bounds
    pub bounds_min: [f32; 3],
    /// Maximum bounds
    pub bounds_max: [f32; 3],
    /// Diffuse colour the mesh brought with it, when the format carries one.
    /// COLLADA does; STL cannot. A URDF link with no material of its own can
    /// fall back to this instead of defaulting to grey.
    pub color: Option<[f32; 4]>,
}

impl MeshData {
    /// Parse a Wavefront OBJ. Handles `v`, `vn`, `f` with `v`, `v/vt`,
    /// `v//vn` and `v/vt/vn` forms, negative (relative) indices, and polygons
    /// with more than three sides (fan-triangulated). Materials, texture
    /// coordinates and smoothing groups are ignored — this is a geometry
    /// loader, and the viewer shades from the URDF material.
    ///
    /// Faces with no normal get a generated flat one.
    pub fn from_obj_str(text: &str) -> Result<Self, String> {
        let mut positions: Vec<[f32; 3]> = Vec::new();
        let mut normals: Vec<[f32; 3]> = Vec::new();
        let mut mesh = MeshData::default();

        // OBJ indices are 1-based; negative counts back from the end
        fn resolve(raw: i32, len: usize) -> Option<usize> {
            if raw > 0 {
                let i = (raw - 1) as usize;
                (i < len).then_some(i)
            } else if raw < 0 {
                len.checked_sub((-raw) as usize)
            } else {
                None
            }
        }

        for line in text.lines() {
            let line = line.trim();
            let mut parts = line.split_whitespace();
            match parts.next() {
                Some("v") => {
                    let v: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                    if v.len() < 3 {
                        return Err("OBJ: malformed vertex".to_string());
                    }
                    positions.push([v[0], v[1], v[2]]);
                }
                Some("vn") => {
                    let v: Vec<f32> = parts.filter_map(|p| p.parse().ok()).collect();
                    if v.len() >= 3 {
                        normals.push([v[0], v[1], v[2]]);
                    }
                }
                Some("f") => {
                    // each corner: v | v/vt | v//vn | v/vt/vn
                    let corners: Vec<(usize, Option<usize>)> = parts
                        .filter_map(|tok| {
                            let mut it = tok.split('/');
                            let vi: i32 = it.next()?.parse().ok()?;
                            let _vt = it.next();
                            let ni = it.next().and_then(|n| n.parse::<i32>().ok());
                            let v = resolve(vi, positions.len())?;
                            let n = ni.and_then(|n| resolve(n, normals.len()));
                            Some((v, n))
                        })
                        .collect();
                    if corners.len() < 3 {
                        continue;
                    }
                    // fan-triangulate n-gons
                    for k in 1..corners.len() - 1 {
                        let tri = [corners[0], corners[k], corners[k + 1]];
                        let flat = {
                            let (a, b, c) = (
                                positions[tri[0].0],
                                positions[tri[1].0],
                                positions[tri[2].0],
                            );
                            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                            let n = [
                                u[1] * v[2] - u[2] * v[1],
                                u[2] * v[0] - u[0] * v[2],
                                u[0] * v[1] - u[1] * v[0],
                            ];
                            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                            if len > 1e-12 {
                                [n[0] / len, n[1] / len, n[2] / len]
                            } else {
                                [0.0, 0.0, 1.0]
                            }
                        };
                        for (vi, ni) in tri {
                            let normal = ni.map(|i| normals[i]).unwrap_or(flat);
                            mesh.push_vertex(positions[vi], normal, 0.0);
                        }
                    }
                }
                _ => {}
            }
        }

        if mesh.vertices.is_empty() {
            return Err("OBJ contained no faces".to_string());
        }
        mesh.indices = (0..(mesh.vertices.len() / FLOATS_PER_VERTEX) as u32).collect();
        mesh.recompute_bounds();
        Ok(mesh)
    }

    /// Recompute `bounds_min`/`bounds_max` from the current vertices.
    pub fn recompute_bounds(&mut self) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for v in self.vertices.chunks(FLOATS_PER_VERTEX) {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        if self.vertices.is_empty() {
            min = [0.0; 3];
            max = [0.0; 3];
        }
        self.bounds_min = min;
        self.bounds_max = max;
    }

    /// URDF `<box size="x y z">`: axis-aligned, centred on the link origin.
    ///
    /// The primitive generators below build geometry in URDF's own frame
    /// (Z up, centred at the origin) — unlike the older Y-up helpers in this
    /// file, which exist for the viewer's own scenery.
    pub fn urdf_box(sx: f32, sy: f32, sz: f32) -> Self {
        let (hx, hy, hz) = (sx * 0.5, sy * 0.5, sz * 0.5);
        let mut mesh = MeshData {
            bounds_min: [-hx, -hy, -hz],
            bounds_max: [hx, hy, hz],
            ..Default::default()
        };
        // (normal, four corners ccw seen from outside)
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            ([0.0, 0.0, 1.0],  [[-hx, -hy, hz], [hx, -hy, hz], [hx, hy, hz], [-hx, hy, hz]]),
            ([0.0, 0.0, -1.0], [[hx, -hy, -hz], [-hx, -hy, -hz], [-hx, hy, -hz], [hx, hy, -hz]]),
            ([1.0, 0.0, 0.0],  [[hx, -hy, hz], [hx, -hy, -hz], [hx, hy, -hz], [hx, hy, hz]]),
            ([-1.0, 0.0, 0.0], [[-hx, -hy, -hz], [-hx, -hy, hz], [-hx, hy, hz], [-hx, hy, -hz]]),
            ([0.0, 1.0, 0.0],  [[-hx, hy, hz], [hx, hy, hz], [hx, hy, -hz], [-hx, hy, -hz]]),
            ([0.0, -1.0, 0.0], [[-hx, -hy, -hz], [hx, -hy, -hz], [hx, -hy, hz], [-hx, -hy, hz]]),
        ];
        for (id, (normal, quad)) in faces.iter().enumerate() {
            for &[a, b, c] in &[[0usize, 1, 2], [0, 2, 3]] {
                for &corner in &[quad[a], quad[b], quad[c]] {
                    mesh.push_vertex(corner, *normal, id as f32);
                }
            }
        }
        mesh.indices = (0..(mesh.vertices.len() / FLOATS_PER_VERTEX) as u32).collect();
        mesh
    }

    /// URDF `<cylinder radius="r" length="l">`: **Z axis**, centred on the
    /// link origin (URDF's convention, not the Y-up `cylinder()` above).
    pub fn urdf_cylinder(radius: f32, length: f32, segments: usize) -> Self {
        let segments = segments.max(3);
        let hz = length * 0.5;
        let mut mesh = MeshData {
            bounds_min: [-radius, -radius, -hz],
            bounds_max: [radius, radius, hz],
            ..Default::default()
        };
        let at = |i: usize| {
            let a = (i as f32 / segments as f32) * std::f32::consts::TAU;
            (radius * a.cos(), radius * a.sin(), a.cos(), a.sin())
        };
        for i in 0..segments {
            let (x0, y0, nx0, ny0) = at(i);
            let (x1, y1, nx1, ny1) = at(i + 1);
            let n0 = [nx0, ny0, 0.0];
            let n1 = [nx1, ny1, 0.0];
            // side
            mesh.push_vertex([x0, y0, -hz], n0, 0.0);
            mesh.push_vertex([x1, y1, -hz], n1, 0.0);
            mesh.push_vertex([x1, y1, hz], n1, 0.0);
            mesh.push_vertex([x0, y0, -hz], n0, 0.0);
            mesh.push_vertex([x1, y1, hz], n1, 0.0);
            mesh.push_vertex([x0, y0, hz], n0, 0.0);
            // caps
            let up = [0.0, 0.0, 1.0];
            let down = [0.0, 0.0, -1.0];
            mesh.push_vertex([0.0, 0.0, hz], up, 1.0);
            mesh.push_vertex([x0, y0, hz], up, 1.0);
            mesh.push_vertex([x1, y1, hz], up, 1.0);
            mesh.push_vertex([0.0, 0.0, -hz], down, 2.0);
            mesh.push_vertex([x1, y1, -hz], down, 2.0);
            mesh.push_vertex([x0, y0, -hz], down, 2.0);
        }
        mesh.indices = (0..(mesh.vertices.len() / FLOATS_PER_VERTEX) as u32).collect();
        mesh
    }

    /// URDF `<sphere radius="r">`, centred on the link origin.
    pub fn urdf_sphere(radius: f32, rings: usize, segments: usize) -> Self {
        let rings = rings.max(2);
        let segments = segments.max(3);
        let mut mesh = MeshData {
            bounds_min: [-radius; 3],
            bounds_max: [radius; 3],
            ..Default::default()
        };
        let point = |ring: usize, seg: usize| {
            let phi = (ring as f32 / rings as f32) * std::f32::consts::PI; // 0..pi from +Z
            let theta = (seg as f32 / segments as f32) * std::f32::consts::TAU;
            let n = [phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos()];
            ([n[0] * radius, n[1] * radius, n[2] * radius], n)
        };
        for r in 0..rings {
            for sgm in 0..segments {
                let (p00, n00) = point(r, sgm);
                let (p01, n01) = point(r, sgm + 1);
                let (p10, n10) = point(r + 1, sgm);
                let (p11, n11) = point(r + 1, sgm + 1);
                // skip the degenerate triangle at each pole
                if r > 0 {
                    mesh.push_vertex(p00, n00, 0.0);
                    mesh.push_vertex(p10, n10, 0.0);
                    mesh.push_vertex(p01, n01, 0.0);
                }
                if r + 1 < rings {
                    mesh.push_vertex(p01, n01, 0.0);
                    mesh.push_vertex(p10, n10, 0.0);
                    mesh.push_vertex(p11, n11, 0.0);
                }
            }
        }
        mesh.indices = (0..(mesh.vertices.len() / FLOATS_PER_VERTEX) as u32).collect();
        mesh
    }

    /// Append one interleaved vertex: pos(3), id(1), normal(3), uv(2).
    pub(crate) fn push_vertex_pub(&mut self, pos: [f32; 3], normal: [f32; 3], id: f32) {
        self.push_vertex(pos, normal, id)
    }

    fn push_vertex(&mut self, pos: [f32; 3], normal: [f32; 3], id: f32) {
        self.vertices.extend_from_slice(&pos);
        self.vertices.push(id);
        self.vertices.extend_from_slice(&normal);
        self.vertices.push(0.0);
        self.vertices.push(0.0);
    }

    /// Load mesh from STL file
    pub fn from_stl<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("Failed to open STL file: {}", e))?;
        Self::from_stl_reader(&mut BufReader::new(file))
    }

    /// Load mesh from in-memory STL bytes (wasm asset path)
    pub fn from_stl_bytes(bytes: &[u8]) -> Result<Self, String> {
        Self::from_stl_reader(&mut std::io::Cursor::new(bytes))
    }

    /// Load mesh from any seekable reader of STL data
    pub fn from_stl_reader<R: std::io::Read + std::io::Seek>(reader: &mut R) -> Result<Self, String> {
        let stl = stl_io::read_stl(reader)
            .map_err(|e| format!("Failed to parse STL: {}", e))?;

        let mut mesh = MeshData {
            bounds_min: [f32::MAX; 3],
            bounds_max: [f32::MIN; 3],
            ..Default::default()
        };

        for (tri_idx, face) in stl.faces.iter().enumerate() {
            let normal = face.normal;

            for (local_idx, &vert_idx) in face.vertices.iter().enumerate() {
                let vertex = &stl.vertices[vert_idx];

                // Position
                mesh.vertices.push(vertex[0]);
                mesh.vertices.push(vertex[1]);
                mesh.vertices.push(vertex[2]);

                // Update bounds
                for i in 0..3 {
                    mesh.bounds_min[i] = mesh.bounds_min[i].min(vertex[i]);
                    mesh.bounds_max[i] = mesh.bounds_max[i].max(vertex[i]);
                }

                // ID (face index)
                mesh.vertices.push(tri_idx as f32);

                // Normal
                mesh.vertices.push(normal[0]);
                mesh.vertices.push(normal[1]);
                mesh.vertices.push(normal[2]);

                // UV (simple planar mapping)
                mesh.vertices.push(vertex[0]);
                mesh.vertices.push(vertex[1]);

                // Index
                mesh.indices.push((tri_idx * 3 + local_idx) as u32);
            }
        }

        Ok(mesh)
    }

    /// Create a ground plane mesh (grid pattern)
    pub fn ground_plane(size: f32, y_pos: f32) -> Self {
        let mut mesh = MeshData::default();
        let half = size / 2.0;
        let normal = [0.0, 1.0, 0.0];

        mesh.bounds_min = [-half, y_pos, -half];
        mesh.bounds_max = [half, y_pos, half];

        let grid_size = 10;
        let cell_size = size / grid_size as f32;

        for i in 0..grid_size {
            for j in 0..grid_size {
                let x0 = -half + i as f32 * cell_size;
                let x1 = x0 + cell_size;
                let z0 = -half + j as f32 * cell_size;
                let z1 = z0 + cell_size;

                let face_idx = (i * grid_size + j) as f32;

                let verts = [
                    [x0, y_pos, z0],
                    [x1, y_pos, z0],
                    [x1, y_pos, z1],
                    [x0, y_pos, z1],
                ];

                // Triangle 1
                for v in [&verts[0], &verts[1], &verts[2]] {
                    mesh.vertices.extend_from_slice(v);
                    mesh.vertices.push(face_idx);
                    mesh.vertices.extend_from_slice(&normal);
                    mesh.vertices.push(v[0] / size + 0.5);
                    mesh.vertices.push(v[2] / size + 0.5);
                }

                // Triangle 2
                for v in [&verts[0], &verts[2], &verts[3]] {
                    mesh.vertices.extend_from_slice(v);
                    mesh.vertices.push(face_idx);
                    mesh.vertices.extend_from_slice(&normal);
                    mesh.vertices.push(v[0] / size + 0.5);
                    mesh.vertices.push(v[2] / size + 0.5);
                }
            }
        }

        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Create a cylinder mesh for axis visualization
    pub fn cylinder(radius: f32, height: f32, segments: usize) -> Self {
        let mut mesh = MeshData {
            bounds_min: [-radius, 0.0, -radius],
            bounds_max: [radius, height, radius],
            ..Default::default()
        };

        let half_height = height / 2.0;

        for i in 0..segments {
            let angle0 = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let angle1 = ((i + 1) as f32 / segments as f32) * std::f32::consts::TAU;

            let x0 = radius * angle0.cos();
            let z0 = radius * angle0.sin();
            let x1 = radius * angle1.cos();
            let z1 = radius * angle1.sin();

            // Side face
            let normal0 = [angle0.cos(), 0.0, angle0.sin()];
            let normal1 = [angle1.cos(), 0.0, angle1.sin()];

            // Triangle 1
            let verts = [
                ([x0, -half_height, z0], normal0),
                ([x0, half_height, z0], normal0),
                ([x1, half_height, z1], normal1),
            ];
            for (pos, norm) in &verts {
                mesh.vertices.extend_from_slice(pos);
                mesh.vertices.push(i as f32);
                mesh.vertices.extend_from_slice(norm);
                mesh.vertices.push(0.0);
                mesh.vertices.push(0.0);
            }

            // Triangle 2
            let verts = [
                ([x0, -half_height, z0], normal0),
                ([x1, half_height, z1], normal1),
                ([x1, -half_height, z1], normal1),
            ];
            for (pos, norm) in &verts {
                mesh.vertices.extend_from_slice(pos);
                mesh.vertices.push(i as f32);
                mesh.vertices.extend_from_slice(norm);
                mesh.vertices.push(0.0);
                mesh.vertices.push(0.0);
            }

            // Top cap
            let top_normal = [0.0, 1.0, 0.0];
            let top_verts = [
                [0.0, half_height, 0.0],
                [x0, half_height, z0],
                [x1, half_height, z1],
            ];
            for pos in &top_verts {
                mesh.vertices.extend_from_slice(pos);
                mesh.vertices.push(i as f32);
                mesh.vertices.extend_from_slice(&top_normal);
                mesh.vertices.push(0.0);
                mesh.vertices.push(0.0);
            }

            // Bottom cap
            let bottom_normal = [0.0, -1.0, 0.0];
            let bottom_verts = [
                [0.0, -half_height, 0.0],
                [x1, -half_height, z1],
                [x0, -half_height, z0],
            ];
            for pos in &bottom_verts {
                mesh.vertices.extend_from_slice(pos);
                mesh.vertices.push(i as f32);
                mesh.vertices.extend_from_slice(&bottom_normal);
                mesh.vertices.push(0.0);
                mesh.vertices.push(0.0);
            }
        }

        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Create a hemisphere mesh for sky dome (normals point inward)
    pub fn hemisphere(radius: f32, lat_segments: usize, lon_segments: usize) -> Self {
        let mut mesh = MeshData {
            bounds_min: [-radius, 0.0, -radius],
            bounds_max: [radius, radius, radius],
            ..Default::default()
        };

        for lat in 0..lat_segments {
            let lat0 = (lat as f32 / lat_segments as f32) * std::f32::consts::FRAC_PI_2;
            let lat1 = ((lat + 1) as f32 / lat_segments as f32) * std::f32::consts::FRAC_PI_2;

            let y0 = radius * lat0.sin();
            let y1 = radius * lat1.sin();
            let r0 = radius * lat0.cos();
            let r1 = radius * lat1.cos();

            for lon in 0..lon_segments {
                let lon0 = (lon as f32 / lon_segments as f32) * std::f32::consts::TAU;
                let lon1 = ((lon + 1) as f32 / lon_segments as f32) * std::f32::consts::TAU;

                let p00 = [r0 * lon0.cos(), y0, r0 * lon0.sin()];
                let p01 = [r0 * lon1.cos(), y0, r0 * lon1.sin()];
                let p10 = [r1 * lon0.cos(), y1, r1 * lon0.sin()];
                let p11 = [r1 * lon1.cos(), y1, r1 * lon1.sin()];

                // Normals point inward
                let n00 = [-lon0.cos() * lat0.cos(), -lat0.sin(), -lon0.sin() * lat0.cos()];
                let n01 = [-lon1.cos() * lat0.cos(), -lat0.sin(), -lon1.sin() * lat0.cos()];
                let n10 = [-lon0.cos() * lat1.cos(), -lat1.sin(), -lon0.sin() * lat1.cos()];
                let n11 = [-lon1.cos() * lat1.cos(), -lat1.sin(), -lon1.sin() * lat1.cos()];

                let face_idx = (lat * lon_segments + lon) as f32;

                // Triangle 1
                for (pos, norm) in [(p00, n00), (p10, n10), (p11, n11)] {
                    mesh.vertices.extend_from_slice(&pos);
                    mesh.vertices.push(face_idx);
                    mesh.vertices.extend_from_slice(&norm);
                    mesh.vertices.push(0.0);
                    mesh.vertices.push(0.0);
                }

                // Triangle 2
                for (pos, norm) in [(p00, n00), (p11, n11), (p01, n01)] {
                    mesh.vertices.extend_from_slice(&pos);
                    mesh.vertices.push(face_idx);
                    mesh.vertices.extend_from_slice(&norm);
                    mesh.vertices.push(0.0);
                    mesh.vertices.push(0.0);
                }
            }
        }

        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Create a simple test cube mesh (faces pointing outward, for external viewing)
    pub fn test_cube(size: f32) -> Self {
        let mut mesh = MeshData::default();
        let s = size / 2.0;

        let faces = [
            // Front face (z+)
            ([s, -s, s], [s, s, s], [-s, s, s], [-s, -s, s], [0.0, 0.0, 1.0]),
            // Back face (z-)
            ([-s, -s, -s], [-s, s, -s], [s, s, -s], [s, -s, -s], [0.0, 0.0, -1.0]),
            // Top face (y+)
            ([-s, s, -s], [-s, s, s], [s, s, s], [s, s, -s], [0.0, 1.0, 0.0]),
            // Bottom face (y-)
            ([-s, -s, s], [-s, -s, -s], [s, -s, -s], [s, -s, s], [0.0, -1.0, 0.0]),
            // Right face (x+)
            ([s, -s, s], [s, -s, -s], [s, s, -s], [s, s, s], [1.0, 0.0, 0.0]),
            // Left face (x-)
            ([-s, -s, -s], [-s, -s, s], [-s, s, s], [-s, s, -s], [-1.0, 0.0, 0.0]),
        ];

        mesh.bounds_min = [-s, -s, -s];
        mesh.bounds_max = [s, s, s];

        for (face_idx, (v0, v1, v2, v3, normal)) in faces.iter().enumerate() {
            // Triangle 1: v0, v1, v2
            for v in [v0, v1, v2] {
                mesh.vertices.extend_from_slice(v);
                mesh.vertices.push(face_idx as f32);
                mesh.vertices.extend_from_slice(normal);
                mesh.vertices.push(v[0]);
                mesh.vertices.push(v[1]);
            }

            // Triangle 2: v0, v2, v3
            for v in [v0, v2, v3] {
                mesh.vertices.extend_from_slice(v);
                mesh.vertices.push(face_idx as f32);
                mesh.vertices.extend_from_slice(normal);
                mesh.vertices.push(v[0]);
                mesh.vertices.push(v[1]);
            }
        }

        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Create a skybox cube mesh (faces pointing inward, for internal viewing)
    pub fn skybox_cube(size: f32) -> Self {
        let mut mesh = MeshData::default();
        let s = size / 2.0;

        // Same faces as test_cube but with reversed winding order (v0, v2, v1) and (v0, v3, v2)
        // Normals point inward for proper lighting if needed
        let faces = [
            // Front face (z+) - viewed from inside
            ([s, -s, s], [s, s, s], [-s, s, s], [-s, -s, s], [0.0, 0.0, -1.0]),
            // Back face (z-)
            ([-s, -s, -s], [-s, s, -s], [s, s, -s], [s, -s, -s], [0.0, 0.0, 1.0]),
            // Top face (y+)
            ([-s, s, -s], [-s, s, s], [s, s, s], [s, s, -s], [0.0, -1.0, 0.0]),
            // Bottom face (y-)
            ([-s, -s, s], [-s, -s, -s], [s, -s, -s], [s, -s, s], [0.0, 1.0, 0.0]),
            // Right face (x+)
            ([s, -s, s], [s, -s, -s], [s, s, -s], [s, s, s], [-1.0, 0.0, 0.0]),
            // Left face (x-)
            ([-s, -s, -s], [-s, -s, s], [-s, s, s], [-s, s, -s], [1.0, 0.0, 0.0]),
        ];

        mesh.bounds_min = [-s, -s, -s];
        mesh.bounds_max = [s, s, s];

        for (face_idx, (v0, v1, v2, v3, normal)) in faces.iter().enumerate() {
            // Triangle 1: v0, v2, v1 (reversed winding)
            for v in [v0, v2, v1] {
                mesh.vertices.extend_from_slice(v);
                mesh.vertices.push(face_idx as f32);
                mesh.vertices.extend_from_slice(normal);
                mesh.vertices.push(v[0]);
                mesh.vertices.push(v[1]);
            }

            // Triangle 2: v0, v3, v2 (reversed winding)
            for v in [v0, v3, v2] {
                mesh.vertices.extend_from_slice(v);
                mesh.vertices.push(face_idx as f32);
                mesh.vertices.extend_from_slice(normal);
                mesh.vertices.push(v[0]);
                mesh.vertices.push(v[1]);
            }
        }

        let num_vertices = mesh.vertices.len() / FLOATS_PER_VERTEX;
        mesh.indices = (0..num_vertices as u32).collect();

        mesh
    }

    /// Combine multiple meshes into one
    pub fn combine(meshes: Vec<MeshData>) -> Self {
        let mut combined = MeshData {
            bounds_min: [f32::MAX; 3],
            bounds_max: [f32::MIN; 3],
            // The first colour any part brought along survives the merge —
            // ..Default::default() silently reset it to None, which cut the
            // one wire COLLADA colours had toward the screen.
            color: meshes.iter().find_map(|m| m.color),
            ..Default::default()
        };

        let mut vertex_offset = 0u32;

        for mesh in meshes {
            for i in 0..3 {
                combined.bounds_min[i] = combined.bounds_min[i].min(mesh.bounds_min[i]);
                combined.bounds_max[i] = combined.bounds_max[i].max(mesh.bounds_max[i]);
            }

            combined.vertices.extend_from_slice(&mesh.vertices);

            for idx in mesh.indices {
                combined.indices.push(idx + vertex_offset);
            }

            vertex_offset += (mesh.vertices.len() / FLOATS_PER_VERTEX) as u32;
        }

        combined
    }

    /// Load all robot STL files from a directory
    pub fn load_robot_meshes(assets_dir: &str) -> Result<Self, String> {
        let stl_files = [
            "Base.stl",
            "Base_Motor.stl",
            "Rotation_Pitch.stl",
            "Rotation_Pitch_Motor.stl",
            "Upper_Arm.stl",
            "Upper_Arm_Motor.stl",
            "Lower_Arm.stl",
            "Lower_Arm_Motor.stl",
            "Wrist_Pitch_Roll.stl",
            "Wrist_Pitch_Roll_Motor.stl",
            "Fixed_Jaw.stl",
            "Fixed_Jaw_Motor.stl",
            "Moving_Jaw.stl",
        ];

        let mut meshes = Vec::new();
        for file in &stl_files {
            let path = format!("{}/{}", assets_dir, file);
            match MeshData::from_stl(&path) {
                Ok(mesh) => {
                    meshes.push(mesh);
                }
                Err(e) => {
                    eprintln!("Warning: Failed to load {}: {}", file, e);
                }
            }
        }

        if meshes.is_empty() {
            return Err("No meshes loaded".to_string());
        }

        let mut combined = MeshData::combine(meshes);
        combined.normalize();
        combined.make_double_sided();
        Ok(combined)
    }

    /// Make mesh double-sided by duplicating triangles with reversed winding
    /// Replace per-facet normals with smoothed vertex normals.
    ///
    /// STL stores one normal per triangle and no vertex sharing, so a curved
    /// CAD surface renders as visible facets — every tessellation edge shows
    /// up as a shading discontinuity. This welds vertices by position and
    /// averages the facet normals meeting there, weighted by triangle area
    /// (via the un-normalized cross product, which is proportional to it).
    ///
    /// `crease_deg` keeps genuine edges sharp: a facet only contributes to a
    /// vertex's averaged normal when it lies within that angle of the
    /// vertex's own facet, so a cylinder's barrel smooths while the rim where
    /// it meets its end cap stays a hard line. 0 disables smoothing entirely.
    ///
    /// Positions are quantized to weld coincident-but-not-bit-identical
    /// vertices, which STL exporters produce routinely.
    pub fn smooth_normals(&mut self, crease_deg: f32) {
        if crease_deg <= 0.0 {
            return;
        }
        let vcount = self.vertices.len() / FLOATS_PER_VERTEX;
        if vcount == 0 {
            return;
        }
        let cos_crease = crease_deg.to_radians().cos();

        // Weld key: 1e-5 m grid. Finer than any real STL tolerance, coarse
        // enough to catch float noise between adjacent facets.
        const WELD: f32 = 1e5;
        let key = |v: &[f32]| -> (i64, i64, i64) {
            (
                (v[0] * WELD).round() as i64,
                (v[1] * WELD).round() as i64,
                (v[2] * WELD).round() as i64,
            )
        };

        // Bucket every vertex by welded position, carrying its facet normal.
        let mut buckets: std::collections::HashMap<(i64, i64, i64), Vec<usize>> =
            std::collections::HashMap::new();
        for i in 0..vcount {
            let base = i * FLOATS_PER_VERTEX;
            buckets
                .entry(key(&self.vertices[base..base + 3]))
                .or_default()
                .push(i);
        }

        // Facet normals scaled by area: recompute from the triangle rather
        // than trusting the stored one, since STL exporters are allowed to
        // write zero normals and some do.
        let tri_count = self.indices.len() / 3;
        let mut face_normal = vec![[0.0f32; 3]; tri_count];
        let mut vert_face = vec![usize::MAX; vcount];
        for t in 0..tri_count {
            let (i0, i1, i2) = (
                self.indices[t * 3] as usize,
                self.indices[t * 3 + 1] as usize,
                self.indices[t * 3 + 2] as usize,
            );
            let p = |i: usize| {
                let b = i * FLOATS_PER_VERTEX;
                glam::Vec3::new(self.vertices[b], self.vertices[b + 1], self.vertices[b + 2])
            };
            let (a, b, c) = (p(i0), p(i1), p(i2));
            let n = (b - a).cross(c - a); // length ∝ 2 * area
            face_normal[t] = [n.x, n.y, n.z];
            for i in [i0, i1, i2] {
                if i < vcount {
                    vert_face[i] = t;
                }
            }
        }

        let mut out = vec![0.0f32; vcount * 3];
        for ids in buckets.values() {
            for &i in ids {
                let own = vert_face[i];
                if own == usize::MAX {
                    continue;
                }
                let own_n = glam::Vec3::from(face_normal[own]).normalize_or_zero();
                let mut acc = glam::Vec3::ZERO;
                for &j in ids {
                    let f = vert_face[j];
                    if f == usize::MAX {
                        continue;
                    }
                    let fj = glam::Vec3::from(face_normal[f]);
                    // Within the crease angle? (compare normalized, accumulate
                    // un-normalized so bigger triangles weigh more)
                    if own_n.dot(fj.normalize_or_zero()) >= cos_crease {
                        acc += fj;
                    }
                }
                let n = if acc.length_squared() > 1e-20 {
                    acc.normalize()
                } else {
                    own_n
                };
                out[i * 3] = n.x;
                out[i * 3 + 1] = n.y;
                out[i * 3 + 2] = n.z;
            }
        }

        for i in 0..vcount {
            let base = i * FLOATS_PER_VERTEX;
            self.vertices[base + 4] = out[i * 3];
            self.vertices[base + 5] = out[i * 3 + 1];
            self.vertices[base + 6] = out[i * 3 + 2];
        }
    }

    pub fn make_double_sided(&mut self) {
        let original_vertex_count = self.vertices.len() / FLOATS_PER_VERTEX;
        let original_index_count = self.indices.len();

        // Duplicate vertices with flipped normals
        let mut new_vertices = self.vertices.clone();
        for i in 0..original_vertex_count {
            let base = i * FLOATS_PER_VERTEX;
            // Copy position
            new_vertices.push(self.vertices[base]);
            new_vertices.push(self.vertices[base + 1]);
            new_vertices.push(self.vertices[base + 2]);
            // Copy id
            new_vertices.push(self.vertices[base + 3]);
            // Flip normal
            new_vertices.push(-self.vertices[base + 4]);
            new_vertices.push(-self.vertices[base + 5]);
            new_vertices.push(-self.vertices[base + 6]);
            // Copy uv
            new_vertices.push(self.vertices[base + 7]);
            new_vertices.push(self.vertices[base + 8]);
        }
        self.vertices = new_vertices;

        // Duplicate indices with reversed winding order
        for i in 0..(original_index_count / 3) {
            let base = i * 3;
            self.indices.push(self.indices[base] + original_vertex_count as u32);
            self.indices.push(self.indices[base + 2] + original_vertex_count as u32);
            self.indices.push(self.indices[base + 1] + original_vertex_count as u32);
        }
    }

    /// Apply uniform scale to all vertices
    pub fn apply_scale(&mut self, scale: f32) {
        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;
        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;
            self.vertices[base] *= scale;
            self.vertices[base + 1] *= scale;
            self.vertices[base + 2] *= scale;
        }
        for i in 0..3 {
            self.bounds_min[i] *= scale;
            self.bounds_max[i] *= scale;
        }
    }

    /// Apply a 4x4 transform matrix to all vertices
    pub fn apply_transform(&mut self, m: &[f32; 16]) {
        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;

        self.bounds_min = [f32::MAX; 3];
        self.bounds_max = [f32::MIN; 3];

        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;

            // Transform position
            let px = self.vertices[base];
            let py = self.vertices[base + 1];
            let pz = self.vertices[base + 2];

            let new_x = m[0] * px + m[4] * py + m[8] * pz + m[12];
            let new_y = m[1] * px + m[5] * py + m[9] * pz + m[13];
            let new_z = m[2] * px + m[6] * py + m[10] * pz + m[14];

            self.vertices[base] = new_x;
            self.vertices[base + 1] = new_y;
            self.vertices[base + 2] = new_z;

            self.bounds_min[0] = self.bounds_min[0].min(new_x);
            self.bounds_min[1] = self.bounds_min[1].min(new_y);
            self.bounds_min[2] = self.bounds_min[2].min(new_z);
            self.bounds_max[0] = self.bounds_max[0].max(new_x);
            self.bounds_max[1] = self.bounds_max[1].max(new_y);
            self.bounds_max[2] = self.bounds_max[2].max(new_z);

            // Transform normal
            let nx = self.vertices[base + 4];
            let ny = self.vertices[base + 5];
            let nz = self.vertices[base + 6];

            let new_nx = m[0] * nx + m[4] * ny + m[8] * nz;
            let new_ny = m[1] * nx + m[5] * ny + m[9] * nz;
            let new_nz = m[2] * nx + m[6] * ny + m[10] * nz;

            let len = (new_nx * new_nx + new_ny * new_ny + new_nz * new_nz).sqrt();
            if len > 0.0001 {
                self.vertices[base + 4] = new_nx / len;
                self.vertices[base + 5] = new_ny / len;
                self.vertices[base + 6] = new_nz / len;
            }
        }
    }

    /// Center the mesh at origin and scale to fit in unit cube
    pub fn normalize(&mut self) {
        let center = [
            (self.bounds_min[0] + self.bounds_max[0]) / 2.0,
            (self.bounds_min[1] + self.bounds_max[1]) / 2.0,
            (self.bounds_min[2] + self.bounds_max[2]) / 2.0,
        ];

        let extent = [
            self.bounds_max[0] - self.bounds_min[0],
            self.bounds_max[1] - self.bounds_min[1],
            self.bounds_max[2] - self.bounds_min[2],
        ];

        let max_extent = extent[0].max(extent[1]).max(extent[2]).max(0.001);
        let scale = 1.0 / max_extent;

        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;
        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;
            self.vertices[base] = (self.vertices[base] - center[0]) * scale;
            self.vertices[base + 1] = (self.vertices[base + 1] - center[1]) * scale;
            self.vertices[base + 2] = (self.vertices[base + 2] - center[2]) * scale;
        }

        self.bounds_min = [-0.5, -0.5, -0.5];
        self.bounds_max = [0.5, 0.5, 0.5];
    }

    /// Get the center point of the mesh
    pub fn center(&self) -> [f32; 3] {
        [
            (self.bounds_min[0] + self.bounds_max[0]) / 2.0,
            (self.bounds_min[1] + self.bounds_max[1]) / 2.0,
            (self.bounds_min[2] + self.bounds_max[2]) / 2.0,
        ]
    }

    /// Get the extent (size) of the mesh
    pub fn extent(&self) -> [f32; 3] {
        [
            self.bounds_max[0] - self.bounds_min[0],
            self.bounds_max[1] - self.bounds_min[1],
            self.bounds_max[2] - self.bounds_min[2],
        ]
    }

    /// Get vertex count
    pub fn vertex_count(&self) -> usize {
        self.vertices.len() / FLOATS_PER_VERTEX
    }

    /// Get triangle count
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cube_creation() {
        let cube = MeshData::test_cube(1.0);
        assert_eq!(cube.vertex_count(), 36); // 6 faces * 2 triangles * 3 vertices
        assert_eq!(cube.triangle_count(), 12); // 6 faces * 2 triangles
    }

    #[test]
    fn test_cylinder_creation() {
        let cyl = MeshData::cylinder(1.0, 2.0, 8);
        assert!(cyl.vertex_count() > 0);
    }

    #[test]
    fn test_combine_meshes() {
        let cube1 = MeshData::test_cube(1.0);
        let cube2 = MeshData::test_cube(1.0);
        let combined = MeshData::combine(vec![cube1.clone(), cube2]);
        assert_eq!(combined.vertex_count(), cube1.vertex_count() * 2);
    }
}
