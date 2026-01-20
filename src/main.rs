//! Makepad URDF Player - 3D Robot Viewer
//!
//! A Makepad-based URDF robot visualization application.
//!
//! Controls:
//!   - Mouse drag: Orbit camera
//!   - Scroll: Zoom
//!   - Arrow keys: Control joints (Left/Right to select, Up/Down to adjust)
//!   - A: Toggle animation
//!   - R: Reset to default pose

use makepad_widgets::*;

// Import from the library crate
use makepad_urdf_player::robot_view::{RobotViewAction, RobotViewWidgetExt};

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    // Import RobotView from library crate
    use makepad_urdf_player::robot_view::RobotView;

    URDFViewer = {{URDFViewer}} {
        width: Fill
        height: Fill
        flow: Overlay  // Allow Modal to float over content

        // Main content container - use Overlay so header draws on top
        main_content = <View> {
            width: Fill
            height: Fill
            flow: Overlay

            // Viewport goes first (drawn first, behind header)
            viewport_container = <View> {
                width: Fill
                height: Fill
                margin: { top: 40 }  // Leave space for header

        viewport = <View> {
            width: Fill
            height: Fill
            flow: Overlay
            show_bg: false
            clip_x: true
            clip_y: true

            robot_view = <RobotView> {
                width: Fill
                height: Fill
            }

            // Status overlay positioned at bottom of viewport
            status_overlay = <View> {
                width: Fill
                height: 36
                margin: { left: 8, top: 0 }
                align: { x: 0.0, y: 1.0 }
                show_bg: true
                draw_bg: { color: #2a2a3580 }

                joint_label = <Label> {
                    draw_text: {
                        text_style: { font_size: 12.0 }
                        color: #ccc
                    }
                    text: "Joint 0: 0.00 rad"
                }
            }
        } // end viewport
            } // end viewport_container

            // Header goes second (drawn on top)
            header = <View> {
            width: Fill
            height: 40
            padding: 8
            spacing: 16
            flow: Right
            align: { y: 0.5 }
            show_bg: true
            draw_bg: { color: #2a2a35 }

            open_btn = <Button> {
                text: "Open Robot..."
                draw_text: { color: #fff }
            }

            <View> { width: 16 }

            robot_name_label = <Label> {
                draw_text: {
                    text_style: { font_size: 14.0 }
                    color: #4080c0
                }
                text: "VX300s (ALOHA)"
            }

            <View> { width: 16 }

            <Label> {
                draw_text: {
                    text_style: { font_size: 12.0 }
                    color: #888
                }
                text: "Arrows=joints, +/-=zoom, A=animate, R=reset"
            }

            <View> { width: Fill }

            light_btn = <Button> {
                text: "Light: ON"
                draw_text: { color: #ff0 }
            }

            <View> { width: 8 }

            axes_btn = <Button> {
                text: "Axes: OFF"
                draw_text: { color: #888 }
            }

            <View> { width: 8 }

            xyz_btn = <Button> {
                text: "XYZ: OFF"
                draw_text: { color: #888 }
            }

            <View> { width: 8 }

            episode_btn = <Button> {
                text: "Episode: OFF"
                draw_text: { color: #888 }
                visible: false
            }

            <View> { width: 8 }

            reset_btn = <Button> {
                text: "Reset"
                draw_text: { color: #fff }
            }
        }
        } // end main_content

        // Robot selection modal (floats over main_content due to Overlay flow)
        robot_modal = <Modal> {
            content: <View> {
                width: 400
                height: 300
                padding: 20
                spacing: 16
                flow: Down
                show_bg: true
                draw_bg: { color: #2a2a35 }

                <Label> {
                    draw_text: {
                        text_style: { font_size: 18.0 }
                        color: #fff
                    }
                    text: "Select Robot Model"
                }

                // Robot selection buttons
                <View> {
                    width: Fill
                    height: Fit
                    flow: Down
                    spacing: 4

                    vx300s_btn = <Button> {
                        width: Fill
                        text: "VX300s (ALOHA) - ViperX 300 6DOF"
                        draw_text: { color: #fff }
                        draw_bg: { color: #4080c0 }
                    }

                    so100_btn = <Button> {
                        width: Fill
                        text: "SO100 - SO-ARM100 Robot"
                        draw_text: { color: #ccc }
                    }

                    lekiwi_btn = <Button> {
                        width: Fill
                        text: "LeKiwi - Mobile Manipulator"
                        draw_text: { color: #ccc }
                    }

                    icub_btn = <Button> {
                        width: Fill
                        text: "iCub - Humanoid Robot"
                        draw_text: { color: #ccc }
                    }
                }

                robot_info = <Label> {
                    width: Fill
                    margin: { top: 12 }
                    draw_text: {
                        text_style: { font_size: 12.0 }
                        color: #888
                    }
                    text: "Click a robot above to load it"
                }

                <View> { height: Fill }

                <View> {
                    width: Fill
                    height: Fit
                    flow: Right
                    align: { x: 1.0 }

                    cancel_btn = <Button> {
                        text: "Cancel"
                    }
                }
            }
        }
    }

    App = {{App}} {
        ui: <Window> {
            window: { title: "VX300s (ALOHA) Robot Viewer" }
            show_bg: false  // Don't draw window background - let 3D content show
            body = <URDFViewer> {}
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct URDFViewer {
    #[deref] view: View,
    #[rust] animating: bool,
    #[rust] anim_timer: Timer,
    #[rust] anim_step: u64,
    #[rust] auto_started: bool,
    #[rust] has_episode: bool,
    #[rust] pending_episode_path: Option<String>,
}

impl URDFViewer {
    fn update_status(&mut self, cx: &mut Cx) {
        let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
        let selected = robot_view.get_selected_joint();

        let text = if let Some((current_frame, total_frames)) = robot_view.get_episode_info() {
            // Episode playback mode - show frame info
            if let Some((name, angle, _, _)) = robot_view.get_joint_info(selected) {
                format!(
                    "Frame {}/{} | Joint {}: {} = {:.1}°",
                    current_frame + 1, total_frames,
                    selected, name, angle.to_degrees()
                )
            } else {
                format!("Frame {}/{}", current_frame + 1, total_frames)
            }
        } else if let Some((name, angle, lower, upper)) = robot_view.get_joint_info(selected) {
            format!(
                "Joint {}: {} = {:.2} rad ({:.1}°) [{:.1}° to {:.1}°]",
                selected, name, angle, angle.to_degrees(),
                lower.to_degrees(), upper.to_degrees()
            )
        } else {
            "Loading robot...".to_string()
        };
        self.view.label(id!(main_content.viewport_container.viewport.status_overlay.joint_label)).set_text(cx, &text);
    }
}

impl Widget for URDFViewer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Capture all actions from a single handle_event call
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Animation timer - managed at URDFViewer level
        if self.anim_timer.is_event(event).is_some() && self.animating {
            self.anim_step += 1;
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            robot_view.animate_step(cx, self.anim_step);
            self.update_status(cx);
            self.redraw(cx);
        }

        // Handle keyboard at top level
        if let Event::KeyDown(ke) = event {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            match ke.key_code {
                KeyCode::ArrowUp | KeyCode::ArrowDown | KeyCode::ArrowLeft | KeyCode::ArrowRight => {
                    // Forward to RobotView - it handles these internally
                }
                KeyCode::KeyA => {
                    self.animating = !self.animating;
                    if self.animating {
                        self.anim_timer = cx.start_interval(0.033);
                    } else {
                        self.anim_timer = Timer::default();
                    }
                }
                KeyCode::KeyR => {
                    robot_view.reset_view(cx);
                    self.update_status(cx);
                }
                _ => {}
            }
            self.update_status(cx);
        }

        for action in &actions {
            match action.as_widget_action().cast::<RobotViewAction>() {
                RobotViewAction::JointChanged { .. } => {
                    self.update_status(cx);
                }
                RobotViewAction::AnimationToggled(_) => {
                    self.update_status(cx);
                }
                RobotViewAction::LoadingComplete => {
                    // Robot finished loading - load pending episode if any
                    if let Some(episode_path) = self.pending_episode_path.take() {
                        let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
                        robot_view.load_episode(cx, &episode_path);
                        eprintln!("=== Episode loaded after robot ready");
                    }
                    self.update_status(cx);
                }
                _ => {}
            }
        }

        // Reset button
        if self.view.button(id!(main_content.header.reset_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            robot_view.reset_view(cx);
            self.update_status(cx);
        }

        // Light toggle button
        if self.view.button(id!(main_content.header.light_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            let enabled = robot_view.toggle_specular(cx);
            let text = if enabled { "Light: ON" } else { "Light: OFF" };
            self.view.button(id!(main_content.header.light_btn)).set_text(cx, text);
        }

        // Joint axes toggle button
        if self.view.button(id!(main_content.header.axes_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            let enabled = robot_view.toggle_joint_axes(cx);
            let text = if enabled { "Axes: ON" } else { "Axes: OFF" };
            self.view.button(id!(main_content.header.axes_btn)).set_text(cx, text);
        }

        // World XYZ axes toggle button
        if self.view.button(id!(main_content.header.xyz_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            let enabled = robot_view.toggle_world_axes(cx);
            let text = if enabled { "XYZ: ON" } else { "XYZ: OFF" };
            self.view.button(id!(main_content.header.xyz_btn)).set_text(cx, text);
        }

        // Episode playback toggle button
        if self.view.button(id!(main_content.header.episode_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            let playing = robot_view.toggle_episode(cx);
            let text = if playing { "Episode: PAUSE" } else { "Episode: PLAY" };
            self.view.button(id!(main_content.header.episode_btn)).set_text(cx, text);
            self.update_status(cx);
        }

        // Open robot selection modal
        if self.view.button(id!(main_content.header.open_btn)).clicked(&actions) {
            println!("Opening modal...");
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, false);
            self.view.modal(id!(robot_modal)).open(cx);
        }

        // Robot selection buttons in modal - click to load directly
        if self.view.button(id!(robot_modal.vx300s_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            robot_view.reload_robot(cx, "data/vx300s/vx300s.urdf", "data/vx300s");
            self.has_episode = false;
            self.pending_episode_path = None;
            self.view.button(id!(main_content.header.episode_btn)).set_visible(cx, false);
            self.view.label(id!(main_content.header.robot_name_label)).set_text(cx, "VX300s (ALOHA)");
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, true);
            self.update_status(cx);
        }

        if self.view.button(id!(robot_modal.so100_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            // URDF uses filename="assets/Base.stl", so assets_dir should be "data/so100"
            robot_view.reload_robot(cx, "data/so100/so100.urdf", "data/so100");
            // Store episode path to load after robot finishes loading
            let episode_path = std::env::var("HOME").unwrap_or_default() + "/home/dorobot/dataset/aimee-6283/data/chunk-000/episode_000000.parquet";
            self.pending_episode_path = Some(episode_path);
            self.has_episode = true;
            // Stop sine wave animation
            self.animating = false;
            self.anim_timer = Timer::default();
            // Show and update episode button
            self.view.button(id!(main_content.header.episode_btn)).set_visible(cx, true);
            self.view.button(id!(main_content.header.episode_btn)).set_text(cx, "Episode: PAUSE");
            self.view.label(id!(main_content.header.robot_name_label)).set_text(cx, "SO100 (Episode)");
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, true);
            self.update_status(cx);
        }

        if self.view.button(id!(robot_modal.lekiwi_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            robot_view.reload_robot(cx, "data/lekiwi/LeKiwi.urdf", "data/lekiwi");
            self.has_episode = false;
            self.pending_episode_path = None;
            self.view.button(id!(main_content.header.episode_btn)).set_visible(cx, false);
            self.view.label(id!(main_content.header.robot_name_label)).set_text(cx, "LeKiwi");
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, true);
            self.update_status(cx);
        }

        if self.view.button(id!(robot_modal.icub_btn)).clicked(&actions) {
            let robot_view = self.view.robot_view(id!(main_content.viewport_container.viewport.robot_view));
            robot_view.reload_robot(cx, "data/icub/model.urdf", "data/icub");
            self.has_episode = false;
            self.pending_episode_path = None;
            self.view.button(id!(main_content.header.episode_btn)).set_visible(cx, false);
            self.view.label(id!(main_content.header.robot_name_label)).set_text(cx, "iCub");
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, true);
            self.update_status(cx);
        }

        // Cancel button in modal
        if self.view.button(id!(robot_modal.cancel_btn)).clicked(&actions) {
            self.view.modal(id!(robot_modal)).close(cx);
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, true);
        }

        // Load button removed - clicking robot buttons loads directly

        // Modal dismissed (click outside or Escape)
        if self.view.modal(id!(robot_modal)).dismissed(&actions) {
            self.view.view(id!(main_content.viewport_container)).set_visible(cx, true);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Auto-start animation for profiling (after first frame)
        if !self.auto_started {
            self.auto_started = true;
            self.animating = true;
            self.anim_timer = cx.cx.start_interval(0.033);
            eprintln!("=== Auto-starting animation for profiling ===");
        }
        self.view.draw_walk(cx, scope, walk)
    }
}

#[derive(Live, LiveHook)]
pub struct App {
    #[live] ui: WidgetRef,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        // Order matters: dependencies first!
        makepad_widgets::live_design(cx);
        makepad_urdf_player::live_design(cx);  // Register library's live_design
    }
}

impl MatchEvent for App {}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
    }
}

app_main!(App);

fn main() {
    app_main();
}
