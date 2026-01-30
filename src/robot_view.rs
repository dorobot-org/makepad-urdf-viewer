//! Embeddable Robot Viewer Widget
//!
//! A reusable Makepad widget for displaying and interacting with URDF robots.

use makepad_widgets::*;
use std::sync::mpsc::{channel, Receiver};

use crate::mesh::{DrawMesh, DrawGrid, DrawSkybox, MeshData};
use crate::robot::{Robot, JointType, load_robot, ForwardKinematicsMethods};
use crate::camera::CameraController;
use crate::error::RobotError;
use crate::episode::Episode;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use crate::mesh::DrawMesh;
    use crate::mesh::DrawGrid;
    use crate::mesh::DrawSkybox;

    pub RobotView = {{RobotView}} {
        width: Fill
        height: Fill

        show_bg: false  // Disable 3D skybox cube to avoid ghost geometry
        draw_bg: {
            top_color: #b8c8d8
            bottom_color: #d0e0d0
        }

        draw_mesh: {
            color: #e07020
        }

        draw_grid: {
            color: #408040
            x_axis_color: #ff4040
            z_axis_color: #4040ff
            grid_spacing: 0.1
            line_width: 0.004
        }
    }
}

/// Loading state for the robot viewer
#[derive(Clone, Debug, Default, PartialEq)]
pub enum LoadingState {
    #[default]
    Idle,
    Loading { message: String },
    Loaded,
    Error { message: String },
}

/// Result of async robot loading
type AsyncLoadResult = Result<Robot, RobotError>;

#[derive(Clone, Debug, DefaultNone)]
pub enum RobotViewAction {
    None,
    JointChanged { joint_idx: usize, angle: f32 },
    AnimationToggled(bool),
    LoadingStarted,
    LoadingComplete,
    LoadingError { message: String },
}

#[derive(Live, LiveHook, Widget)]
pub struct RobotView {
    #[redraw] #[live] draw_bg: DrawSkybox,
    #[live] show_bg: bool,
    #[redraw] #[live] draw_mesh: DrawMesh,
    #[redraw] #[live] draw_grid: DrawGrid,
    #[walk] walk: Walk,
    #[layout] layout: Layout,

    #[rust] camera: CameraController,
    #[rust] selected_joint: usize,
    #[rust] animating: bool,
    #[rust] anim_timer: Timer,
    #[rust] anim_step: u64,
    #[rust] initialized: bool,

    #[rust] robot: Option<Robot>,
    #[rust] link_drawers: Vec<DrawMesh>,
    #[rust] original_meshes: Vec<MeshData>,
    #[rust] grid_mesh: Option<MeshData>,
    #[rust] grid_drawer: Option<DrawMesh>,
    #[rust] grid_initialized: bool,

    #[rust] urdf_path: String,
    #[rust] assets_dir: String,
    #[rust] area: Area,
    #[rust] loading_state: LoadingState,
    #[rust] async_receiver: Option<Receiver<AsyncLoadResult>>,
    #[rust] load_poll_timer: Timer,
    #[rust] specular_enabled: bool,
    #[rust] show_joint_axes: bool,
    #[rust] axis_drawers: Vec<DrawMesh>,
    #[rust] axis_mesh: Option<MeshData>,
    #[rust] show_world_axes: bool,
    #[rust] world_axis_drawers: Vec<DrawMesh>,
    #[rust] world_axes_initialized: bool,

    // Episode playback
    #[rust] episode: Option<Episode>,
    #[rust] episode_frame: usize,
    #[rust] episode_playing: bool,
    #[rust] episode_timer: Timer,
}

