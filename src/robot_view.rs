//! Embeddable robot viewer widget (makepad dev script system).
//!
//! Renders the loaded URDF robot into an offscreen 3D pass (color+depth
//! textures, XrCamera orbit) and composites it into the UI — the same
//! viewport pattern as makepad's examples/cad and examples/box3d.
//!
//! Controls: drag = orbit, wheel = zoom, ←/→ = select joint,
//! ↑/↓ = move joint, A = toggle animation, R = reset pose.

use makepad_widgets::*;
use makepad_xr::scene::*;

use crate::render::draw::{DrawGridPlane, DrawRobotMesh, DrawSceneComposite};
use crate::robot::{load_any, load_robot, ForwardKinematics, Robot};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.math.*
    use mod.draw

    mod.widgets.RobotViewBase = #(RobotView::register_widget(vm))
    mod.widgets.RobotView = set_type_default() do mod.widgets.RobotViewBase{
        width: Fill
        height: Fill
        // model to show; leave empty and call `load_robot` from Rust instead
        urdf: ""
        assets: ""
        // environment
        show_grid: true
        sky_horizon: #xFFFBFB
        sky_zenith: #xFAE5E7
        ground_color: #xFFFFC5
        grid_color: #x6B6A3D
        draw_bg: mod.draw.DrawSceneComposite{}
        draw_mesh: mod.draw.DrawRobotMesh{}
        draw_grid: mod.draw.DrawGridPlane{}
        camera: mod.widgets.XrCamera{
            fov_y: 45.0
            desktop_target: vec3(0.0, 0.15, 0.0)
            distance: 1.1
            distance_min: 0.02
            distance_max: 50.0
            wheel_zoom_step: 0.08
        }
    }
}

fn set_pass_camera(cx: &mut Cx, pass: &DrawPass, scene: &SceneState3D, pan: Vec2f) {
    // screen-space pan: translate the view output; orbit pivot is untouched
    let mut view = scene.view;
    view.v[12] += pan.x;
    // vertical pan crosses the wasm projection flip below — keep the
    // on-screen direction identical on both platforms
    if cfg!(target_arch = "wasm32") {
        view.v[13] += pan.y;
    } else {
        view.v[13] -= pan.y;
    }
    let camera_inv = view.invert();
    // WebGL samples the offscreen pass texture vertically flipped (Metal
    // does not) — negate the projection Y row on wasm so the scene reads
    // upright; the flip is about the screen centre, so framing and the
    // orbit pivot are unaffected
    let mut projection = scene.projection;
    if cfg!(target_arch = "wasm32") {
        projection.v[1] = -projection.v[1];
        projection.v[5] = -projection.v[5];
        projection.v[9] = -projection.v[9];
        projection.v[13] = -projection.v[13];
    }
    let pass_uniforms = &mut cx.passes[pass.draw_pass_id()].pass_uniforms;
    pass_uniforms.camera_projection = projection;
    pass_uniforms.camera_projection_r = projection;
    pass_uniforms.camera_view = view;
    pass_uniforms.camera_view_r = view;
    pass_uniforms.depth_projection = projection;
    pass_uniforms.depth_projection_r = projection;
    pass_uniforms.depth_view = view;
    pass_uniforms.depth_view_r = view;
    pass_uniforms.camera_inv = camera_inv;
    pass_uniforms.camera_inv_r = camera_inv;
}

/// Per-instance transforms (FK link matrices) copy directly — glam and the
/// shader's instance path agree on layout.
fn m4(m: glam::Mat4) -> Mat4f {
    Mat4f { v: m.to_cols_array() }
}

/// The scene world transform (draw_list.view_transform) is consumed with the
/// TRANSPOSED convention relative to instance transforms — verified
/// empirically: a direct copy renders the Z-up->Y-up rotation inverted
/// (model below its own orbit target = the "orbit rail" bug), while
/// instance translations are correct. Transpose only this one.
fn m4_world(m: glam::Mat4) -> Mat4f {
    Mat4f { v: m.transpose().to_cols_array() }
}

/// URDF robots are Z-up; the scene camera is Y-up.
fn z_up_to_y_up() -> glam::Mat4 {
    glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2)
}

/// Default orbit angles. The pitch is POSITIVE (camera tilted up) so the
/// horizon sits low in the frame and the ground stays under ~30% of the view.
const DEFAULT_YAW: f32 = 0.72;
const DEFAULT_PITCH: f32 = 0.16;

/// What the widget reports back to its host. Read these with
/// `robot_view.loaded(actions)` / `.load_failed(actions)` on a `WidgetRef`,
/// or match on the action directly.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum RobotViewAction {
    #[default]
    None,
    /// A model finished loading.
    Loaded { links: usize, movable_joints: usize },
    /// A model failed to load; the scene is unchanged.
    LoadFailed { path: String, error: String },
}

const DEFAULT_LINK_COLOR: [f32; 4] = [0.62, 0.65, 0.70, 1.0];
const SELECTED_TINT: [f32; 3] = [1.0, 0.75, 0.25];

