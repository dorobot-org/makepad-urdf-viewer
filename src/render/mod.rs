//! Render module for 3D graphics
//!
//! This module provides rendering primitives for 3D robot visualization:
//!
//! - `MeshData`: CPU-side mesh data loaded from STL files
//! - `GeometryMesh3D`: GPU geometry buffers
//! - `DrawMesh`: Shader for rendering solid meshes with lighting
//! - `DrawGrid`: Shader for rendering ground plane grids
//! - `DrawSkybox`: Shader for rendering sky backgrounds
//!
//! # Example
//!
//! ```ignore
//! use makepad_urdf_player::render::{MeshData, DrawMesh, GeometryMesh3D};
//!
//! // Load mesh from STL
//! let mesh = MeshData::from_stl("path/to/mesh.stl")?;
//!
//! // Create primitives
//! let cube = MeshData::test_cube(1.0);
//! let cylinder = MeshData::cylinder(0.1, 1.0, 16);
//! ```

pub mod mesh;
pub mod geometry;
pub mod draw;

pub use mesh::{MeshData, FLOATS_PER_VERTEX};
pub use geometry::GeometryMesh3D;
pub use draw::{DrawMesh, DrawGrid, DrawSkybox};

use makepad_widgets::Cx;

/// Register live design for render module
pub fn live_design(cx: &mut Cx) {
    // Geometry must be registered first since draw depends on it
    geometry::live_design(cx);
    draw::live_design(cx);
}