impl RobotView {
    pub fn animate_step(&mut self, cx: &mut Cx, step: u64) {
        self.anim_step = step;
        if let Some(ref mut robot) = self.robot {
            let speed = if robot.num_joints() > 20 { 0.002 } else { 0.02 };

            for (joint_idx, joint) in robot.joints.iter_mut().enumerate() {
                if joint.joint_type == JointType::Fixed {
                    continue;
                }

                let sin_val = (step as f64 * (speed + joint_idx as f64 / 1000.0)).sin();

                let dynamic_angle = match joint.joint_type {
                    JointType::Continuous => {
                        remap(sin_val, -1.0, 1.0, -std::f64::consts::PI, std::f64::consts::PI) as f32
                    }
                    JointType::Revolute => {
                        remap(sin_val, -1.0, 1.0, joint.limit_lower as f64, joint.limit_upper as f64) as f32
                    }
                    _ => 0.0,
                };
                joint.angle = dynamic_angle;
            }
        }
        self.redraw(cx);
    }

    pub fn load_robot(&mut self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        self.urdf_path = urdf_path.to_string();
        self.assets_dir = assets_dir.to_string();
        self.initialized = false;
        self.redraw(cx);
    }

    pub fn reload_robot(&mut self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        self.robot = None;
        self.link_drawers.clear();
        self.original_meshes.clear();
        self.axis_drawers.clear();
        self.axis_mesh = None;
        self.selected_joint = 0;
        self.animating = false;
        self.anim_timer = Timer::default();
        self.anim_step = 0;
        self.urdf_path = urdf_path.to_string();
        self.assets_dir = assets_dir.to_string();
        self.initialized = false;
        self.redraw(cx);
    }

    pub fn set_joint_angle(&mut self, cx: &mut Cx, idx: usize, angle: f32) {
        if let Some(ref mut robot) = self.robot {
            robot.set_joint_angle(idx, angle);
            self.redraw(cx);
        }
    }

    pub fn get_joint_angles(&self) -> Vec<f32> {
        self.robot.as_ref().map(|r| r.get_joint_angles()).unwrap_or_default()
    }

    pub fn set_joint_angles(&mut self, cx: &mut Cx, angles: &[f32]) {
        if let Some(ref mut robot) = self.robot {
            robot.set_joint_angles(angles);
            self.redraw(cx);
        }
    }

    pub fn toggle_animation(&mut self, cx: &mut Cx) -> bool {
        self.animating = !self.animating;
        if self.animating {
            self.anim_timer = cx.start_interval(0.033);
        } else {
            self.anim_timer = Timer::default();
        }
        self.animating
    }

    pub fn reset_view(&mut self, cx: &mut Cx) {
        self.camera.camera.reset();
        if let Some(ref mut robot) = self.robot {
            robot.reset_joints();
        }
        self.redraw(cx);
    }

    pub fn toggle_specular(&mut self, cx: &mut Cx) -> bool {
        self.specular_enabled = !self.specular_enabled;
        self.redraw(cx);
        self.specular_enabled
    }

    pub fn is_specular_enabled(&self) -> bool {
        self.specular_enabled
    }

    pub fn toggle_joint_axes(&mut self, cx: &mut Cx) -> bool {
        self.show_joint_axes = !self.show_joint_axes;
        self.redraw(cx);
        self.show_joint_axes
    }

    pub fn is_joint_axes_shown(&self) -> bool {
        self.show_joint_axes
    }

    pub fn toggle_world_axes(&mut self, cx: &mut Cx) -> bool {
        self.show_world_axes = !self.show_world_axes;
        self.redraw(cx);
        self.show_world_axes
    }

    pub fn is_world_axes_shown(&self) -> bool {
        self.show_world_axes
    }

    /// Load episode data from a parquet file
    pub fn load_episode(&mut self, cx: &mut Cx, path: &str) {
        match Episode::load(path) {
            Ok(episode) => {
                self.episode = Some(episode);
                self.episode_frame = 0;
                self.episode_playing = true; // Auto-start playback
                self.episode_timer = cx.start_interval(0.033); // ~30fps playback
                // Apply first frame
                self.apply_episode_frame(cx);
            }
            Err(_e) => {
                // Episode loading failed - silently ignore, episode playback won't be available
            }
        }
    }