#[derive(Script, ScriptHook, WidgetRef, WidgetRegister)]
pub struct RobotView {
    #[uid]
    uid: WidgetUid,
    #[source]
    source: ScriptObjectRef,
    #[walk]
    walk: Walk,
    #[layout]
    layout: Layout,
    #[live(true)]
    visible: bool,
    /// URDF to load on first draw. Empty = start with no model.
    #[live]
    urdf: String,
    /// Directory (or "embedded") the URDF's meshes resolve against.
    #[live]
    assets: String,
    #[live(true)]
    show_grid: bool,
    #[live(vec4(1.0, 0.985, 0.985, 1.0))]
    sky_horizon: Vec4f,
    #[live(vec4(0.98, 0.895, 0.905, 1.0))]
    sky_zenith: Vec4f,
    #[live(vec4(1.0, 1.0, 0.773, 1.0))]
    ground_color: Vec4f,
    #[live(vec4(0.42, 0.41, 0.24, 1.0))]
    grid_color: Vec4f,
    #[live]
    draw_bg: DrawSceneComposite,
    #[live]
    draw_mesh: DrawRobotMesh,
    #[live]
    draw_grid: DrawGridPlane,
    #[live(vec4(0.051, 0.067, 0.091, 1.0))]
    clear_color: Vec4f,
    #[live]
    camera: XrCamera,
    #[new]
    pass: DrawPass,
    #[new]
    draw_list: DrawList,
    #[new]
    color_texture: Texture,
    #[new]
    depth_texture: Texture,
    #[rust]
    area: Area,
    #[rust(false)]
    initialized: bool,
    #[rust]
    robot: Option<Robot>,
    /// per link: index into `geometry_pool` (identical links share a slot)
    #[rust]
    geometries: Vec<Option<usize>>,
    #[rust]
    geometry_pool: Vec<Geometry>,
    #[rust]
    grid_geometry: Option<Geometry>,
    #[rust(0.05f32)]
    grid_spacing: f32,
    #[rust(2.0f32)]
    grid_extent: f32,
    #[rust(0.0f32)]
    grid_y: f32,
    #[rust(false)]
    geometries_dirty: bool,
    #[rust]
    movable: Vec<usize>,
    #[rust(0usize)]
    selected: usize,
    #[rust(false)]
    animating: bool,
    #[rust(0.0f32)]
    anim_t: f32,
    #[rust]
    next_frame: NextFrame,
    #[rust(0usize)]
    dbg_frames: usize,
    #[rust]
    pan_last_abs: Option<DVec2>,
    #[rust(vec2(0.0, 0.0))]
    pan_offset: Vec2f,
    #[rust]
    view_rect: Rect,
    // --- draggable lamp (world azimuth / elevation of the sun) ---
    #[rust(0.85f32)]
    light_yaw: f32,
    #[rust(0.80f32)]
    light_pitch: f32,
    #[rust(false)]
    light_on: bool,
    #[rust]
    light_last_abs: Option<DVec2>,
    // camera basis of the last frame, used to drag the sun in screen space
    #[rust([1.0f32, 0.0, 0.0])]
    cam_right: [f32; 3],
    #[rust([0.0f32, 1.0, 0.0])]
    cam_up: [f32; 3],
    #[rust([0.0f32, 0.0, -1.0])]
    cam_fwd: [f32; 3],
    #[rust(0.41f32)]
    cam_tan_x: f32,
    #[rust(0.41f32)]
    cam_tan_y: f32,
    /// world size of one screen pixel per unit of distance — the grid shader
    /// needs it to keep its lines a pixel wide all the way to the horizon
    #[rust(0.001f32)]
    px_scale: f32,
    /// outcome of the last load, emitted as a widget action on the next event
    #[rust]
    last_action: RobotViewAction,
}

impl RobotView {
    fn robot_links_dbg(&self, robot: &Robot) -> usize {
        robot.links.len()
    }

    fn ensure_initialized(&mut self, cx: &mut Cx) {
        if self.initialized {
            return;
        }
        self.initialized = true;
        self.camera.orbit_yaw = DEFAULT_YAW;
        self.camera.orbit_pitch = DEFAULT_PITCH;
        self.color_texture = Texture::new_with_format(
            cx,
            TextureFormat::RenderBGRAu8 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        self.depth_texture = Texture::new_with_format(
            cx,
            TextureFormat::DepthD32 {
                size: TextureSize::Auto,
                initial: true,
            },
        );
        self.pass.set_color_texture(
            cx,
            &self.color_texture,
            DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 0.0)),
        );
        self.pass
            .set_depth_texture(cx, &self.depth_texture, DrawPassClearDepth::ClearWith(1.0));
        cx.passes[self.pass.draw_pass_id()].keep_camera_matrix = true;

