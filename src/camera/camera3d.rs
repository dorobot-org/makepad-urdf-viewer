//! 3D Camera with orbital controls and smooth animation

use makepad_widgets::*;

/// 3D Camera with perspective projection and orbital controls
#[derive(Clone, Debug)]
pub struct Camera3D {
    /// Distance from target (orbital radius)
    pub distance: f64,
    /// Horizontal angle (around Y axis)
    pub yaw: f64,
    /// Vertical angle (around X axis)
    pub pitch: f64,
    /// Point the camera is looking at
    pub target: glam::DVec3,
    /// Pan offset in screen space
    pub pan_x: f64,
    pub pan_y: f64,
    /// Vertical field of view in radians
    pub fov: f64,
    /// Near clipping plane distance
    pub near: f64,
    /// Far clipping plane distance
    pub far: f64,
    /// Pitch limits to prevent gimbal lock
    pub min_pitch: f64,
    pub max_pitch: f64,
    /// Smoothing factor (0 = instant, 1 = very smooth)
    pub smoothing: f64,
    // Target values for smooth animation
    target_distance: f64,
    target_yaw: f64,
    target_pitch: f64,
    target_pan_x: f64,
    target_pan_y: f64,
}

impl Default for Camera3D {
    fn default() -> Self {
        Self {
            distance: 1.5,  // Closer to see small robots
            yaw: 0.5,
            pitch: 0.3,
            target: glam::DVec3::new(0.0, 0.15, 0.0),  // Look at robot center (slightly above ground)
            pan_x: 0.0,
            pan_y: 0.0,
            fov: std::f64::consts::FRAC_PI_4, // 45 degrees
            near: 0.01,
            far: 100.0,
            min_pitch: -std::f64::consts::FRAC_PI_2 + 0.1,
            max_pitch: std::f64::consts::FRAC_PI_2 - 0.1,
            smoothing: 0.0,
            target_distance: 1.5,
            target_yaw: 0.5,
            target_pitch: 0.3,
            target_pan_x: 0.0,
            target_pan_y: 0.0,
        }
    }
}

impl Camera3D {
    /// Create a new camera with orbital parameters
    pub fn from_orbital(distance: f64, yaw: f64, pitch: f64) -> Self {
        Self {
            distance,
            yaw,
            pitch,
            target_distance: distance,
            target_yaw: yaw,
            target_pitch: pitch,
            ..Default::default()
        }
    }

    /// Create camera with smoothing enabled
    pub fn with_smoothing(mut self, smoothing: f64) -> Self {
        self.smoothing = smoothing.clamp(0.0, 0.99);
        self
    }

    /// Set pitch limits
    pub fn with_pitch_limits(mut self, min: f64, max: f64) -> Self {
        self.min_pitch = min;
        self.max_pitch = max;
        self
    }

    /// Set clipping planes
    pub fn with_clip_planes(mut self, near: f64, far: f64) -> Self {
        self.near = near;
        self.far = far;
        self
    }

    /// Orbit the camera (rotate around target)
    pub fn orbit(&mut self, delta_yaw: f64, delta_pitch: f64) {
        self.target_yaw += delta_yaw;
        self.target_pitch += delta_pitch;
        self.target_pitch = self.target_pitch.clamp(self.min_pitch, self.max_pitch);

        if self.smoothing == 0.0 {
            self.yaw = self.target_yaw;
            self.pitch = self.target_pitch;
        }
    }

    /// Pan the camera (translate in screen space)
    pub fn pan(&mut self, delta_x: f64, delta_y: f64) {
        let pan_speed = 0.003 * self.distance;
        self.target_pan_x += delta_x * pan_speed;
        self.target_pan_y -= delta_y * pan_speed;

        if self.smoothing == 0.0 {
            self.pan_x = self.target_pan_x;
            self.pan_y = self.target_pan_y;
        }
    }

    /// Zoom the camera (change distance from target)
    pub fn zoom(&mut self, factor: f64) {
        self.target_distance *= factor;
        self.target_distance = self.target_distance.clamp(0.1, 30.0);

        if self.smoothing == 0.0 {
            self.distance = self.target_distance;
        }
    }

    /// Zoom by scroll amount
    pub fn zoom_by_scroll(&mut self, scroll_y: f64) {
        let factor = 1.0 - scroll_y * 0.01;
        self.zoom(factor);
    }