    /// Toggle episode playback
    pub fn toggle_episode(&mut self, cx: &mut Cx) -> bool {
        if self.episode.is_none() {
            return false;
        }
        self.episode_playing = !self.episode_playing;
        if self.episode_playing {
            self.episode_timer = cx.start_interval(0.033); // ~30fps playback
        } else {
            self.episode_timer = Timer::default();
        }
        self.episode_playing
    }

    /// Get episode info (current_frame, total_frames)
    pub fn get_episode_info(&self) -> Option<(usize, usize)> {
        self.episode.as_ref().map(|ep| (self.episode_frame, ep.num_frames()))
    }

    /// Check if episode is playing
    pub fn is_episode_playing(&self) -> bool {
        self.episode_playing
    }

    /// Apply joint angles from current episode frame
    fn apply_episode_frame(&mut self, cx: &mut Cx) {
        if let Some(ref episode) = self.episode {
            if let Some(frame) = episode.get_frame(self.episode_frame) {
                if let Some(ref mut robot) = self.robot {
                    // Use unclamped setter to apply episode angles without URDF limit restrictions
                    robot.set_joint_angles_unclamped(&frame.joint_angles);
                    self.redraw(cx);
                }
            }
        }
    }

    /// Advance to next episode frame (called by timer)
    pub fn advance_episode_frame(&mut self, cx: &mut Cx) {
        if let Some(ref episode) = self.episode {
            if self.episode_frame < episode.num_frames() - 1 {
                self.episode_frame += 1;
            } else {
                // Loop back to start
                self.episode_frame = 0;
            }
            self.apply_episode_frame(cx);
        }
    }

    /// Enable/disable camera smoothing
    pub fn set_camera_smoothing(&mut self, smoothing: f64) {
        self.camera.camera.smoothing = smoothing.clamp(0.0, 0.99);
    }

    /// Get camera smoothing value
    pub fn get_camera_smoothing(&self) -> f64 {
        self.camera.camera.smoothing
    }

    fn init_robot(&mut self, cx: &mut Cx) {
        if self.urdf_path.is_empty() {
            self.urdf_path = "data/vx300s/vx300s.urdf".to_string();
            self.assets_dir = "data/vx300s".to_string();
        }

        self.loading_state = LoadingState::Loading {
            message: format!("Loading {}...", self.urdf_path)
        };

        // Start async loading
        self.start_async_load(cx);
    }

    /// Check if currently loading
    pub fn is_loading(&self) -> bool {
        matches!(self.loading_state, LoadingState::Loading { .. })
    }

    /// Start async robot loading in a background thread
    fn start_async_load(&mut self, cx: &mut Cx) {
        let (sender, receiver) = channel::<AsyncLoadResult>();
        self.async_receiver = Some(receiver);

        // Clone paths for the thread
        let urdf_path = self.urdf_path.clone();
        let assets_dir = self.assets_dir.clone();

        // Spawn background thread
        std::thread::spawn(move || {
            let result = load_robot(&urdf_path, &assets_dir);
            if sender.send(result).is_err() {
                eprintln!("Warning: Failed to send robot load result - receiver dropped");
            }
        });

        // Start polling timer (check every 16ms ~60fps)
        self.load_poll_timer = cx.start_interval(0.016);
    }

    /// Check if async loading has completed
    fn check_async_load(&mut self, cx: &mut Cx) -> bool {
        if let Some(ref receiver) = self.async_receiver {
            match receiver.try_recv() {
                Ok(result) => {
                    // Stop polling
                    self.load_poll_timer = Timer::default();
                    self.async_receiver = None;

                    // Process result
                    self.finalize_robot_load(cx, result);
                    return true;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    // Still loading
                    return false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    // Thread crashed
                    self.load_poll_timer = Timer::default();
                    self.async_receiver = None;
                    self.loading_state = LoadingState::Error {
                        message: "Loading thread disconnected unexpectedly".to_string()
                    };
                    return true;
                }
            }
        }
        false
    }