        // No built-in model: a library widget shows whatever the host asked
        // for, either via the `urdf`/`assets` properties or `load_robot`.
        if self.robot.is_none() && !self.urdf.is_empty() {
            let (urdf, assets) = (self.urdf.clone(), self.assets.clone());
            // the outcome is published as a widget action on the next event
            let _ = self.load_robot(&urdf, &assets);
        }
    }

    /// Load (or reload) a robot from a URDF path. Geometry uploads happen
    /// lazily on the next draw. Returns the outcome so a host that calls this
    /// directly can react without waiting for the widget action.
    pub fn load_robot(&mut self, urdf_path: &str, assets_dir: &str) -> Result<(), String> {
        log!("RobotView: loading {}", urdf_path);
        self.last_action = match load_robot(urdf_path, assets_dir) {
            Ok(mut robot) => {
                ForwardKinematics::update(&mut robot);
                log!("RobotView: {} links, {} movable joints",
                     self.robot_links_dbg(&robot),
                     robot.joints.iter().filter(|j| j.is_movable()).count());
                self.adopt_robot(robot)
            }
            Err(e) => {
                error!("RobotView: FAILED to load {}: {}", urdf_path, e);
                RobotViewAction::LoadFailed {
                    path: urdf_path.to_string(),
                    error: e.to_string(),
                }
            }
        };
        match &self.last_action {
            RobotViewAction::LoadFailed { error, .. } => Err(error.clone()),
            _ => Ok(()),
        }
    }

    /// Open a `.urdf`, `.stl` or `.obj` by path. Assets resolve against the
    /// file's own folder, and a bare mesh becomes a single-link model — so a
    /// host can hand this whatever the user picked out of a folder.
    pub fn open_path(&mut self, path: &str) -> Result<(), String> {
        log!("RobotView: opening {}", path);
        self.last_action = match load_any(path) {
            Ok(mut robot) => {
                ForwardKinematics::update(&mut robot);
                self.adopt_robot(robot)
            }
            Err(e) => {
                error!("RobotView: FAILED to open {}: {}", path, e);
                RobotViewAction::LoadFailed {
                    path: path.to_string(),
                    error: e.to_string(),
                }
            }
        };
        match &self.last_action {
            RobotViewAction::LoadFailed { error, .. } => Err(error.clone()),
            _ => Ok(()),
        }
    }

    /// Shared tail of the load paths: adopt a robot and refresh derived state.
    fn adopt_robot(&mut self, robot: Robot) -> RobotViewAction {
        self.movable = robot
            .joints
            .iter()
            .enumerate()
            .filter(|(_, j)| j.is_movable())
            .map(|(i, _)| i)
            .collect();
        self.selected = 0;
        self.animating = false;
        self.anim_t = 0.0;
        self.frame_camera_on(&robot);
        let links = robot.links.len();
        let joints = self.movable.len();
        self.robot = Some(robot);
        self.geometries.clear();
        self.geometries_dirty = true;
        RobotViewAction::Loaded { links, movable_joints: joints }
    }

    /// Remove the current model, leaving an empty scene.
    pub fn clear_robot(&mut self, cx: &mut Cx) {
        self.robot = None;
        self.movable.clear();
        self.geometries.clear();
        self.geometry_pool.clear();
        self.selected = 0;
        self.animating = false;
        self.area.redraw(cx);
    }

    /// The loaded model, for hosts that want to inspect links/joints.
    pub fn robot(&self) -> Option<&Robot> {
        self.robot.as_ref()
    }

    /// Indices into the URDF joint list that this widget treats as movable.
    pub fn movable_joints(&self) -> &[usize] {
        &self.movable
    }

    /// Current angles of the movable joints, in the same order as
    /// `set_joint_angles` expects them.
    pub fn joint_angles(&self) -> Vec<f32> {
        let Some(robot) = &self.robot else { return Vec::new() };
        self.movable.iter().map(|&i| robot.joints[i].angle).collect()
    }

    /// Re-frame the camera on the current model and reset the orbit.
    pub fn reset_view(&mut self, cx: &mut Cx) {
        self.reset_pose(cx);
    }

    /// Direction the key light points FROM, as (azimuth, elevation) radians.
    pub fn light_angles(&self) -> (f32, f32) {
        (self.light_yaw, self.light_pitch)
    }

    /// Aim the key light. Elevation is clamped to the usable range.
    pub fn set_light_angles(&mut self, cx: &mut Cx, yaw: f32, pitch: f32) {
        self.light_yaw = yaw;
        self.light_pitch = pitch.clamp(-1.55, 1.55);
        self.area.redraw(cx);
    }

    /// Aim the orbit camera (radians). Framing and distance are untouched.
    pub fn set_view_angles(&mut self, cx: &mut Cx, yaw: f32, pitch: f32) {
        self.camera.orbit_yaw = yaw;
        self.camera.orbit_pitch = pitch.clamp(-1.55, 1.55);
        self.pan_offset = vec2(0.0, 0.0);
        self.area.redraw(cx);
    }

    pub fn is_animating(&self) -> bool {
        self.animating
    }

    pub fn set_animating(&mut self, cx: &mut Cx, on: bool) {
        self.animating = on;
        if on {
            self.next_frame = cx.new_next_frame();
        }
        self.area.redraw(cx);
    }

    /// Drive all movable joints from a state vector (e.g. dataset playback).
    /// Values map to movable joints in URDF order; extra values are ignored.
    pub fn set_joint_angles(&mut self, cx: &mut Cx, angles: &[f32]) {
        let Some(robot) = &mut self.robot else { return };
        for (k, &ji) in self.movable.iter().enumerate() {
            let Some(&angle) = angles.get(k) else { break };
            // recorded data is replayed as recorded — clamping to the URDF
            // limits here would silently rewrite the pose
            robot.set_joint_angle_unclamped(ji, angle);
        }
        ForwardKinematics::update(robot);
        self.area.redraw(cx);
    }

    fn ensure_geometries(&mut self, cx: &mut Cx) {
        if !self.geometries_dirty {
            return;
        }
        self.geometries_dirty = false;
        self.geometries.clear();
        self.geometry_pool.clear();
        let Some(robot) = &self.robot else { return };
        // The array repeats two meshes 32 times each; uploading one buffer per
        // link cost ~55 MB of GPU vertex data for 3 distinct shapes. Links are
        // mapped to a shared pool keyed by mesh content, so only the instance
        // transform differs. (Geometry is not Clone — hence indices, not
        // handles, in self.geometries.)
        let mut by_hash: std::collections::HashMap<u64, usize> = Default::default();
        for link in &robot.links {
            let Some(mesh) = &link.mesh_data else {
                self.geometries.push(None);
                continue;
            };
            let key = Self::mesh_key(mesh);
            if let Some(&slot) = by_hash.get(&key) {
                self.geometries.push(Some(slot));
                continue;
            }
            // MeshData: interleaved pos(3), id(1), normal(3), uv(2)
            // IcoVertex geometry: pos.xyzw, normal.xyzw
            let n_verts = mesh.vertices.len() / 9;
            let mut vertices = Vec::with_capacity(n_verts * 8);
            for i in 0..n_verts {
                let v = &mesh.vertices[i * 9..i * 9 + 9];
                vertices.extend_from_slice(&[v[0], v[1], v[2], 1.0, v[4], v[5], v[6], 0.0]);
            }
            let geometry = Geometry::new(cx);
            geometry.update(cx, mesh.indices.clone(), vertices);
            let slot = self.geometry_pool.len();
            self.geometry_pool.push(geometry);
            by_hash.insert(key, slot);
            self.geometries.push(Some(slot));
        }
    }

    /// Content hash of a link mesh, used to share one GPU buffer between
    /// identical links. Hashes the raw float bits, so it only collapses
    /// meshes that are byte-for-byte the same shape.
    fn mesh_key(mesh: &crate::mesh::MeshData) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        mesh.vertices.len().hash(&mut hasher);
        mesh.indices.hash(&mut hasher);
        for v in &mesh.vertices {
            v.to_bits().hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Frame the camera on a robot (Z-up URDF -> Y-up world: y' = z).
    fn frame_camera_on(&mut self, robot: &Robot) {
        // world-space: link-local boxes would frame translated models wrong
        let (bmin, bmax) = robot.bounds_world();
        let center = (bmin + bmax) * 0.5;
        let radius = ((bmax - bmin).length() * 0.5).max(0.05);
        let distance = (radius * 3.1).clamp(0.15, 40.0);
        // The camera sits at target - forward * distance, and forward tilts UP
        // by DEFAULT_PITCH, so a low target drops the camera underneath the
        // ground plane (the 4x8 array ended up 0.14 m below it, looking at the
        // plate from beneath). Lift the orbit target just enough to keep the
        // eye a little above the plane; small models are unaffected.
        let plane_y = bmin.z - 0.002;
        let eye_clearance = 0.15 * radius;
        let min_target_y = plane_y + eye_clearance + DEFAULT_PITCH.sin() * distance;
        // orbit pivots on the body centre (raised only if the eye needs it)
        self.camera.desktop_target =
            vec3(center.x, center.z.max(min_target_y), -center.y);
        self.camera.distance = distance;
        self.pan_offset = vec2(0.0, 0.0);
        // grid scaled to the model: pick a round spacing near radius/3
        let steps = [0.01f32, 0.02, 0.05, 0.1, 0.2, 0.5, 1.0];
        let want = radius / 3.0;
        self.grid_spacing = steps
            .iter()
            .copied()
            .min_by(|a, b| {
                (a - want).abs().partial_cmp(&(b - want).abs()).unwrap()
            })
            .unwrap();
        // the plane runs out to the horizon: 130 m keeps its corners
        // (130 * sqrt(2) = 184) inside the camera's 200 m far plane, while its
        // own edge sits ~0.03 deg below eye level — i.e. on the horizon line
        self.grid_extent = 130.0;
        // sit the grid just below the model's lowest point (world y = urdf z)
        self.grid_y = plane_y;
    }

    fn ensure_grid_geometry(&mut self, cx: &mut Cx) -> GeometryId {
        let geometry = self.grid_geometry.get_or_insert_with(|| {
            let geometry = Geometry::new(cx);
            let mut vertices: Vec<f32> = Vec::with_capacity(4 * 8);
            for (x, z) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                vertices.extend_from_slice(&[x, 0.0, z, 1.0, 0.0, 1.0, 0.0, 0.0]);
            }
            let indices: Vec<u32> = vec![0, 1, 2, 0, 2, 3];
            geometry.update(cx, indices, vertices);
            geometry
        });
        geometry.geometry_id()
    }

    /// Unit vector towards the lamp, world (Y-up) space.
    fn light_vec(&self) -> [f32; 3] {
        let cp = self.light_pitch.cos();
        [
            cp * self.light_yaw.sin(),
            self.light_pitch.sin(),
            cp * self.light_yaw.cos(),
        ]
    }

    pub fn is_light_on(&self) -> bool {
        self.light_on
    }

    /// Switch the lamp on/off. Moving it is always alt+drag, so orbiting and
    /// the joint keys keep working while the lamp is on.
    /// Switching on parks the sun in the upper right of the current view so
    /// there is something visible to grab.
    pub fn set_light_on(&mut self, cx: &mut Cx, on: bool) {
        self.light_on = on;
        self.light_last_abs = None;
        if on {
            let sx = 0.45 * self.cam_tan_x;
            let sy = 0.55 * self.cam_tan_y;
            let v = [
                self.cam_fwd[0] + self.cam_right[0] * sx + self.cam_up[0] * sy,
                self.cam_fwd[1] + self.cam_right[1] * sx + self.cam_up[1] * sy,
                self.cam_fwd[2] + self.cam_right[2] * sx + self.cam_up[2] * sy,
            ];
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
            let n = [v[0] / len, v[1] / len, v[2] / len];
            self.light_yaw = n[0].atan2(n[2]);
            self.light_pitch = n[1].clamp(-1.0, 1.0).asin();
        }
        self.area.redraw(cx);
    }

    /// Point the lamp along the camera ray through an absolute screen point,
    /// so alt+click drops the sun exactly where the cursor is.
    fn aim_light_at(&mut self, abs: DVec2) {
        if self.view_rect.size.x <= 1.0 || self.view_rect.size.y <= 1.0 {
            return;
        }
        let ndc_x = ((abs.x - self.view_rect.pos.x) / self.view_rect.size.x * 2.0 - 1.0) as f32;
        let ndc_y = (1.0 - (abs.y - self.view_rect.pos.y) / self.view_rect.size.y * 2.0) as f32;
        let sx = ndc_x * self.cam_tan_x;
        let sy = ndc_y * self.cam_tan_y;
        let v = [
            self.cam_fwd[0] + self.cam_right[0] * sx + self.cam_up[0] * sy,
            self.cam_fwd[1] + self.cam_right[1] * sx + self.cam_up[1] * sy,
            self.cam_fwd[2] + self.cam_right[2] * sx + self.cam_up[2] * sy,
        ];
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        let n = [v[0] / len, v[1] / len, v[2] / len];
        self.light_yaw = n[0].atan2(n[2]);
        self.light_pitch = n[1].clamp(-1.0, 1.0).asin();
    }

    /// Drag the sun: while it is in front of the camera it follows the
    /// cursor exactly (its screen position is moved by the drag); once it
    /// swings behind, fall back to rotating it in world space.
    fn drag_light(&mut self, delta: DVec2) {
        let d = self.light_vec();
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let f = dot(d, self.cam_fwd);
        let usable_rect = self.view_rect.size.x > 1.0 && self.view_rect.size.y > 1.0;
        if f > 0.25 && usable_rect {
            let ndc_x = dot(d, self.cam_right) / (f * self.cam_tan_x);
            let ndc_y = dot(d, self.cam_up) / (f * self.cam_tan_y);
            let nx = ndc_x + 2.0 * delta.x as f32 / self.view_rect.size.x as f32;
            let ny = ndc_y - 2.0 * delta.y as f32 / self.view_rect.size.y as f32;
            let sx = nx * self.cam_tan_x;
            let sy = ny * self.cam_tan_y;
            let v = [
                self.cam_fwd[0] + self.cam_right[0] * sx + self.cam_up[0] * sy,
                self.cam_fwd[1] + self.cam_right[1] * sx + self.cam_up[1] * sy,
                self.cam_fwd[2] + self.cam_right[2] * sx + self.cam_up[2] * sy,
            ];
            let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
            let n = [v[0] / len, v[1] / len, v[2] / len];
            self.light_yaw = n[0].atan2(n[2]);
            self.light_pitch = n[1].clamp(-1.0, 1.0).asin();
        } else {
            // behind the camera: keep turning it in world space so the lamp
            // can be walked all the way around the sphere
            self.light_yaw -= delta.x as f32 * 0.006;
            self.light_pitch -= delta.y as f32 * 0.006;
        }
        // full sphere: any azimuth, and elevation from straight down to
        // straight up (short of the poles, where the azimuth degenerates)
        self.light_pitch = self.light_pitch.clamp(-1.55, 1.55);
        if self.light_yaw > std::f32::consts::PI {
            self.light_yaw -= 2.0 * std::f32::consts::PI;
        } else if self.light_yaw < -std::f32::consts::PI {
            self.light_yaw += 2.0 * std::f32::consts::PI;
        }
    }

    fn selected_joint_index(&self) -> Option<usize> {
        self.movable.get(self.selected).copied()
    }

    /// Link highlighted by the current joint selection (the joint's child).
    fn selected_link_index(&self) -> Option<usize> {
        let robot = self.robot.as_ref()?;
        let joint = robot.joints.get(self.selected_joint_index()?)?;
        robot.get_link_index(&joint.child_link)
    }

    fn adjust_selected_joint(&mut self, cx: &mut Cx, delta: f32) {
        let Some(ji) = self.selected_joint_index() else { return };
        if let Some(robot) = &mut self.robot {
            let angle = robot.joints[ji].angle + delta;
            robot.set_joint_angle(ji, angle);
            ForwardKinematics::update(robot);
            self.area.redraw(cx);
        }
    }

    fn step_animation(&mut self) {
        let Some(robot) = &mut self.robot else { return };
        self.anim_t += 1.0 / 60.0;
        for (k, &ji) in self.movable.iter().enumerate() {
            let j = &robot.joints[ji];
            let phase = self.anim_t * 1.4 + k as f32 * 0.7;
            let angle = if j.limit_upper > j.limit_lower
                && j.limit_upper.is_finite()
                && j.limit_lower.is_finite()
            {
                let center = 0.5 * (j.limit_upper + j.limit_lower);
                let amp = (0.5 * (j.limit_upper - j.limit_lower)).min(0.8);
                center + amp * phase.sin()
            } else {
                phase
            };
            robot.set_joint_angle(ji, angle);
        }
        ForwardKinematics::update(robot);
    }

    fn reset_pose(&mut self, cx: &mut Cx) {
        if let Some(mut robot) = self.robot.take() {
            robot.reset_joints();
            ForwardKinematics::update(&mut robot);
            self.frame_camera_on(&robot);
            self.robot = Some(robot);
        }
        self.animating = false;
        self.camera.orbit_yaw = DEFAULT_YAW;
        self.camera.orbit_pitch = DEFAULT_PITCH;
        self.pan_offset = vec2(0.0, 0.0);
        self.area.redraw(cx);
    }

    fn draw_scene(&mut self, cx: &mut Cx3d, scene_state: SceneState3D) {
        self.draw_list.begin_always(cx);
        cx.begin_scene_3d(scene_state);
        let previous_world = cx.set_scene_world_transform_3d(m4(glam::Mat4::IDENTITY));
        let world = z_up_to_y_up();
        let mut dbg_drawn = 0usize;
        let mut dbg_can = 0usize;

        let l = self.light_vec();
        self.draw_mesh.key_light = vec4(l[0], l[1], l[2], if self.light_on { 1.0 } else { 0.0 });

        let selected_link = self.selected_link_index();
        if let Some(robot) = &self.robot {
            for (i, link) in robot.links.iter().enumerate() {
                let Some(Some(slot)) = self.geometries.get(i) else { continue };
                let Some(geometry) = self.geometry_pool.get(*slot) else { continue };
                let Some(transform) = robot.get_link_transform(i) else { continue };
                let base = link.color.unwrap_or(DEFAULT_LINK_COLOR);
                let color = if Some(i) == selected_link {
                    [
                        base[0] * 0.45 + SELECTED_TINT[0] * 0.55,
                        base[1] * 0.45 + SELECTED_TINT[1] * 0.55,
                        base[2] * 0.45 + SELECTED_TINT[2] * 0.55,
                        base[3],
                    ]
                } else {
                    base
                };
                self.draw_mesh.transform = m4(world * transform);
                self.draw_mesh.color = vec4(color[0], color[1], color[2], color[3]);
                self.draw_mesh.scale = vec3(1.0, 1.0, 1.0);
                self.draw_mesh.depth_clip = 0.0;
                let geometry_id = geometry.geometry_id();
                if self.draw_mesh.draw_vars.can_instance() { dbg_can += 1; }
                self.draw_mesh.draw(cx, geometry_id);
                dbg_drawn += 1;
            }
        }
        if self.dbg_frames < 3 {
            self.dbg_frames += 1;
            log!("RVDBG dist={} target=({},{},{}) yaw={} pitch={} pan=({},{})",
                 self.camera.distance,
                 self.camera.desktop_target.x, self.camera.desktop_target.y,
                 self.camera.desktop_target.z,
                 self.camera.orbit_yaw, self.camera.orbit_pitch,
                 self.pan_offset.x, self.pan_offset.y);
        }

        let grid_geom = self.ensure_grid_geometry(cx.cx);
        self.draw_grid.color = self.grid_color;
        self.draw_grid.soil_color = self.ground_color;
        // the plane must fade into the same colour the sky shows there
        self.draw_grid.haze_color = self.sky_horizon;
        self.draw_grid.extent = self.grid_extent;
        self.draw_grid.spacing = self.grid_spacing;
        self.draw_grid.plane_y = self.grid_y;
        self.draw_grid.px_scale = self.px_scale;
        self.draw_grid.depth_clip = 0.0;
        if self.show_grid {
            self.draw_grid.draw(cx, grid_geom);
        }

        if let Some(previous_world) = previous_world {
            let _ = cx.set_scene_world_transform_3d(previous_world);
        }
        cx.end_scene_3d();
        self.draw_list.end(cx);
    }
}

