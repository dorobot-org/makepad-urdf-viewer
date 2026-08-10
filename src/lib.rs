//! Makepad URDF robot viewer library (makepad dev script system).
//!
//! `RobotView` is an embeddable widget; register this crate's script module
//! from your app's `AppMain::script_mod`:
//!
//! ```ignore
//! fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
//!     makepad_widgets::script_mod(vm);
//!     makepad_xr::script_mod(vm);
//!     makepad_urdf_player::script_mod(vm);
//!     self::script_mod(vm)
//! }
//! ```

pub use makepad_widgets;
pub use makepad_xr;

use makepad_widgets::*;

#[cfg(all(feature = "episode", not(target_arch = "wasm32")))]
pub mod episode;
pub mod error;
pub mod mesh;
pub mod profiling;
pub mod render;
pub mod robot;
pub mod robot_view;

/// Register this crate's shaders and widgets into the script VM.
pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    render::draw::script_mod(vm);
    render::draw::composite_shader::script_mod(vm);
    robot_view::script_mod(vm)
}