    /// Finalize robot loading after async completion
    fn finalize_robot_load(&mut self, cx: &mut Cx, result: AsyncLoadResult) {
        match result {
            Ok(mut robot) => {
                for link in robot.links.iter() {
                    if let Some(ref mesh_data) = link.mesh_data {
                        self.original_meshes.push(mesh_data.clone());
                        let mut draw = DrawMesh::new_for_link(cx, mesh_data.clone(), &self.draw_mesh);
                        draw.init_link_geometry(cx);
                        self.link_drawers.push(draw);
                    }
                }

                let axis_mesh = MeshData::cylinder(0.005, 0.15, 8);
                self.axis_mesh = Some(axis_mesh.clone());

                for _ in 0..robot.joints.len() {
                    let mut axis_draw = DrawMesh::new_for_link(cx, axis_mesh.clone(), &self.draw_mesh);
                    axis_draw.init_link_geometry(cx);
                    axis_draw.color = vec4(1.0, 0.2, 0.2, 1.0);
                    self.axis_drawers.push(axis_draw);
                }

                robot.update_forward_kinematics();
                self.robot = Some(robot);
                self.loading_state = LoadingState::Loaded;
            }
            Err(e) => {
                self.loading_state = LoadingState::Error {
                    message: e.to_string()
                };
            }
        }
        self.redraw(cx);
    }

    /// Get the current loading state
    pub fn get_loading_state(&self) -> &LoadingState {
        &self.loading_state
    }

    /// Check if a robot is loaded
    pub fn is_loaded(&self) -> bool {
        self.loading_state == LoadingState::Loaded && self.robot.is_some()
    }

    /// Get error message if in error state
    pub fn get_error(&self) -> Option<&str> {
        match &self.loading_state {
            LoadingState::Error { message } => Some(message),
            _ => None,
        }
    }

    fn init_grid(&mut self, cx: &mut Cx) {
        let grid_size = 10.0;  // 10 meter grid
        let grid = MeshData::ground_plane(grid_size, 0.0);
        self.grid_mesh = Some(grid.clone());

        let mut grid_draw = DrawMesh::new_for_link(cx, grid, &self.draw_mesh);
        grid_draw.init_link_geometry(cx);
        grid_draw.color = vec4(0.5, 0.7, 0.5, 1.0);  // Light green
        grid_draw.draw_grid_lines = 1.0;
        self.grid_drawer = Some(grid_draw);
    }

    fn init_world_axes(&mut self, cx: &mut Cx) {
        let axis_length = 0.15;
        let axis_radius = 0.001;
        let axis_mesh = MeshData::cylinder(axis_radius, axis_length, 6);

        for _ in 0..3 {
            let mut axis = DrawMesh::new_for_link(cx, axis_mesh.clone(), &self.draw_mesh);
            axis.init_link_geometry(cx);
            axis.color = vec4(0.1, 0.1, 0.1, 1.0);
            self.world_axis_drawers.push(axis);
        }
    }

    fn glam_to_makepad(m: glam::Mat4) -> Mat4 {
        Mat4 { v: m.to_cols_array() }
    }
}

impl Widget for RobotView {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Check for episode playback timer
        if self.episode_timer.is_event(event).is_some() && self.episode_playing {
            self.advance_episode_frame(cx);
        }

        // Check for async load completion on timer tick
        if self.load_poll_timer.is_event(event).is_some() && self.check_async_load(cx) {
            // Emit appropriate action based on result
            match &self.loading_state {
                LoadingState::Loaded => {
                    cx.widget_action(self.widget_uid(), &scope.path, RobotViewAction::LoadingComplete);
                }
                LoadingState::Error { message } => {
                    cx.widget_action(self.widget_uid(), &scope.path,
                        RobotViewAction::LoadingError { message: message.clone() });
                }
                _ => {}
            }
        }

