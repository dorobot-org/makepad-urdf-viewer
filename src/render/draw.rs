//! Draw shaders for 3D rendering
//!
//! Provides DrawMesh, DrawGrid, and DrawSkybox for rendering various 3D elements.

use makepad_widgets::*;
use super::geometry::GeometryMesh3D;
use super::mesh::MeshData;

live_design! {
    use link::shaders::*;
    use crate::render::geometry::GeometryMesh3D;

    pub DrawGrid = {{DrawGrid}} {
        geometry: <GeometryMesh3D> {}

        varying line_color: vec4;
        varying world_pos: vec3;
        varying screen_pos: vec2;

        fn vertex(self) -> vec4 {
            let col0 = self.transform_col0;
            let col1 = self.transform_col1;
            let col2 = self.transform_col2;
            let col3 = self.transform_col3;
            let pos_in = self.geom_pos;
            let pos = vec3(
                col0.x * pos_in.x + col1.x * pos_in.y + col2.x * pos_in.z + col3.x,
                col0.y * pos_in.x + col1.y * pos_in.y + col2.y * pos_in.z + col3.y,
                col0.z * pos_in.x + col1.z * pos_in.y + col2.z * pos_in.z + col3.z
            );

            self.world_pos = pos;

            let scale = 4.0;
            let px = pos.x * scale;
            let py = pos.y * scale;

            let raw_depth = (pos.z + 2.0) * 0.2;
            let depth = clamp(raw_depth, 0.1, 0.9);

            self.line_color = self.color;

            let win_w = max(self.window_size.x, 1.0);
            let win_h = max(self.window_size.y, 1.0);
            let screen_x_pixels = (px + 1.0) / 2.0 * win_w;
            let screen_y_pixels = (1.0 - py) / 2.0 * win_h;
            self.screen_pos = vec2(screen_x_pixels, screen_y_pixels);

            return vec4(px, py, depth, 1.0);
        }

        fn pixel(self) -> vec4 {
            // Viewport clipping
            if self.screen_pos.x < self.draw_clip.x || self.screen_pos.x > self.draw_clip.z ||
               self.screen_pos.y < self.draw_clip.y || self.screen_pos.y > self.draw_clip.w {
                return vec4(0.165, 0.165, 0.208, 1.0);  // Header background color
            }

            let bg_color = vec4(0.7, 0.9, 0.7, 1.0);

            let x_norm = self.world_pos.x / self.grid_spacing + 0.5;
            let y_norm = self.world_pos.y / self.grid_spacing + 0.5;
            let x_frac = x_norm - floor(x_norm);
            let y_frac = y_norm - floor(y_norm);
            let x_dist = abs(x_frac - 0.5) * self.grid_spacing;
            let y_dist = abs(y_frac - 0.5) * self.grid_spacing;

            // X axis (red)
            if abs(self.world_pos.y) < self.line_width * 5.0 {
                return self.x_axis_color;
            }

            // Y axis (blue)
            if abs(self.world_pos.x) < self.line_width * 5.0 {
                return self.z_axis_color;
            }

            // Regular grid lines
            if x_dist < self.line_width || y_dist < self.line_width {
                return self.line_color;
            }

            return bg_color;
        }

        fn fragment(self) -> vec4 {
            return self.pixel();
        }
    }

    pub DrawMesh = {{DrawMesh}} {
        geometry: <GeometryMesh3D> {}

        varying lit_color: vec4;
        varying world_pos: vec3;
        varying world_normal: vec3;
        varying uv: vec2;
        varying screen_pos: vec2;

        fn vertex(self) -> vec4 {
            let col0 = self.transform_col0;
            let col1 = self.transform_col1;
            let col2 = self.transform_col2;
            let col3 = self.transform_col3;

            let pos_in = self.geom_pos;
            let pos = vec3(
                col0.x * pos_in.x + col1.x * pos_in.y + col2.x * pos_in.z + col3.x,
                col0.y * pos_in.x + col1.y * pos_in.y + col2.y * pos_in.z + col3.y,
                col0.z * pos_in.x + col1.z * pos_in.y + col2.z * pos_in.z + col3.z
            );

            let normal_in = self.geom_normal;
            let normal = vec3(
                col0.x * normal_in.x + col1.x * normal_in.y + col2.x * normal_in.z,
                col0.y * normal_in.x + col1.y * normal_in.y + col2.y * normal_in.z,
                col0.z * normal_in.x + col1.z * normal_in.y + col2.z * normal_in.z
            );

            self.world_pos = pos;
            self.world_normal = normalize(normal);
            self.uv = self.geom_uv;

            // Diffuse lighting
            let light_dir = normalize(vec3(0.3, 0.8, 0.5));
            let n = normalize(normal);
            let diff = max(0.0, dot(n, light_dir));
            let ambient = 0.4;
            let diffuse_brightness = ambient + diff * 0.6;

            let bottom_blend = max(0.0, -n.y);
            let base_color = mix(self.color.xyz, self.bottom_color.xyz, bottom_blend);

            self.lit_color = vec4(base_color * diffuse_brightness, 1.0);

            let scale = 4.0;
            let px = pos.x * scale;
            let py = pos.y * scale;

            let raw_depth = (pos.z + 2.0) * 0.2;
            let depth = clamp(raw_depth, 0.1, 0.9);

            let win_w = max(self.window_size.x, 1.0);
            let win_h = max(self.window_size.y, 1.0);
            let screen_x_pixels = (px + 1.0) / 2.0 * win_w;
            let screen_y_pixels = (1.0 - py) / 2.0 * win_h;
            self.screen_pos = vec2(screen_x_pixels, screen_y_pixels);

            return vec4(px, py, depth, 1.0);
        }

        fn pixel(self) -> vec4 {
            // Viewport clipping
            if self.screen_pos.x < self.draw_clip.x || self.screen_pos.x > self.draw_clip.z ||
               self.screen_pos.y < self.draw_clip.y || self.screen_pos.y > self.draw_clip.w {
                return vec4(0.165, 0.165, 0.208, 1.0);  // Header background color
            }

            // Grid line rendering mode
            if self.draw_grid_lines > 0.5 {
                let grid_cells = 10.0;
                let gx = self.uv.x * grid_cells;
                let gy = self.uv.y * grid_cells;

                let dx = abs(gx - floor(gx + 0.5));
                let dy = abs(gy - floor(gy + 0.5));

                if dx < 0.001 || dy < 0.001 {
                    return vec4(0.65, 0.65, 0.65, 1.0);
                }
                return vec4(0.0, 0.0, 0.0, 0.0);
            }

            // Specular lighting
            let light_dir = normalize(vec3(0.3, 0.8, 0.5));
            let view_dir = normalize(self.camera_pos - self.world_pos);
            let normal = normalize(self.world_normal);

            // Blinn-Phong specular
            let halfway = normalize(light_dir + view_dir);
            let spec_angle = max(dot(normal, halfway), 0.0);
            let specular = pow(spec_angle, self.shininess) * self.specular_strength;

            let final_color = self.lit_color.xyz + vec3(specular, specular, specular);

            return vec4(final_color, 1.0);
        }

        fn fragment(self) -> vec4 {
            return self.pixel();
        }
    }

    pub DrawSkybox = {{DrawSkybox}} {}
}

