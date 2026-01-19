//! Makepad URDF Player
//!
//! A Makepad-based URDF robot viewer with embeddable RobotView widget.
//!
//! # Modules
//!
//! - `mesh`: 3D mesh data and STL loading (re-exports from render)
//! - `render`: 3D rendering primitives (MeshData, DrawMesh, etc.)
//! - `robot_view`: The main RobotView widget for displaying robots
//! - `robot`: Robot data model, URDF loading, and forward kinematics
//! - `camera`: 3D camera with orbital controls
//! - `error`: Error types for robot loading and validation

use makepad_widgets::*;

pub mod render;
pub mod mesh;
pub mod robot_view;
pub mod robot;
pub mod camera;
pub mod error;
pub mod profiling;

pub fn live_design(cx: &mut Cx) {
    render::live_design(cx);
    mesh::live_design(cx);
    robot_view::live_design(cx);
}