        if let Event::KeyDown(ke) = event {
            let num_joints = self.robot.as_ref().map(|r| r.num_joints()).unwrap_or(6);
            match ke.key_code {
                KeyCode::ArrowUp => {
                    if let Some(ref mut robot) = self.robot {
                        if let Some(joint) = robot.joints.get(self.selected_joint) {
                            let current = joint.angle;
                            robot.set_joint_angle(self.selected_joint, current + 0.15);
                            if let Some(new_angle) = robot.joints.get(self.selected_joint).map(|j| j.angle) {
                                cx.widget_action(self.widget_uid(), &scope.path,
                                    RobotViewAction::JointChanged { joint_idx: self.selected_joint, angle: new_angle });
                            }
                        }
                    }
                    self.redraw(cx);
                }
                KeyCode::ArrowDown => {
                    if let Some(ref mut robot) = self.robot {
                        if let Some(joint) = robot.joints.get(self.selected_joint) {
                            let current = joint.angle;
                            robot.set_joint_angle(self.selected_joint, current - 0.15);
                            if let Some(new_angle) = robot.joints.get(self.selected_joint).map(|j| j.angle) {
                                cx.widget_action(self.widget_uid(), &scope.path,
                                    RobotViewAction::JointChanged { joint_idx: self.selected_joint, angle: new_angle });
                            }
                        }
                    }
                    self.redraw(cx);
                }
                KeyCode::ArrowLeft => {
                    if num_joints > 0 {
                        self.selected_joint = (self.selected_joint + num_joints - 1) % num_joints;
                    }
                    self.redraw(cx);
                }
                KeyCode::ArrowRight => {
                    if num_joints > 0 {
                        self.selected_joint = (self.selected_joint + 1) % num_joints;
                    }
                    self.redraw(cx);
                }
                KeyCode::KeyR => self.reset_view(cx),
                KeyCode::Equals => { self.camera.handle_key(KeyCode::Equals); self.redraw(cx); }
                KeyCode::Minus => { self.camera.handle_key(KeyCode::Minus); self.redraw(cx); }
                _ => {}
            }
        }

