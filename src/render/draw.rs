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
            // Reconstruct matrices
            let m_col0 = self.transform_col0;
            let m_col1 = self.transform_col1;
            let m_col2 = self.transform_col2;
            let m_col3 = self.transform_col3;
            let model = mat4(
                vec4(m_col0.x, m_col0.y, m_col0.z, m_col0.w),
                vec4(m_col1.x, m_col1.y, m_col1.z, m_col1.w),
                vec4(m_col2.x, m_col2.y, m_col2.z, m_col2.w),
                vec4(m_col3.x, m_col3.y, m_col3.z, m_col3.w)
            );

            let v_col0 = self.view_col0;
            let v_col1 = self.view_col1;
            let v_col2 = self.view_col2;
            let v_col3 = self.view_col3;
            let view = mat4(
                vec4(v_col0.x, v_col0.y, v_col0.z, v_col0.w),
                vec4(v_col1.x, v_col1.y, v_col1.z, v_col1.w),
                vec4(v_col2.x, v_col2.y, v_col2.z, v_col2.w),
                vec4(v_col3.x, v_col3.y, v_col3.z, v_col3.w)
            );

            let p_col0 = self.proj_col0;
            let p_col1 = self.proj_col1;
            let p_col2 = self.proj_col2;
            let p_col3 = self.proj_col3;
            let proj = mat4(
                vec4(p_col0.x, p_col0.y, p_col0.z, p_col0.w),
                vec4(p_col1.x, p_col1.y, p_col1.z, p_col1.w),
                vec4(p_col2.x, p_col2.y, p_col2.z, p_col2.w),
                vec4(p_col3.x, p_col3.y, p_col3.z, p_col3.w)
            );

            let pos_in = vec4(self.geom_pos, 1.0);
            let world_pos_4 = model * pos_in;
            self.world_pos = world_pos_4.xyz;
            
            let view_pos = view * world_pos_4;
            let clip_pos = proj * view_pos;

            self.line_color = self.color;

            // Calculate screen position for scissor clipping
            let ndc = clip_pos.xyz / clip_pos.w;
            let vp_x = self.viewport_rect.x;
            let vp_y = self.viewport_rect.y;
            let vp_w = self.viewport_rect.z;
            let vp_h = self.viewport_rect.w;
            
            let pixel_x = vp_x + (ndc.x + 1.0) * 0.5 * vp_w;
            let pixel_y = vp_y + (1.0 - ndc.y) * 0.5 * vp_h;
            self.screen_pos = vec2(pixel_x, pixel_y);

            // Apply Viewport Correction to clip_pos
            let win_w = max(self.full_size.x, 1.0);
            let win_h = max(self.full_size.y, 1.0);
            
            let center_x_px = vp_x + vp_w * 0.5;
            let center_y_px = vp_y + vp_h * 0.5;
            
            let offset_x = (center_x_px / win_w) * 2.0 - 1.0;
            let offset_y = 1.0 - (center_y_px / win_h) * 2.0;
            
            let scale_x = vp_w / win_w;
            let scale_y = vp_h / win_h;
            
            clip_pos.x = clip_pos.x * scale_x + clip_pos.w * offset_x;
            clip_pos.y = clip_pos.y * scale_y + clip_pos.w * offset_y;

            return clip_pos;
        }

        fn pixel(self) -> vec4 {
            // Viewport clipping - discard pixels outside viewport
            if self.screen_pos.x < self.draw_clip.x || self.screen_pos.x > self.draw_clip.z ||
               self.screen_pos.y < self.draw_clip.y || self.screen_pos.y > self.draw_clip.w {
                return vec4(0.0, 0.0, 0.0, 0.0);  // Transparent - discard
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
            // Reconstruct matrices
            let m_col0 = self.transform_col0;
            let m_col1 = self.transform_col1;
            let m_col2 = self.transform_col2;
            let m_col3 = self.transform_col3;
            let model = mat4(
                vec4(m_col0.x, m_col0.y, m_col0.z, m_col0.w),
                vec4(m_col1.x, m_col1.y, m_col1.z, m_col1.w),
                vec4(m_col2.x, m_col2.y, m_col2.z, m_col2.w),
                vec4(m_col3.x, m_col3.y, m_col3.z, m_col3.w)
            );

            let v_col0 = self.view_col0;
            let v_col1 = self.view_col1;
            let v_col2 = self.view_col2;
            let v_col3 = self.view_col3;
            let view = mat4(
                vec4(v_col0.x, v_col0.y, v_col0.z, v_col0.w),
                vec4(v_col1.x, v_col1.y, v_col1.z, v_col1.w),
                vec4(v_col2.x, v_col2.y, v_col2.z, v_col2.w),
                vec4(v_col3.x, v_col3.y, v_col3.z, v_col3.w)
            );

            let p_col0 = self.proj_col0;
            let p_col1 = self.proj_col1;
            let p_col2 = self.proj_col2;
            let p_col3 = self.proj_col3;
            let proj = mat4(
                vec4(p_col0.x, p_col0.y, p_col0.z, p_col0.w),
                vec4(p_col1.x, p_col1.y, p_col1.z, p_col1.w),
                vec4(p_col2.x, p_col2.y, p_col2.z, p_col2.w),
                vec4(p_col3.x, p_col3.y, p_col3.z, p_col3.w)
            );

            // Transform vertex
            let pos_in = vec4(self.geom_pos, 1.0);
            let world_pos_4 = model * pos_in;
            self.world_pos = world_pos_4.xyz;

            let view_pos = view * world_pos_4;
            let clip_pos = proj * view_pos;

            // Transform normal
            let normal_in = vec4(self.geom_normal, 0.0);
            let world_normal = model * normal_in;
            self.world_normal = normalize(world_normal.xyz);
            
            self.uv = self.geom_uv;

            // Lighting
            let light_dir = normalize(vec3(0.3, 0.8, 0.5));
            let n = self.world_normal;
            let diff = max(0.0, dot(n, light_dir));
            let ambient = 0.4;
            let diffuse_brightness = ambient + diff * 0.6;

            let bottom_blend = max(0.0, -n.y);
            let base_color = mix(self.color.xyz, self.bottom_color.xyz, bottom_blend);

            self.lit_color = vec4(base_color * diffuse_brightness, 1.0);

            // Calculate screen position for scissor clipping
            let ndc = clip_pos.xyz / clip_pos.w;
            
            // Viewport transform
            let vp_x = self.viewport_rect.x;
            let vp_y = self.viewport_rect.y;
            let vp_w = self.viewport_rect.z;
            let vp_h = self.viewport_rect.w;
            
            let pixel_x = vp_x + (ndc.x + 1.0) * 0.5 * vp_w;
            let pixel_y = vp_y + (1.0 - ndc.y) * 0.5 * vp_h;
            
            self.screen_pos = vec2(pixel_x, pixel_y);

            // Apply Viewport Correction to clip_pos
            let win_w = max(self.full_size.x, 1.0);
            let win_h = max(self.full_size.y, 1.0);
            
            let center_x_px = vp_x + vp_w * 0.5;
            let center_y_px = vp_y + vp_h * 0.5;
            
            let offset_x = (center_x_px / win_w) * 2.0 - 1.0;
            let offset_y = 1.0 - (center_y_px / win_h) * 2.0;
            
            let scale_x = vp_w / win_w;
            let scale_y = vp_h / win_h;
            
            clip_pos.x = clip_pos.x * scale_x + clip_pos.w * offset_x;
            clip_pos.y = clip_pos.y * scale_y + clip_pos.w * offset_y;

            return clip_pos;
        }

        fn pixel(self) -> vec4 {
            // DISABLED viewport clipping for debugging
            // if self.screen_pos.x < self.draw_clip.x || self.screen_pos.x > self.draw_clip.z ||
            //    self.screen_pos.y < self.draw_clip.y || self.screen_pos.y > self.draw_clip.w {
            //     return vec4(0.0, 0.0, 0.0, 0.0);  // Transparent - discard
            // }

            // Grid line rendering mode - only draw lines, transparent background
            if self.draw_grid_lines > 0.5 {
                let grid_cells = 10.0;
                let gx = self.uv.x * grid_cells;
                let gy = self.uv.y * grid_cells;

                let dx = abs(gx - floor(gx + 0.5));
                let dy = abs(gy - floor(gy + 0.5));

                // Use larger threshold (0.03 = 3% of cell width) for visible lines
                if dx < 0.03 || dy < 0.03 {
                    return vec4(0.4, 0.4, 0.45, 1.0);  // Gray grid lines
                }
                // Transparent background - let window background show through
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

    // 3D skybox cube - renders a gradient based on view direction
    pub DrawSkybox = {{DrawSkybox}} {
        geometry: <GeometryMesh3D> {}

        varying world_pos: vec3;
        varying screen_pos: vec2;

        fn vertex(self) -> vec4 {
            // Reconstruct view matrix (rotation only - we want skybox centered on camera)
            let v_col0 = self.view_col0;
            let v_col1 = self.view_col1;
            let v_col2 = self.view_col2;
            let v_col3 = self.view_col3;

            let p_col0 = self.proj_col0;
            let p_col1 = self.proj_col1;
            let p_col2 = self.proj_col2;
            let p_col3 = self.proj_col3;
            let proj = mat4(
                vec4(p_col0.x, p_col0.y, p_col0.z, p_col0.w),
                vec4(p_col1.x, p_col1.y, p_col1.z, p_col1.w),
                vec4(p_col2.x, p_col2.y, p_col2.z, p_col2.w),
                vec4(p_col3.x, p_col3.y, p_col3.z, p_col3.w)
            );

            // Store world position (direction from center) for gradient
            self.world_pos = self.geom_pos;

            // Transform: use only view rotation (remove translation for skybox)
            // Replace column 3 with (0,0,0,1) to remove translation
            let view_rot = mat4(
                vec4(v_col0.x, v_col0.y, v_col0.z, 0.0),
                vec4(v_col1.x, v_col1.y, v_col1.z, 0.0),
                vec4(v_col2.x, v_col2.y, v_col2.z, 0.0),
                vec4(0.0, 0.0, 0.0, 1.0)
            );

            let pos_in = vec4(self.geom_pos, 1.0);
            let view_pos = view_rot * pos_in;
            let clip_pos = proj * view_pos;

            // Calculate screen position for clipping
            let ndc = clip_pos.xyz / clip_pos.w;
            let vp_x = self.viewport_rect.x;
            let vp_y = self.viewport_rect.y;
            let vp_w = self.viewport_rect.z;
            let vp_h = self.viewport_rect.w;

            let pixel_x = vp_x + (ndc.x + 1.0) * 0.5 * vp_w;
            let pixel_y = vp_y + (1.0 - ndc.y) * 0.5 * vp_h;
            self.screen_pos = vec2(pixel_x, pixel_y);

            // Apply Viewport Correction
            let win_w = max(self.full_size.x, 1.0);
            let win_h = max(self.full_size.y, 1.0);

            let center_x_px = vp_x + vp_w * 0.5;
            let center_y_px = vp_y + vp_h * 0.5;

            let offset_x = (center_x_px / win_w) * 2.0 - 1.0;
            let offset_y = 1.0 - (center_y_px / win_h) * 2.0;

            let scale_x = vp_w / win_w;
            let scale_y = vp_h / win_h;

            clip_pos.x = clip_pos.x * scale_x + clip_pos.w * offset_x;
            clip_pos.y = clip_pos.y * scale_y + clip_pos.w * offset_y;

            // Push skybox to far depth (0.9999 * w so it's behind everything)
            clip_pos.z = clip_pos.w * 0.9999;

            return clip_pos;
        }

        fn pixel(self) -> vec4 {
            // Viewport clipping
            if self.screen_pos.x < self.draw_clip.x || self.screen_pos.x > self.draw_clip.z ||
               self.screen_pos.y < self.draw_clip.y || self.screen_pos.y > self.draw_clip.w {
                return vec4(0.0, 0.0, 0.0, 0.0);
            }

            // Gradient based on view direction (world_pos is the cube vertex position = view direction)
            let dir = normalize(self.world_pos);
            // Map Y from -1..1 to 0..1 for gradient
            let t = 0.5 * (dir.y + 1.0);
            return mix(self.bottom_color, self.top_color, t);
        }

        fn fragment(self) -> vec4 {
            return self.pixel();
        }
    }
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
    // Model transform
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub transform_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub transform_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub transform_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub transform_col3: Vec4,
    // View transform
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub view_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub view_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub view_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub view_col3: Vec4,
    // Projection transform
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub proj_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub proj_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub proj_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub proj_col3: Vec4,
    
    #[live] pub draw_clip: Vec4,
    #[live] pub window_size: Vec2,
    #[live(vec4(0.0, 0.0, 1024.0, 768.0))] pub viewport_rect: Vec4,
    #[live(vec2(1024.0, 768.0))] pub full_size: Vec2,
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
    pub fn new_from_template(_cx: &mut Cx, template: &DrawGrid) -> Self {
        let mut geom = GeometryMesh3D::new_empty();
        // geom.mesh_data is None by default
        
        let draw_vars = template.draw_vars.clone();
        
        DrawGrid {
            many_instances: None,
            geometry: geom,
            draw_vars,
            color: template.color,
            x_axis_color: template.x_axis_color,
            z_axis_color: template.z_axis_color,
            grid_spacing: template.grid_spacing,
            line_width: template.line_width,
            transform_col0: template.transform_col0,
            transform_col1: template.transform_col1,
            transform_col2: template.transform_col2,
            transform_col3: template.transform_col3,
            view_col0: template.view_col0,
            view_col1: template.view_col1,
            view_col2: template.view_col2,
            view_col3: template.view_col3,
            proj_col0: template.proj_col0,
            proj_col1: template.proj_col1,
            proj_col2: template.proj_col2,
            proj_col3: template.proj_col3,
            draw_clip: template.draw_clip,
            window_size: template.window_size,
            viewport_rect: template.viewport_rect,
            full_size: template.full_size,
        }
    }

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

    pub fn set_draw_clip(&mut self, clip: Vec4) {
        self.draw_clip = clip;
    }

    pub fn set_window_size(&mut self, size: Vec2) {
        self.window_size = size;
    }

    pub fn set_viewport_rect(&mut self, rect: Vec4) {
        self.viewport_rect = rect;
    }

    pub fn set_full_size(&mut self, size: Vec2) {
        self.full_size = size;
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
    // Viewport transform for embedded rendering
    #[live(vec4(0.0, 0.0, 1024.0, 768.0))] pub viewport_rect: Vec4,  // x, y, width, height
    #[live(vec2(1024.0, 768.0))] pub full_size: Vec2,  // full window size
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
            viewport_rect: vec4(0.0, 0.0, 1024.0, 768.0),
            full_size: vec2(1024.0, 768.0),
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

    pub fn set_viewport_rect(&mut self, rect: Vec4) {
        self.viewport_rect = rect;
    }

    pub fn set_full_size(&mut self, size: Vec2) {
        self.full_size = size;
    }

    pub fn reset_transform(&mut self) {
        self.transform_col0 = vec4(1.0, 0.0, 0.0, 0.0);
        self.transform_col1 = vec4(0.0, 1.0, 0.0, 0.0);
        self.transform_col2 = vec4(0.0, 0.0, 1.0, 0.0);
        self.transform_col3 = vec4(0.0, 0.0, 0.0, 1.0);
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
    #[rust] pub many_instances: Option<ManyInstances>,
    #[live] pub geometry: GeometryMesh3D,
    #[deref] pub draw_vars: DrawVars,

    #[live(vec4(0.7, 0.8, 0.9, 1.0))] pub top_color: Vec4,
    #[live(vec4(0.9, 0.95, 0.9, 1.0))] pub bottom_color: Vec4,

    // View transform
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub view_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub view_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub view_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub view_col3: Vec4,
    // Projection transform
    #[live(vec4(1.0, 0.0, 0.0, 0.0))] pub proj_col0: Vec4,
    #[live(vec4(0.0, 1.0, 0.0, 0.0))] pub proj_col1: Vec4,
    #[live(vec4(0.0, 0.0, 1.0, 0.0))] pub proj_col2: Vec4,
    #[live(vec4(0.0, 0.0, 0.0, 1.0))] pub proj_col3: Vec4,

    #[live(vec4(0.0, 0.0, 10000.0, 10000.0))] pub draw_clip: Vec4,
    #[live(vec4(0.0, 0.0, 1024.0, 768.0))] pub viewport_rect: Vec4,
    #[live(vec2(1024.0, 768.0))] pub full_size: Vec2,
}

impl LiveHook for DrawSkybox {
    fn before_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.before_apply_init_shader(cx, apply, index, nodes, &self.geometry);
    }

    fn after_apply(&mut self, cx: &mut Cx, apply: &mut Apply, index: usize, nodes: &[LiveNode]) {
        self.draw_vars.after_apply_update_self(cx, apply, index, nodes, &self.geometry);
    }
}

impl DrawSkybox {
    pub fn init_geometry(&mut self, cx: &mut Cx) {
        // Create a skybox cube (faces pointing inward for viewing from inside)
        let mesh = MeshData::skybox_cube(100.0);
        self.geometry.upload_mesh_data(cx, mesh);
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

    pub fn set_draw_clip(&mut self, clip: Vec4) {
        self.draw_clip = clip;
    }

    pub fn set_viewport_rect(&mut self, rect: Vec4) {
        self.viewport_rect = rect;
    }

    pub fn set_full_size(&mut self, size: Vec2) {
        self.full_size = size;
    }
}