/// Host-facing handle. Grab one with `ui.robot_view(cx, ids!(viewport))`
/// (or `WidgetRef::as_robot_view`) and drive the widget through it — these
/// wrap the borrow for you.
impl RobotViewRef {
    /// Load a model. `assets_dir` is a directory, or `"embedded"` to resolve
    /// mesh names through [`crate::robot::set_virtual_assets`].
    pub fn load_robot(&self, cx: &mut Cx, urdf: &str, assets_dir: &str) -> Result<(), String> {
        let r = match self.borrow_mut() {
            Some(mut inner) => inner.load_robot(urdf, assets_dir),
            None => Err("RobotView not found".to_string()),
        };
        self.redraw(cx);
        r
    }

    /// Open a `.urdf`, `.stl` or `.obj` by path — the "user picked a file"
    /// entry point. Assets resolve against the file's own folder.
    pub fn open_path(&self, cx: &mut Cx, path: &str) -> Result<(), String> {
        let r = match self.borrow_mut() {
            Some(mut inner) => inner.open_path(path),
            None => Err("RobotView not found".to_string()),
        };
        self.redraw(cx);
        r
    }

    /// Drop the current model.
    pub fn clear_robot(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear_robot(cx);
        }
    }

    /// `Some((links, movable_joints))` on the frame a model finished loading.
    pub fn loaded(&self, actions: &Actions) -> Option<(usize, usize)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let RobotViewAction::Loaded { links, movable_joints } = item.cast() {
                return Some((links, movable_joints));
            }
        }
        None
    }

    /// `Some((path, error))` on the frame a load failed.
    pub fn load_failed(&self, actions: &Actions) -> Option<(String, String)> {
        if let Some(item) = actions.find_widget_action(self.widget_uid()) {
            if let RobotViewAction::LoadFailed { path, error } = item.cast() {
                return Some((path, error));
            }
        }
        None
    }

    /// Pose the movable joints, in URDF order. Values are applied unclamped
    /// so recorded data renders exactly as recorded.
    pub fn set_joint_angles(&self, cx: &mut Cx, angles: &[f32]) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_joint_angles(cx, angles);
        }
    }

    /// Current movable-joint angles, matching `set_joint_angles` order.
    pub fn joint_angles(&self) -> Vec<f32> {
        self.borrow()
            .map(|inner| inner.joint_angles())
            .unwrap_or_default()
    }

    /// Number of movable joints in the loaded model.
    pub fn movable_joint_count(&self) -> usize {
        self.borrow()
            .map(|inner| inner.movable_joints().len())
            .unwrap_or(0)
    }

    /// Re-frame the camera on the model and reset the orbit.
    pub fn reset_view(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.reset_view(cx);
        }
    }

    pub fn is_light_on(&self) -> bool {
        self.borrow()
            .map(|inner| inner.is_light_on())
            .unwrap_or(false)
    }

    pub fn set_light_on(&self, cx: &mut Cx, on: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_light_on(cx, on);
        }
    }

    /// Aim the key light: azimuth and elevation in radians.
    pub fn set_light_angles(&self, cx: &mut Cx, yaw: f32, pitch: f32) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_light_angles(cx, yaw, pitch);
        }
    }

    /// Point the camera without changing what it is framed on — for view
    /// presets (iso / front / side / top).
    pub fn set_view_angles(&self, cx: &mut Cx, yaw: f32, pitch: f32) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_view_angles(cx, yaw, pitch);
        }
    }

    pub fn is_grid_visible(&self) -> bool {
        self.borrow().map(|i| i.show_grid).unwrap_or(false)
    }

    pub fn set_grid_visible(&self, cx: &mut Cx, on: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.show_grid = on;
            inner.area.redraw(cx);
        }
    }

    pub fn is_animating(&self) -> bool {
        self.borrow()
            .map(|inner| inner.is_animating())
            .unwrap_or(false)
    }

    pub fn set_animating(&self, cx: &mut Cx, on: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_animating(cx, on);
        }
    }
}

