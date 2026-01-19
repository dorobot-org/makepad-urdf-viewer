//! Mesh data structures for 3D rendering
//!
//! Provides mesh loading from STL files and primitive generation.

use makepad_widgets::*;
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
}

impl MeshData {
    /// Load mesh from STL file
    pub fn from_stl<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let file = File::open(path.as_ref())
            .map_err(|e| format!("Failed to open STL file: {}", e))?;
        let mut reader = BufReader::new(file);

        let stl = stl_io::read_stl(&mut reader)
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

    /// Create a simple test cube mesh
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

    /// Combine multiple meshes into one
    pub fn combine(meshes: Vec<MeshData>) -> Self {
        let mut combined = MeshData {
            bounds_min: [f32::MAX; 3],
            bounds_max: [f32::MIN; 3],
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
    pub fn apply_transform(&mut self, m: &Mat4) {
        let num_vertices = self.vertices.len() / FLOATS_PER_VERTEX;

        self.bounds_min = [f32::MAX; 3];
        self.bounds_max = [f32::MIN; 3];

        for i in 0..num_vertices {
            let base = i * FLOATS_PER_VERTEX;

            // Transform position
            let px = self.vertices[base];
            let py = self.vertices[base + 1];
            let pz = self.vertices[base + 2];

            let new_x = m.v[0] * px + m.v[4] * py + m.v[8] * pz + m.v[12];
            let new_y = m.v[1] * px + m.v[5] * py + m.v[9] * pz + m.v[13];
            let new_z = m.v[2] * px + m.v[6] * py + m.v[10] * pz + m.v[14];

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

            let new_nx = m.v[0] * nx + m.v[4] * ny + m.v[8] * nz;
            let new_ny = m.v[1] * nx + m.v[5] * ny + m.v[9] * nz;
            let new_nz = m.v[2] * nx + m.v[6] * ny + m.v[10] * nz;

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