/// Draw shader for grid rendering
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawGrid {
    #[rust] pub many_instances: Option<ManyInstances>,
    #[live] pub geometry: GeometryMesh3D,
    #[deref] pub draw_vars: DrawVars,
    #[live] pub color: Vec4,
    #[live] pub x_axis_color: Vec4,
    #[live] pub z_axis_color: Vec4,
    #[live(0.05)] pub grid_spacing: f32,
    #[live(0.002)] pub line_width: f32,
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub transform_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub transform_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub transform_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub transform_col3: Vec4,
    #[live] pub draw_clip: Vec4,
    #[live] pub window_size: Vec2,
}

impl LiveHook for DrawGrid {
    fn before_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.before_apply_init_shader(cx, apply, index, nodes, &self.geometry);
    }

    fn after_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.after_apply_update_self(cx, apply, index, nodes, &self.geometry);
    }
}

impl DrawGrid {
    pub fn create_ground_plane(&mut self, cx: &mut Cx, size: f32, y_pos: f32) {
        let mesh = MeshData::ground_plane(size, y_pos);
        self.geometry.upload_mesh_data(cx, mesh);
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry,
        );
    }

    pub fn update_transformed_geometry(&mut self, cx: &mut Cx, original_mesh: &MeshData, transform: &Mat4) {
        let mut transformed = original_mesh.clone();
        transformed.apply_transform(transform);
        self.geometry.upload_mesh_data(cx, transformed);
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry,
        );
    }

    pub fn draw(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = &mut self.many_instances {
            mi.instances.extend_from_slice(self.draw_vars.as_slice());
        } else if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }

    pub fn begin_many_instances(&mut self, cx: &mut Cx2d) {
        self.many_instances = cx.begin_many_instances(&self.draw_vars);
    }

    pub fn end_many_instances(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = self.many_instances.take() {
            let new_area = cx.end_many_instances(mi);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }

    pub fn set_transform(&mut self, transform: &[f32; 16]) {
        self.transform_col0 = vec4(transform[0], transform[1], transform[2], transform[3]);
        self.transform_col1 = vec4(transform[4], transform[5], transform[6], transform[7]);
        self.transform_col2 = vec4(transform[8], transform[9], transform[10], transform[11]);
        self.transform_col3 = vec4(transform[12], transform[13], transform[14], transform[15]);
    }

    pub fn set_draw_clip(&mut self, clip: Vec4) {
        self.draw_clip = clip;
    }

    pub fn set_window_size(&mut self, size: Vec2) {
        self.window_size = size;
    }
}

