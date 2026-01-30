//! Camera controller for handling input events

use makepad_widgets::*;
use super::Camera3D;

/// Events that can affect the camera
#[derive(Clone, Debug)]
pub enum CameraEvent {
    /// Start dragging (mouse down)
    DragStart(DVec2),
    /// Continue dragging (mouse move while down)
    DragMove { pos: DVec2, shift: bool },
    /// End dragging (mouse up)
    DragEnd,
    /// Scroll/zoom
    Scroll(f64),
    /// Keyboard zoom in
    ZoomIn,
    /// Keyboard zoom out
    ZoomOut,
    /// Reset to default view
    Reset,
}

/// Camera controller that wraps Camera3D with input handling
#[derive(Clone, Debug, Default)]
pub struct CameraController {
    pub camera: Camera3D,
    is_dragging: bool,
    last_pos: DVec2,
}

impl CameraController {
    /// Create a new controller with default camera
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a controller with a specific camera
    pub fn with_camera(camera: Camera3D) -> Self {
        Self {
            camera,
            is_dragging: false,
            last_pos: DVec2::default(),
        }
    }

    /// Handle a camera event
    pub fn handle_event(&mut self, event: CameraEvent) -> bool {
        match event {
            CameraEvent::DragStart(pos) => {
                self.is_dragging = true;
                self.last_pos = pos;
                false
            }
            CameraEvent::DragMove { pos, shift } => {
                if self.is_dragging {
                    let delta = pos - self.last_pos;
                    self.last_pos = pos;

                    if shift {
                        // Pan mode
                        self.camera.pan(delta.x, delta.y);
                    } else {
                        // Orbit mode
                        self.camera.orbit(delta.x * 0.01, delta.y * 0.01);
                    }
                    true // needs redraw
                } else {
                    false
                }
            }
            CameraEvent::DragEnd => {
                self.is_dragging = false;
                false
            }
            CameraEvent::Scroll(delta) => {
                self.camera.zoom_by_scroll(delta);
                true
            }
            CameraEvent::ZoomIn => {
                self.camera.zoom(0.9);
                true
            }
            CameraEvent::ZoomOut => {
                self.camera.zoom(1.1);
                true
            }
            CameraEvent::Reset => {
                self.camera.reset();
                true
            }
        }
    }

    /// Handle Makepad Hit events directly
    pub fn handle_hit(&mut self, hit: &Hit) -> bool {
        match hit {
            Hit::FingerDown(fe) => {
                self.handle_event(CameraEvent::DragStart(fe.abs))
            }
            Hit::FingerMove(fe) => {
                self.handle_event(CameraEvent::DragMove {
                    pos: fe.abs,
                    shift: fe.modifiers.shift,
                })
            }
            Hit::FingerUp(_) => {
                self.handle_event(CameraEvent::DragEnd)
            }
            Hit::FingerScroll(se) => {
                self.handle_event(CameraEvent::Scroll(se.scroll.y))
            }
            _ => false,
        }
    }

    /// Handle keyboard events
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Equals => self.handle_event(CameraEvent::ZoomIn),
            KeyCode::Minus => self.handle_event(CameraEvent::ZoomOut),
            _ => false,
        }
    }

    /// Update for smooth animation (call each frame)
    pub fn update(&mut self) {
        self.camera.update();
    }

    /// Check if currently dragging
    pub fn is_dragging(&self) -> bool {
        self.is_dragging
    }

    /// Get the camera transform matrix for rendering
    pub fn transform(&self) -> glam::Mat4 {
        self.camera.orbital_transform()
    }

    /// Get camera position for lighting
    pub fn position(&self) -> Vec3 {
        self.camera.position_vec3()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_controller() {
        let controller = CameraController::new();
        assert!(!controller.is_dragging());
    }

    #[test]
    fn test_drag_start() {
        let mut controller = CameraController::new();
        let result = controller.handle_event(CameraEvent::DragStart(dvec2(100.0, 100.0)));
        assert!(!result); // DragStart doesn't need redraw
        assert!(controller.is_dragging());
    }

    #[test]
    fn test_drag_move() {
        let mut controller = CameraController::new();
        controller.handle_event(CameraEvent::DragStart(dvec2(100.0, 100.0)));
        let result = controller.handle_event(CameraEvent::DragMove {
            pos: dvec2(150.0, 100.0),
            shift: false,
        });
        assert!(result); // DragMove needs redraw
    }

    #[test]
    fn test_drag_end() {
        let mut controller = CameraController::new();
        controller.handle_event(CameraEvent::DragStart(dvec2(100.0, 100.0)));
        controller.handle_event(CameraEvent::DragEnd);
        assert!(!controller.is_dragging());
    }

    #[test]
    fn test_scroll_zoom() {
        let mut controller = CameraController::new();
        let initial_distance = controller.camera.distance;
        controller.handle_event(CameraEvent::Scroll(10.0));
        assert!(controller.camera.distance < initial_distance);
    }

    #[test]
    fn test_reset() {
        let mut controller = CameraController::new();
        controller.handle_event(CameraEvent::Scroll(50.0)); // Zoom in a lot
        controller.handle_event(CameraEvent::Reset);
        assert_eq!(controller.camera.distance, 1.5); // Default camera distance
    }

    #[test]
    fn test_pan_mode() {
        let mut controller = CameraController::new();
        controller.handle_event(CameraEvent::DragStart(dvec2(100.0, 100.0)));

        let initial_pan_x = controller.camera.pan_x;
        controller.handle_event(CameraEvent::DragMove {
            pos: dvec2(150.0, 100.0),
            shift: true, // Pan mode
        });
        assert!(controller.camera.pan_x != initial_pan_x);
    }
}