        let hit = event.hits(cx, self.area);
        if self.camera.handle_hit(&hit) {
            self.redraw(cx);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, _scope: &mut Scope, walk: Walk) -> DrawStep {
        cx.begin_turtle(walk, self.layout);
        let avail_rect = cx.turtle().rect();

        if !self.initialized {
            self.initialized = true;
            self.specular_enabled = true;
            self.init_robot(cx.cx);
        }

        if !self.grid_initialized {
            self.grid_initialized = true;
            self.init_grid(cx.cx);
        }

        if !self.world_axes_initialized || self.world_axis_drawers.is_empty() {
            self.world_axes_initialized = true;
            self.world_axis_drawers.clear();
            self.init_world_axes(cx.cx);
        }

        // Calculate View and Projection matrices
        let view_mat = self.camera.camera.view_matrix();
        let aspect_ratio = avail_rect.size.x / avail_rect.size.y;
        let proj_mat = self.camera.camera.projection_matrix(aspect_ratio);
        
        let view_mat_mkp = Self::glam_to_makepad(view_mat);
        let proj_mat_mkp = Self::glam_to_makepad(proj_mat);

        // Viewport rect (x, y, width, height) for embedded rendering
        let viewport_rect = vec4(
            avail_rect.pos.x as f32,
            avail_rect.pos.y as f32,
            avail_rect.size.x as f32,
            avail_rect.size.y as f32,
        );
        // Full window size - get from pass size
        let pass_size = cx.current_pass_size();
        let full_size = vec2(pass_size.x as f32, pass_size.y as f32);

        // Common rendering variables
        let base_rot = glam::Mat4::from_rotation_x(-std::f32::consts::FRAC_PI_2);
        let camera_pos = self.camera.position();
        let specular_strength = if self.specular_enabled { 0.5 } else { 0.0 };
        let draw_clip = vec4(
            avail_rect.pos.x as f32,
            avail_rect.pos.y as f32,
            (avail_rect.pos.x + avail_rect.size.x) as f32,
            (avail_rect.pos.y + avail_rect.size.y) as f32,
        );
        let window_size = vec2(
            avail_rect.size.x as f32,
            avail_rect.size.y as f32,
        );

        // Draw 3D skybox (gradient cube)
        if self.show_bg {
            self.draw_bg.set_view_matrix(&view_mat_mkp);
            self.draw_bg.set_projection_matrix(&proj_mat_mkp);
            self.draw_bg.set_draw_clip(draw_clip);
            self.draw_bg.set_viewport_rect(viewport_rect);
            self.draw_bg.set_full_size(full_size);
            self.draw_bg.begin_many_instances(cx);
            self.draw_bg.draw(cx);
            self.draw_bg.end_many_instances(cx);
        }

        // Draw grid (always, not dependent on robot being loaded)
        if let Some(ref mut grid_draw) = self.grid_drawer {
            // Grid at Y=0, no additional rotation needed since ground_plane is already rotated in init_grid
            let grid_model = Self::glam_to_makepad(glam::Mat4::IDENTITY);

            grid_draw.set_transform(&grid_model);
            grid_draw.set_view_matrix(&view_mat_mkp);
            grid_draw.set_projection_matrix(&proj_mat_mkp);
            grid_draw.set_draw_clip(draw_clip);
            grid_draw.set_window_size(window_size);
            grid_draw.set_viewport_rect(viewport_rect);
            grid_draw.set_full_size(full_size);

            grid_draw.begin_many_instances(cx);
            grid_draw.draw(cx);
            grid_draw.end_many_instances(cx);
        }

        if let Some(ref mut robot) = self.robot {
            robot.update_forward_kinematics();

            let link_colors = [
                vec4(0.3, 0.3, 0.35, 1.0),
                vec4(0.2, 0.4, 0.8, 1.0),
                vec4(0.2, 0.6, 0.9, 1.0),
                vec4(1.0, 0.9, 0.2, 1.0),
                vec4(0.15, 0.15, 0.15, 1.0),
                vec4(0.9, 0.6, 0.2, 1.0),
                vec4(0.8, 0.3, 0.3, 1.0),
                vec4(0.7, 0.7, 0.7, 1.0),
                vec4(0.6, 0.6, 0.65, 1.0),
                vec4(0.6, 0.6, 0.65, 1.0),
            ];

            // Draw robot links
            let mut drawer_idx = 0;
            for (link_idx, link) in robot.links.iter().enumerate() {
                if link.mesh_data.is_none() { continue; }
                if drawer_idx >= self.link_drawers.len() { break; }

                let link_transform = robot.link_transforms[link_idx];
                // Apply base rotation to convert URDF Z-up to rendering Y-up
                let model_mat = Self::glam_to_makepad(base_rot * link_transform);

                let drawer = &mut self.link_drawers[drawer_idx];
                drawer.set_transform(&model_mat);
                drawer.set_view_matrix(&view_mat_mkp);
                drawer.set_projection_matrix(&proj_mat_mkp);
                drawer.set_camera_position(camera_pos);
                drawer.set_specular_strength(specular_strength);
                drawer.set_draw_clip(draw_clip);
                drawer.set_window_size(window_size);
                drawer.set_viewport_rect(viewport_rect);
                drawer.color = link.color.map(|c| vec4(c[0], c[1], c[2], c[3]))
                    .unwrap_or(link_colors[drawer_idx % link_colors.len()]);

                drawer_idx += 1;
            }

            for drawer in &mut self.link_drawers {
                drawer.begin_many_instances(cx);
                drawer.draw(cx);
                drawer.end_many_instances(cx);
            }

            // Draw world axes
            if self.show_world_axes && self.world_axis_drawers.len() >= 3 {
                let base_x = 0.25;
                let base_z = 0.0;
                let ground_y = 0.01;
                let half_len = 0.075;

                let axes_transforms = [
                    (glam::Quat::from_rotation_z(-std::f32::consts::FRAC_PI_2), glam::Vec3::new(base_x + half_len, ground_y, base_z)),
                    (glam::Quat::IDENTITY, glam::Vec3::new(base_x, half_len + ground_y, base_z)),
                    (glam::Quat::from_rotation_x(std::f32::consts::FRAC_PI_2), glam::Vec3::new(base_x, ground_y, base_z + half_len)),
                ];

                for (i, (rot, pos)) in axes_transforms.iter().enumerate() {
                    let model = glam::Mat4::from_rotation_translation(*rot, *pos);
                    let model_mat = Self::glam_to_makepad(model);
                    self.world_axis_drawers[i].set_transform(&model_mat);
                    self.world_axis_drawers[i].set_view_matrix(&view_mat_mkp);
                    self.world_axis_drawers[i].set_projection_matrix(&proj_mat_mkp);
                    self.world_axis_drawers[i].set_camera_position(camera_pos);
                    self.world_axis_drawers[i].set_specular_strength(0.0);
                    self.world_axis_drawers[i].set_draw_clip(draw_clip);
                    self.world_axis_drawers[i].set_window_size(window_size);
                    self.world_axis_drawers[i].set_viewport_rect(viewport_rect);
                    self.world_axis_drawers[i].begin_many_instances(cx);
                    self.world_axis_drawers[i].draw(cx);
                    self.world_axis_drawers[i].end_many_instances(cx);
                }
            }

            // Draw joint axes
            if self.show_joint_axes {
                let axis_colors = [
                    vec4(1.0, 0.2, 0.2, 1.0), vec4(0.2, 1.0, 0.2, 1.0), vec4(0.2, 0.2, 1.0, 1.0),
                    vec4(1.0, 1.0, 0.2, 1.0), vec4(1.0, 0.2, 1.0, 1.0), vec4(0.2, 1.0, 1.0, 1.0),
                ];

                for (joint_idx, joint) in robot.joints.iter().enumerate() {
                    if joint_idx >= self.axis_drawers.len() { break; }

                    let parent_transform = robot.link_map.get(&joint.parent_link)
                        .map(|&idx| robot.link_transforms[idx])
                        .unwrap_or(glam::Mat4::IDENTITY);

                    let axis = joint.axis.normalize();
                    let up = glam::Vec3::Y;
                    let axis_rotation = if (axis - up).length() < 0.001 {
                        glam::Quat::IDENTITY
                    } else if (axis + up).length() < 0.001 {
                        glam::Quat::from_rotation_x(std::f32::consts::PI)
                    } else {
                        glam::Quat::from_rotation_arc(up, axis)
                    };

                    let origin_rot = glam::Quat::from_euler(glam::EulerRot::ZYX,
                        joint.origin_rpy.z, joint.origin_rpy.y, joint.origin_rpy.x);
                    let joint_transform = glam::Mat4::from_rotation_translation(
                        origin_rot * axis_rotation, joint.origin_xyz);
                    let model = parent_transform * joint_transform;
                    // Apply base rotation to convert URDF Z-up to rendering Y-up
                    let model_mat = Self::glam_to_makepad(base_rot * model);

                    let drawer = &mut self.axis_drawers[joint_idx];
                    drawer.set_transform(&model_mat);
                    drawer.set_view_matrix(&view_mat_mkp);
                    drawer.set_projection_matrix(&proj_mat_mkp);
                    drawer.set_camera_position(camera_pos);
                    drawer.set_specular_strength(0.3);
                    drawer.set_draw_clip(draw_clip);
                    drawer.set_window_size(window_size);
                    drawer.set_viewport_rect(viewport_rect);
                    drawer.color = axis_colors[joint_idx % axis_colors.len()];

                    drawer.begin_many_instances(cx);
                    drawer.draw(cx);
                    drawer.end_many_instances(cx);
                }
            }
        }

        cx.end_turtle_with_area(&mut self.area);
        DrawStep::done()
    }
}