/// Draw shader for mesh rendering
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawMesh {
    #[rust] pub many_instances: Option<ManyInstances>,
    #[live] pub geometry: GeometryMesh3D,
    #[deref] pub draw_vars: DrawVars,
    #[live] pub color: Vec4,
    #[live] pub bottom_color: Vec4,
    #[live(vec3(0.0, 0.0, 0.0))] pub mesh_pos: Vec3,
    #[live(vec3(1.0, 1.0, 1.0))] pub mesh_scale: Vec3,
    #[live(1.0)] pub depth_clip: f32,
    #[live(0.0)] pub draw_grid_lines: f32,
    // Model transform matrix
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub transform_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub transform_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub transform_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub transform_col3: Vec4,
    // View matrix
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub view_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub view_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub view_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub view_col3: Vec4,
    // Projection matrix
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub proj_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub proj_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub proj_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub proj_col3: Vec4,
    // Lighting
    #[live(vec3(0.0, 0.5, 3.0))] pub camera_pos: Vec3,
    #[live(0.5)] pub specular_strength: f32,
    #[live(32.0)] pub shininess: f32,
    #[live(1.4)] pub aspect_ratio: f32,
    #[live] pub draw_clip: Vec4,
    #[live] pub window_size: Vec2,
}

impl LiveHook for DrawMesh {
    fn before_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.before_apply_init_shader(cx, apply, index, nodes, &self.geometry);
    }

    fn after_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.after_apply_update_self(cx, apply, index, nodes, &self.geometry);
    }
}

impl DrawMesh {
    /// Create a new DrawMesh for a robot link with separate geometry
    pub fn new_for_link(_cx: &mut Cx, mesh_data: MeshData, template: &DrawMesh) -> Self {
        let mut geom = GeometryMesh3D::new_empty();
        geom.mesh_data = Some(mesh_data);

        let draw_vars = template.draw_vars.clone();

        DrawMesh {
            many_instances: None,
            geometry: geom,
            draw_vars,
            color: vec4(1.0, 0.65, 0.1, 1.0),
            bottom_color: vec4(0.2, 0.2, 0.25, 1.0),
            mesh_pos: vec3(0.0, 0.0, 0.0),
            mesh_scale: vec3(1.0, 1.0, 1.0),
            depth_clip: 1.0,
            draw_grid_lines: 0.0,
            transform_col0: vec4(1.0, 0.0, 0.0, 0.0),
            transform_col1: vec4(0.0, 1.0, 0.0, 0.0),
            transform_col2: vec4(0.0, 0.0, 1.0, 0.0),
            transform_col3: vec4(0.0, 0.0, 0.0, 1.0),
            view_col0: vec4(1.0, 0.0, 0.0, 0.0),
            view_col1: vec4(0.0, 1.0, 0.0, 0.0),
            view_col2: vec4(0.0, 0.0, 1.0, 0.0),
            view_col3: vec4(0.0, 0.0, 0.0, 1.0),
            proj_col0: vec4(1.0, 0.0, 0.0, 0.0),
            proj_col1: vec4(0.0, 1.0, 0.0, 0.0),
            proj_col2: vec4(0.0, 0.0, 1.0, 0.0),
            proj_col3: vec4(0.0, 0.0, 0.0, 1.0),
            camera_pos: vec3(0.0, 0.5, 3.0),
            specular_strength: 0.5,
            shininess: 32.0,
            aspect_ratio: 1.4,
            draw_clip: vec4(0.0, 0.0, 10000.0, 10000.0),
            window_size: vec2(1024.0, 768.0),
        }
    }

