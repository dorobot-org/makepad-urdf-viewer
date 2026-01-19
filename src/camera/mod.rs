//! Camera module for 3D viewing
//!
//! Provides orbital camera with smooth controls for robot visualization.

mod camera3d;
mod controller;

pub use camera3d::{Camera3D, Frustum};
pub use controller::{CameraController, CameraEvent};