    /// Reset camera to default view
    pub fn reset(&mut self) {
        self.target_distance = 1.5;
        self.target_yaw = 0.5;
        self.target_pitch = 0.3;
        self.target_pan_x = 0.0;
        self.target_pan_y = 0.0;
        self.target = glam::DVec3::new(0.0, 0.15, 0.0);

        if self.smoothing == 0.0 {
            self.distance = self.target_distance;
            self.yaw = self.target_yaw;
            self.pitch = self.target_pitch;
            self.pan_x = self.target_pan_x;
            self.pan_y = self.target_pan_y;
        }
    }

    /// Update camera for smooth animation (call each frame)
    pub fn update(&mut self) {
        if self.smoothing > 0.0 {
            let t = 1.0 - self.smoothing;
            self.distance += (self.target_distance - self.distance) * t;
            self.yaw += (self.target_yaw - self.yaw) * t;
            self.pitch += (self.target_pitch - self.pitch) * t;
            self.pan_x += (self.target_pan_x - self.pan_x) * t;
            self.pan_y += (self.target_pan_y - self.pan_y) * t;
        }
    }

    /// Get camera position in world space
    pub fn position(&self) -> glam::DVec3 {
        let x = self.distance * self.pitch.cos() * self.yaw.sin();
        let y = self.distance * self.pitch.sin();
        let z = self.distance * self.pitch.cos() * self.yaw.cos();
        self.target + glam::DVec3::new(x, y, z)
    }

    /// Compute the view matrix (world to camera space)
    pub fn view_matrix(&self) -> glam::Mat4 {
        let pos = self.position();
        glam::Mat4::look_at_rh(
            pos.as_vec3(),
            self.target.as_vec3(),
            glam::Vec3::Y,
        )
    }

    /// Compute the perspective projection matrix
    pub fn projection_matrix(&self, aspect_ratio: f64) -> glam::Mat4 {
        glam::Mat4::perspective_rh(
            self.fov as f32,
            aspect_ratio as f32,
            self.near as f32,
            self.far as f32,
        )
    }

    /// Get combined view-projection matrix
    pub fn view_projection(&self, aspect_ratio: f64) -> glam::Mat4 {
        self.projection_matrix(aspect_ratio) * self.view_matrix()
    }

    /// Build orbital transform matrix for URDF-style rendering
    /// This combines: pan * scale * orbital_rotation * base_rotation
    pub fn orbital_transform(&self) -> glam::Mat4 {
        let cam_yaw = self.yaw as f32;
        let cam_pitch = self.pitch as f32;
        let cam_scale = 1.0 / self.distance as f32;

        // Z-up to Y-up conversion
        let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        // Orbital rotation
        let orbital_rot = glam::Mat4::from_euler(glam::EulerRot::YXZ, cam_yaw, cam_pitch, 0.0);
        // Scale based on distance
        let scale_mat = glam::Mat4::from_scale(glam::Vec3::splat(cam_scale));
        // Pan offset
        let pan_mat = glam::Mat4::from_translation(glam::Vec3::new(
            self.pan_x as f32,
            self.pan_y as f32,
            0.0,
        ));

        pan_mat * scale_mat * orbital_rot * base_rot
    }

    /// Get camera position as Vec3 for shader uniforms
    pub fn position_vec3(&self) -> Vec3 {
        let pos = self.position();
        vec3(pos.x as f32, pos.y as f32, pos.z as f32)
    }

    /// Convert glam Mat4 to Makepad Mat4
    pub fn glam_to_makepad(m: glam::Mat4) -> Mat4 {
        Mat4 { v: m.to_cols_array() }
    }

    /// Extract frustum planes for culling
    pub fn frustum(&self, aspect_ratio: f64) -> Frustum {
        let vp = self.view_projection(aspect_ratio);
        Frustum::from_view_projection(&vp)
    }
}

/// A view frustum for culling tests
///
/// The frustum is defined by 6 planes (left, right, bottom, top, near, far).
/// Each plane is stored as a Vec4 where (x, y, z) is the normal and w is the distance.
#[derive(Clone, Debug)]
pub struct Frustum {
    /// Frustum planes: left, right, bottom, top, near, far
    pub planes: [glam::Vec4; 6],
}