    pub fn set_transform(&mut self, m: &Mat4) {
        self.transform_col0 = vec4(m.v[0], m.v[1], m.v[2], m.v[3]);
        self.transform_col1 = vec4(m.v[4], m.v[5], m.v[6], m.v[7]);
        self.transform_col2 = vec4(m.v[8], m.v[9], m.v[10], m.v[11]);
        self.transform_col3 = vec4(m.v[12], m.v[13], m.v[14], m.v[15]);
    }

    pub fn set_view_matrix(&mut self, m: &Mat4) {
        self.view_col0 = vec4(m.v[0], m.v[1], m.v[2], m.v[3]);
        self.view_col1 = vec4(m.v[4], m.v[5], m.v[6], m.v[7]);
        self.view_col2 = vec4(m.v[8], m.v[9], m.v[10], m.v[11]);
        self.view_col3 = vec4(m.v[12], m.v[13], m.v[14], m.v[15]);
    }

    pub fn set_projection_matrix(&mut self, m: &Mat4) {
        self.proj_col0 = vec4(m.v[0], m.v[1], m.v[2], m.v[3]);
        self.proj_col1 = vec4(m.v[4], m.v[5], m.v[6], m.v[7]);
        self.proj_col2 = vec4(m.v[8], m.v[9], m.v[10], m.v[11]);
        self.proj_col3 = vec4(m.v[12], m.v[13], m.v[14], m.v[15]);
    }

    pub fn set_camera_position(&mut self, pos: Vec3) {
        self.camera_pos = pos;
    }

    pub fn set_specular_strength(&mut self, strength: f32) {
        self.specular_strength = strength;
    }

    pub fn set_draw_clip(&mut self, clip: Vec4) {
        self.draw_clip = clip;
    }

    pub fn set_window_size(&mut self, size: Vec2) {
        self.window_size = size;
    }

    pub fn reset_transform(&mut self) {
        self.transform_col0 = vec4(1.0, 0.0, 0.0, 0.0);
        self.transform_col1 = vec4(0.0, 1.0, 0.0, 0.0);
        self.transform_col2 = vec4(0.0, 0.0, 1.0, 0.0);
        self.transform_col3 = vec4(0.0, 0.0, 0.0, 1.0);
    }

    pub fn update_transformed_geometry(&mut self, cx: &mut Cx, original_mesh: &MeshData, transform: &Mat4) {
        let mut transformed = original_mesh.clone();
        transformed.apply_transform(transform);
        self.geometry.upload_mesh_data(cx, transformed);
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry,
        );
    }

    pub fn init_link_geometry(&mut self, cx: &mut Cx) {
        if let Some(mesh_data) = self.geometry.mesh_data.take() {
            self.geometry.upload_mesh_data(cx, mesh_data);
        }
        self.draw_vars.after_apply_update_self(
            cx,
            &mut Apply::from(ApplyFrom::UpdateFromDoc { file_id: Default::default() }),
            0,
            &[],
            &self.geometry,
        );
    }

    pub fn draw(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = &mut self.many_instances {
            mi.instances.extend_from_slice(self.draw_vars.as_slice());
        } else if self.draw_vars.can_instance() {
            let new_area = cx.add_instance(&self.draw_vars);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }

    pub fn new_draw_call(&self, cx: &mut Cx2d) {
        cx.new_draw_call(&self.draw_vars);
    }

    pub fn begin_many_instances(&mut self, cx: &mut Cx2d) {
        self.many_instances = cx.begin_many_instances(&self.draw_vars);
    }

    pub fn end_many_instances(&mut self, cx: &mut Cx2d) {
        if let Some(mi) = self.many_instances.take() {
            let new_area = cx.end_many_instances(mi);
            self.draw_vars.area = cx.update_area_refs(self.draw_vars.area, new_area);
        }
    }
}

/// Draw shader for sky gradient background
#[derive(Live, LiveRegister)]
#[repr(C)]
pub struct DrawSkybox {
    #[deref] pub draw_super: DrawQuad,
    #[live(0.0)] pub camera_pitch: f32,
}

impl LiveHook for DrawSkybox {
    fn after_new_from_doc(&mut self, cx: &mut Cx) {
        self.draw_super.after_new_from_doc(cx);
    }
}

impl DrawSkybox {
    pub fn draw_abs(&mut self, cx: &mut Cx2d, rect: Rect) {
        self.draw_super.draw_abs(cx, rect);
    }
}