impl WidgetNode for RobotView {
    fn widget_uid(&self) -> WidgetUid {
        self.uid
    }

    fn walk(&mut self, _cx: &mut Cx) -> Walk {
        self.walk
    }

    fn area(&self) -> Area {
        self.area
    }

    fn redraw(&mut self, cx: &mut Cx) {
        self.area.redraw(cx);
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, cx: &mut Cx, visible: bool) {
        self.visible = visible;
        self.area.redraw(cx);
    }
}

impl Widget for RobotView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, _scope: &mut Scope) {
        // a load can happen outside the event loop (DSL property, host call),
        // so publish its outcome on the next event the widget sees
        if self.last_action != RobotViewAction::None {
            let action = std::mem::take(&mut self.last_action);
            cx.widget_action(self.uid, action);
        }
        // pan: shift+drag, right-drag or middle-drag (XrCamera only orbits/zooms)
        // A late script re-apply (web: fires when resources finish loading)
        // resets #[rust] state — view_rect, camera viewport — after the last
        // draw. Self-heal: any event in that state forces a redraw, which
        // repopulates them; until then accept pointer input everywhere.
        if self.view_rect.size.x <= 1.0 {
            // state can be reset behind our back (web script re-apply);
            // self.area resets too, so redraw globally to repopulate
            cx.redraw_all();
        }
        let in_view = |abs: DVec2, rect: &Rect| rect.size.x <= 1.0 || rect.contains(abs);
        match event {
            // alt+drag moves the lamp — its own modifier, so plain drag keeps
            // orbiting and the joint keys keep working while the lamp is lit
            Event::MouseDown(fe)
                if in_view(fe.abs, &self.view_rect)
                    && fe.button.is_primary()
                    && fe.modifiers.alt =>
            {
                self.light_on = true;
                // put the sun under the cursor, then let the drag carry it
                self.aim_light_at(fe.abs);
                self.light_last_abs = Some(fe.abs);
                self.area.redraw(cx);
                return;
            }
            Event::MouseDown(fe)
                if in_view(fe.abs, &self.view_rect)
                    && ((fe.button.is_primary() && fe.modifiers.shift)
                        || fe.button.is_secondary()
                        || fe.button.is_middle()) =>
            {
                self.pan_last_abs = Some(fe.abs);
                return;
            }
            Event::MouseMove(fe) => {
                if let Some(last) = self.light_last_abs {
                    let delta = fe.abs - last;
                    self.light_last_abs = Some(fe.abs);
                    self.drag_light(delta);
                    self.area.redraw(cx);
                    return;
                }
                if let Some(last) = self.pan_last_abs {
                    let delta = fe.abs - last;
                    self.pan_last_abs = Some(fe.abs);
                    if self.view_rect.size.y <= 1.0 {
                        return; // rect not repopulated yet — no scale reference
                    }
                    let fov = (self.camera.fov_y as f32).to_radians() * 0.5;
                    let per_px = 2.0 * self.camera.distance * fov.tan()
                        / (self.view_rect.size.y as f32);
                    self.pan_offset.x += delta.x as f32 * per_px;
                    self.pan_offset.y -= delta.y as f32 * per_px;
                    self.area.redraw(cx);
                    return;
                }
            }
            Event::MouseUp(_) => {
                self.pan_last_abs = None;
                self.light_last_abs = None;
            }
            Event::Scroll(fs)
                if self.view_rect.contains(fs.abs)
                    && fs.scroll.x.abs() > fs.scroll.y.abs() =>
            {
                // trackpad horizontal two-finger drag orbits instead of zooming
                self.camera.orbit_yaw -= fs.scroll.x as f32 * 0.005;
                self.area.redraw(cx);
                return;
            }
            _ => (),
        }
        self.camera.handle_desktop_interaction(cx, event);