impl Frustum {
    /// Extract frustum planes from a view-projection matrix
    pub fn from_view_projection(vp: &glam::Mat4) -> Self {
        let m = vp.to_cols_array_2d();

        // Extract planes using Gribb/Hartmann method
        let planes = [
            // Left plane
            glam::Vec4::new(
                m[0][3] + m[0][0],
                m[1][3] + m[1][0],
                m[2][3] + m[2][0],
                m[3][3] + m[3][0],
            ),
            // Right plane
            glam::Vec4::new(
                m[0][3] - m[0][0],
                m[1][3] - m[1][0],
                m[2][3] - m[2][0],
                m[3][3] - m[3][0],
            ),
            // Bottom plane
            glam::Vec4::new(
                m[0][3] + m[0][1],
                m[1][3] + m[1][1],
                m[2][3] + m[2][1],
                m[3][3] + m[3][1],
            ),
            // Top plane
            glam::Vec4::new(
                m[0][3] - m[0][1],
                m[1][3] - m[1][1],
                m[2][3] - m[2][1],
                m[3][3] - m[3][1],
            ),
            // Near plane
            glam::Vec4::new(
                m[0][3] + m[0][2],
                m[1][3] + m[1][2],
                m[2][3] + m[2][2],
                m[3][3] + m[3][2],
            ),
            // Far plane
            glam::Vec4::new(
                m[0][3] - m[0][2],
                m[1][3] - m[1][2],
                m[2][3] - m[2][2],
                m[3][3] - m[3][2],
            ),
        ];

        // Normalize all planes
        let planes = planes.map(|p| {
            let len = glam::Vec3::new(p.x, p.y, p.z).length();
            if len > 0.0 {
                p / len
            } else {
                p
            }
        });

        Self { planes }
    }

    /// Test if a point is inside the frustum
    pub fn contains_point(&self, point: glam::Vec3) -> bool {
        for plane in &self.planes {
            let dist = plane.x * point.x + plane.y * point.y + plane.z * point.z + plane.w;
            if dist < 0.0 {
                return false;
            }
        }
        true
    }

    /// Test if an axis-aligned bounding box intersects the frustum
    ///
    /// Returns true if any part of the AABB is inside the frustum.
    pub fn intersects_aabb(&self, min: glam::Vec3, max: glam::Vec3) -> bool {
        for plane in &self.planes {
            // Find the positive vertex (furthest in direction of plane normal)
            let p = glam::Vec3::new(
                if plane.x >= 0.0 { max.x } else { min.x },
                if plane.y >= 0.0 { max.y } else { min.y },
                if plane.z >= 0.0 { max.z } else { min.z },
            );

            // If positive vertex is outside, the whole box is outside
            let dist = plane.x * p.x + plane.y * p.y + plane.z * p.z + plane.w;
            if dist < 0.0 {
                return false;
            }
        }
        true
    }

    /// Test if a sphere intersects the frustum
    pub fn intersects_sphere(&self, center: glam::Vec3, radius: f32) -> bool {
        for plane in &self.planes {
            let dist = plane.x * center.x + plane.y * center.y + plane.z * center.z + plane.w;
            if dist < -radius {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_camera() {
        let cam = Camera3D::default();
        assert_eq!(cam.distance, 12.0);
        assert_eq!(cam.fov, std::f64::consts::FRAC_PI_4);
    }

    #[test]
    fn test_orbital_camera() {
        let cam = Camera3D::from_orbital(5.0, 0.0, 0.0);
        assert_eq!(cam.distance, 5.0);
    }

    #[test]
    fn test_pitch_clamping() {
        let mut cam = Camera3D::default();
        cam.orbit(0.0, 100.0); // Try to pitch way up
        assert!(cam.pitch < std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn test_zoom() {
        let mut cam = Camera3D::default();
        let initial = cam.distance;
        cam.zoom(0.5);
        assert!(cam.distance < initial);
    }

    #[test]
    fn test_frustum_contains_origin() {
        let cam = Camera3D::from_orbital(5.0, 0.0, 0.0);
        let frustum = cam.frustum(1.0);
        // Origin should be visible from default camera
        assert!(frustum.contains_point(glam::Vec3::ZERO));
    }

    #[test]
    fn test_frustum_point_behind_camera() {
        let cam = Camera3D::from_orbital(5.0, 0.0, 0.0);
        let frustum = cam.frustum(1.0);
        // Point behind the camera (at z=10, camera looking from z=5)
        assert!(!frustum.contains_point(glam::Vec3::new(0.0, 0.0, 10.0)));
    }

    #[test]
    fn test_frustum_aabb_at_origin() {
        let cam = Camera3D::from_orbital(5.0, 0.0, 0.0);
        let frustum = cam.frustum(1.0);
        // A box around the origin should be visible
        let min = glam::Vec3::new(-1.0, -1.0, -1.0);
        let max = glam::Vec3::new(1.0, 1.0, 1.0);
        assert!(frustum.intersects_aabb(min, max));
    }

    #[test]
    fn test_frustum_sphere_at_origin() {
        let cam = Camera3D::from_orbital(5.0, 0.0, 0.0);
        let frustum = cam.frustum(1.0);
        // A sphere at the origin should be visible
        assert!(frustum.intersects_sphere(glam::Vec3::ZERO, 1.0));
    }
}
