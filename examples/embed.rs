//! Smallest useful integration of the `RobotView` widget.
//!
//!     cargo run --example embed -- data/so100.urdf data
//!
//! Shows the three things a host needs: register the script module, place the
//! widget (here it loads declaratively via its `urdf`/`assets` properties),
//! and react to what it reports back.

use makepad_urdf_player::robot_view::RobotViewWidgetRefExt;
use makepad_widgets::*;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    load_all_resources() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(900, 640)
                body +: {
                    root := SolidView{
                        width: Fill
                        height: Fill
                        flow: Down
                        draw_bg +: {color: #x11151b}

                        bar := SolidView{
                            width: Fill
                            height: 40.0
                            flow: Right
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 10.0 right: 10.0}
                            spacing: 8.0
                            draw_bg +: {color: #x1b2129}
                            reset_btn := Button{text: "Reset view"}
                            light_btn := Button{text: "Toggle light"}
                            wave_btn := Button{text: "Wave joints"}
                            status := Label{
                                text: "loading..."
                                draw_text +: {color: #x93a1b0}
                            }
                        }

                        // The widget itself. Everything below is optional —
                        // drop `urdf`/`assets` and call `load_robot` instead,
                        // and re-theme the environment to taste.
                        viewer := mod.widgets.RobotView{
                            urdf: ""
                            assets: ""
                            show_grid: true
                            sky_horizon: #xFFFBFB
                            sky_zenith: #xFAE5E7
                            ground_color: #xFFFFC5
                            grid_color: #x6B6A3D
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust]
    phase: f32,
    #[rust]
    waving: bool,
    #[rust]
    next_frame: NextFrame,
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // CLI args, so the example works with any URDF on disk
        let args: Vec<String> = std::env::args().collect();
        let urdf = args
            .get(1)
            .cloned()
            .unwrap_or_else(|| "data/so100.urdf".to_string());
        let assets = args.get(2).cloned().unwrap_or_else(|| "data".to_string());

        if let Err(e) = self
            .ui
            .robot_view(cx, ids!(viewer))
            .load_robot(cx, &urdf, &assets)
        {
            error!("load failed: {e}");
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let viewer = self.ui.robot_view(cx, ids!(viewer));

        // what the widget reports back
        if let Some((links, joints)) = viewer.loaded(actions) {
            self.ui.widget(cx, ids!(status)).set_text(
                cx,
                &format!("{links} links · {joints} movable joints"),
            );
        }
        if let Some((path, err)) = viewer.load_failed(actions) {
            self.ui
                .widget(cx, ids!(status))
                .set_text(cx, &format!("failed: {path}"));
            error!("{path}: {err}");
        }

        if self.ui.button(cx, ids!(reset_btn)).clicked(actions) {
            viewer.reset_view(cx);
        }
        if self.ui.button(cx, ids!(light_btn)).clicked(actions) {
            let on = !viewer.is_light_on();
            viewer.set_light_on(cx, on);
        }
        if self.ui.button(cx, ids!(wave_btn)).clicked(actions) {
            self.waving = !self.waving;
            if self.waving {
                self.next_frame = cx.new_next_frame();
            }
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        makepad_widgets::script_mod(vm);
        makepad_xr::script_mod(vm);
        // the widget's own shaders and DSL types
        makepad_urdf_player::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());

        // drive the joints yourself — this is the hook a teleop UI or a
        // dataset player would use
        if self.next_frame.is_event(event).is_some() && self.waving {
            self.phase += 1.0 / 60.0;
            let viewer = self.ui.robot_view(cx, ids!(viewer));
            let n = viewer.movable_joint_count();
            let angles: Vec<f32> = (0..n)
                .map(|i| (self.phase * 1.2 + i as f32 * 0.6).sin() * 0.6)
                .collect();
            viewer.set_joint_angles(cx, &angles);
            self.next_frame = cx.new_next_frame();
        }
    }
}