        if self.next_frame.is_event(event).is_some() {
            if self.animating {
                self.step_animation();
                self.next_frame = cx.new_next_frame();
                self.area.redraw(cx);
            }
        }

        if let Event::KeyDown(ke) = event {
            match ke.key_code {
                KeyCode::ArrowLeft => {
                    if !self.movable.is_empty() {
                        self.selected = (self.selected + self.movable.len() - 1) % self.movable.len();
                        self.area.redraw(cx);
                    }
                }
                KeyCode::ArrowRight => {
                    if !self.movable.is_empty() {
                        self.selected = (self.selected + 1) % self.movable.len();
                        self.area.redraw(cx);
                    }
                }
                KeyCode::ArrowUp => self.adjust_selected_joint(cx, 0.06),
                KeyCode::ArrowDown => self.adjust_selected_joint(cx, -0.06),
                KeyCode::KeyA => {
                    self.animating = !self.animating;
                    if self.animating {
                        self.next_frame = cx.new_next_frame();
                    }
                    self.area.redraw(cx);
                }
                KeyCode::KeyR => self.reset_pose(cx),
                _ => (),
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.visible {
            return DrawStep::done();
        }
        let rect = cx.walk_turtle_with_area(&mut self.area, walk);
        if rect.size.x <= 1.0 || rect.size.y <= 1.0 {
            return DrawStep::done();
        }
        self.view_rect = rect;

        self.ensure_initialized(cx.cx);
        self.ensure_geometries(cx.cx);
        self.camera.set_desktop_viewport_rect(rect);
        self.pass.set_size(cx, rect.size);
        self.pass.set_color_texture(
            cx,
            &self.color_texture,
            DrawPassClearColor::ClearWith(vec4(0.0, 0.0, 0.0, 0.0)),
        );
        self.pass
            .set_depth_texture(cx, &self.depth_texture, DrawPassClearDepth::ClearWith(1.0));

        cx.make_child_pass(&self.pass);
        cx.begin_pass(&self.pass, None);
        if let Some(scene_state) = self.camera.desktop_scene_state(rect, cx.time()) {
            set_pass_camera(cx.cx, &self.pass, &scene_state, self.pan_offset);
            let cx3d = &mut Cx3d::new(cx.cx);
            self.draw_scene(cx3d, scene_state);
        }
        cx.end_pass(&self.pass);

        self.draw_bg.set_scene_texture(&self.color_texture);
        // camera basis for the directional sky dome in the composite shader
        if let Some(scene_state) = self.camera.desktop_scene_state(rect, cx.time()) {
            let inv = scene_state.view.invert();
            let tan_y = ((self.camera.fov_y as f32).to_radians() * 0.5).tan();
            let tan_x = tan_y * (rect.size.x / rect.size.y) as f32;
            self.draw_bg.cam_right = vec4(inv.v[0], inv.v[1], inv.v[2], tan_x);
            self.draw_bg.cam_up = vec4(inv.v[4], inv.v[5], inv.v[6], tan_y);
            self.draw_bg.cam_fwd = vec4(-inv.v[8], -inv.v[9], -inv.v[10], 0.0);
            // same basis for dragging the sun around in screen space
            self.cam_right = [inv.v[0], inv.v[1], inv.v[2]];
            self.cam_up = [inv.v[4], inv.v[5], inv.v[6]];
            self.cam_fwd = [-inv.v[8], -inv.v[9], -inv.v[10]];
            self.cam_tan_x = tan_x;
            self.cam_tan_y = tan_y;
            self.px_scale = 2.0 * tan_y / (rect.size.y as f32).max(1.0);
        }
        let l = self.light_vec();
        self.draw_bg.light_dir = vec4(l[0], l[1], l[2], if self.light_on { 1.0 } else { 0.0 });
        self.draw_bg.sky_horizon = self.sky_horizon;
        self.draw_bg.sky_zenith = self.sky_zenith;
        self.draw_bg.ground_color = self.ground_color;
        self.draw_bg.draw_abs(cx, rect);
        self.area = self.draw_bg.area();
        cx.set_pass_area(&self.pass, self.area);
        DrawStep::done()
    }
}