impl RobotViewRef {
    pub fn animate_step(&self, cx: &mut Cx, step: u64) {
        if let Some(mut inner) = self.borrow_mut() { inner.animate_step(cx, step); }
    }

    pub fn load_robot(&self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        if let Some(mut inner) = self.borrow_mut() { inner.load_robot(cx, urdf_path, assets_dir); }
    }

    pub fn reload_robot(&self, cx: &mut Cx, urdf_path: &str, assets_dir: &str) {
        if let Some(mut inner) = self.borrow_mut() { inner.reload_robot(cx, urdf_path, assets_dir); }
    }

    pub fn set_joint_angle(&self, cx: &mut Cx, idx: usize, angle: f32) {
        if let Some(mut inner) = self.borrow_mut() { inner.set_joint_angle(cx, idx, angle); }
    }

    pub fn get_joint_angles(&self) -> Vec<f32> {
        self.borrow().map(|inner| inner.get_joint_angles()).unwrap_or_default()
    }

    pub fn set_joint_angles(&self, cx: &mut Cx, angles: &[f32]) {
        if let Some(mut inner) = self.borrow_mut() { inner.set_joint_angles(cx, angles); }
    }

    pub fn toggle_animation(&self, cx: &mut Cx) -> bool {
        self.borrow_mut().map(|mut inner| inner.toggle_animation(cx)).unwrap_or(false)
    }

