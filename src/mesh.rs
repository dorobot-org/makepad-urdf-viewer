//! STL Mesh rendering for Makepad
//!
//! This module re-exports from the render module for backward compatibility.
//! New code should use `crate::render` directly.

use makepad_widgets::*;

// Re-export everything from render module
pub use crate::render::mesh::*;
pub use crate::render::geometry::*;
pub use crate::render::draw::*;

/// Register live design (calls render module's live_design)
pub fn live_design(cx: &mut Cx) {
    // The render module's live_design is already called from lib.rs
    // This is kept for backward compatibility with existing code
    let _ = cx;
}