    pub fn reset_view(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() { inner.reset_view(cx); }
    }

    pub fn get_selected_joint(&self) -> usize {
        self.borrow().map(|inner| inner.selected_joint).unwrap_or(0)
    }

    pub fn get_joint_info(&self, idx: usize) -> Option<(String, f32, f32, f32)> {
        self.borrow().and_then(|inner| {
            inner.robot.as_ref().and_then(|r| r.get_joint_info(idx))
                .map(|(name, angle, lower, upper)| (name.to_string(), angle, lower, upper))
        })
    }

    pub fn toggle_specular(&self, cx: &mut Cx) -> bool {
        self.borrow_mut().map(|mut inner| inner.toggle_specular(cx)).unwrap_or(false)
    }

    pub fn is_specular_enabled(&self) -> bool {
        self.borrow().map(|inner| inner.is_specular_enabled()).unwrap_or(true)
    }

    pub fn toggle_joint_axes(&self, cx: &mut Cx) -> bool {
        self.borrow_mut().map(|mut inner| inner.toggle_joint_axes(cx)).unwrap_or(false)
    }

    pub fn is_joint_axes_shown(&self) -> bool {
        self.borrow().map(|inner| inner.is_joint_axes_shown()).unwrap_or(false)
    }

    pub fn toggle_world_axes(&self, cx: &mut Cx) -> bool {
        self.borrow_mut().map(|mut inner| inner.toggle_world_axes(cx)).unwrap_or(false)
    }

    pub fn is_world_axes_shown(&self) -> bool {
        self.borrow().map(|inner| inner.is_world_axes_shown()).unwrap_or(false)
    }

    pub fn load_episode(&self, cx: &mut Cx, path: &str) {
        if let Some(mut inner) = self.borrow_mut() { inner.load_episode(cx, path); }
    }

    pub fn toggle_episode(&self, cx: &mut Cx) -> bool {
        self.borrow_mut().map(|mut inner| inner.toggle_episode(cx)).unwrap_or(false)
    }

    pub fn get_episode_info(&self) -> Option<(usize, usize)> {
        self.borrow().and_then(|inner| inner.get_episode_info())
    }

    pub fn is_episode_playing(&self) -> bool {
        self.borrow().map(|inner| inner.is_episode_playing()).unwrap_or(false)
    }
}

/// Remap a value from one range to another.
/// Returns `to_min` if the input range has zero width to avoid division by zero.
fn remap(value: f64, from_min: f64, from_max: f64, to_min: f64, to_max: f64) -> f64 {
    let range = from_max - from_min;
    if range.abs() < 1e-10 {
        return to_min;
    }
    let t = (value - from_min) / range;
    to_min + t * (to_max - to_min)
}
