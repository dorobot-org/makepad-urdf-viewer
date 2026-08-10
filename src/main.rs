//! Makepad URDF Viewer — desktop app shell.
//!
//! Header buttons switch robots (including the Redbank III telescope unit
//! and 4x8 array); the viewport is the embeddable RobotView widget.

pub use makepad_urdf_player;
pub use makepad_widgets;
pub use makepad_xr;

use makepad_urdf_player::robot::{scan_folder, ModelFile, ModelKind};
use makepad_urdf_player::robot_view::RobotViewWidgetRefExt;
use makepad_widgets::*;

app_main!(App);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    load_all_resources() do #(App::script_component(vm)){
        ui: Root{
            main_window := Window{
                window.inner_size: vec2(1280, 820)
                body +: {
                    app_view := SolidView{
                        width: Fill
                        height: Fill
                        flow: Down
                        draw_bg +: {color: #x0d1116}

                        header := SolidView{
                            width: Fill
                            height: 46.0
                            flow: Right
                            align: Align{x: 0.0 y: 0.5}
                            padding: Inset{left: 12.0 right: 12.0}
                            spacing: 8.0
                            draw_bg +: {color: #x171d24}

                            rb_unit_btn := Button{text: "Redbank III Unit"}
                            rb_array_btn := Button{text: "Redbank 4x8 Array"}
                            so100_btn := Button{text: "SO-100 Arm"}
                            open_btn := Button{text: "Open folder…"}
                            model_pick := DropDown{labels: ["(no folder)"]}
                            light_btn := Button{text: "Light: off"}
                            hint := Label{
                                text: "drag orbit · alt drag light · shift/right pan · wheel zoom · arrows joints · A/R/L"
                                draw_text +: {color: #x8391a0}
                            }
                        }

                        viewport := mod.widgets.RobotView{
                            // the library widget starts empty; this demo asks
                            // for a model declaratively
                            urdf: "data/redbank/redbank_unit.urdf"
                            assets: "data/redbank"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
const ROBOTS: &[(&str, &str, &str)] = &[
    ("rb_unit_btn", "data/redbank/redbank_unit.urdf", "data/redbank"),
    ("rb_array_btn", "data/redbank/redbank_array.urdf", "data/redbank"),
    ("so100_btn", "data/so100.urdf", "data"),
];

/// wasm resolves the same models from bytes embedded in the binary.
#[cfg(target_arch = "wasm32")]
const ROBOTS: &[(&str, &str, &str)] = &[
    ("rb_unit_btn", "redbank_unit.urdf", "embedded"),
    ("rb_array_btn", "redbank_array.urdf", "embedded"),
    ("so100_btn", "so100.urdf", "embedded"),
];

#[cfg(target_arch = "wasm32")]
fn register_embedded_assets() {
    use makepad_urdf_player::robot::set_virtual_assets;
    let mut m = std::collections::HashMap::new();
    m.insert("redbank_unit.urdf",
             include_bytes!("../data/redbank/redbank_unit.urdf").as_slice());
    m.insert("redbank_array.urdf",
             include_bytes!("../data/redbank/redbank_array.urdf").as_slice());
    m.insert("tube_body.stl",
             include_bytes!("../data/redbank/meshes/tube_body.stl").as_slice());
    m.insert("tube_rear_cap.stl",
             include_bytes!("../data/redbank/meshes/tube_rear_cap.stl").as_slice());
    m.insert("array_frame.stl",
             include_bytes!("../data/redbank/meshes/array_frame.stl").as_slice());
    m.insert("lens.stl",
             include_bytes!("../data/redbank/meshes/lens.stl").as_slice());
    m.insert("so100.urdf",
             include_bytes!("../data/so100.urdf").as_slice());
    m.insert("Base.stl",
             include_bytes!("../data/assets/Base.stl").as_slice());
    m.insert("Base_Motor.stl",
             include_bytes!("../data/assets/Base_Motor.stl").as_slice());
    m.insert("Fixed_Jaw.stl",
             include_bytes!("../data/assets/Fixed_Jaw.stl").as_slice());
    m.insert("Fixed_Jaw_Motor.stl",
             include_bytes!("../data/assets/Fixed_Jaw_Motor.stl").as_slice());
    m.insert("Lower_Arm.stl",
             include_bytes!("../data/assets/Lower_Arm.stl").as_slice());
    m.insert("Lower_Arm_Motor.stl",
             include_bytes!("../data/assets/Lower_Arm_Motor.stl").as_slice());
    m.insert("Moving_Jaw.stl",
             include_bytes!("../data/assets/Moving_Jaw.stl").as_slice());
    m.insert("Rotation_Pitch.stl",
             include_bytes!("../data/assets/Rotation_Pitch.stl").as_slice());
    m.insert("Rotation_Pitch_Motor.stl",
             include_bytes!("../data/assets/Rotation_Pitch_Motor.stl").as_slice());
    m.insert("Upper_Arm.stl",
             include_bytes!("../data/assets/Upper_Arm.stl").as_slice());
    m.insert("Upper_Arm_Motor.stl",
             include_bytes!("../data/assets/Upper_Arm_Motor.stl").as_slice());
    m.insert("Wrist_Pitch_Roll.stl",
             include_bytes!("../data/assets/Wrist_Pitch_Roll.stl").as_slice());
    m.insert("Wrist_Pitch_Roll_Motor.stl",
             include_bytes!("../data/assets/Wrist_Pitch_Roll_Motor.stl").as_slice());
    set_virtual_assets(m);
}

#[derive(Script, ScriptHook)]
pub struct App {
    #[live]
    ui: WidgetRef,
    #[rust(false)]
    light_label_on: bool,
    /// models discovered by the last folder scan, parallel to the dropdown
    #[rust]
    found: Vec<ModelFile>,
}

impl App {
    fn load_robot(&mut self, cx: &mut Cx, urdf: &str, assets: &str) {
        // demo of the library's host API: no borrowing, and failures surface
        // as a RobotViewAction in handle_actions below
        let _ = self.ui.robot_view(cx, ids!(viewport)).load_robot(cx, urdf, assets);
    }

    /// Lamp on = the sun is drawn in the sky and lights the model. Moving it
    /// is alt+drag, so orbit / pan / joint keys are unaffected either way.
    fn toggle_light(&mut self, cx: &mut Cx) {
        let view = self.ui.robot_view(cx, ids!(viewport));
        let on = !view.is_light_on();
        view.set_light_on(cx, on);
        self.sync_light_label(cx);
    }

    /// The widget also switches the lamp on by itself (alt+drag) — keep the
    /// button label in step with it.
    fn sync_light_label(&mut self, cx: &mut Cx) {
        let on = self.ui.robot_view(cx, ids!(viewport)).is_light_on();
        if on != self.light_label_on {
            self.light_label_on = on;
            let label = if on { "Light: on" } else { "Light: off" };
            self.ui.widget(cx, ids!(light_btn)).set_text(cx, label);
        }
    }
}

impl App {
    /// Ask the OS for a folder. makepad's own folder dialog is a stub on
    /// macOS (it prints and returns), so this shells out to the system
    /// picker; elsewhere, pass a folder on the command line instead.
    fn pick_folder() -> Option<String> {
        #[cfg(target_os = "macos")]
        {
            let out = std::process::Command::new("osascript")
                .args([
                    "-e",
                    "POSIX path of (choose folder with prompt \"Choose a folder of URDF / STL / OBJ files\")",
                ])
                .output()
                .ok()?;
            if !out.status.success() {
                return None; // user cancelled
            }
            let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
            return (!path.is_empty()).then_some(path);
        }
        #[allow(unreachable_code)]
        {
            log!("pass a folder on the command line: makepad-urdf-player <folder>");
            None
        }
    }

    /// Scan a folder and offer whatever it holds in the dropdown.
    fn open_folder(&mut self, cx: &mut Cx, dir: &str) {
        self.found = scan_folder(dir, 2);
        let pick = self.ui.drop_down(cx, ids!(model_pick));
        if self.found.is_empty() {
            pick.set_labels(cx, vec!["(nothing found)".to_string()]);
            log!("no .urdf/.stl/.obj under {}", dir);
            return;
        }
        let labels: Vec<String> = self
            .found
            .iter()
            .map(|m| match m.kind {
                ModelKind::Urdf => m.relative.clone(),
                ModelKind::Mesh => format!("{} (mesh)", m.relative),
            })
            .collect();
        log!("found {} model(s) under {}", labels.len(), dir);
        pick.set_labels(cx, labels);
        // open the first one so the folder shows something immediately
        let first = self.found[0].path.to_string_lossy().to_string();
        let _ = self.ui.robot_view(cx, ids!(viewport)).open_path(cx, &first);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        // `makepad-urdf-player <folder>` opens that folder at startup, so the
        // feature is usable without the GUI picker (and on platforms where
        // there isn't one).
        if let Some(dir) = std::env::args().nth(1) {
            if std::path::Path::new(&dir).is_dir() {
                self.open_folder(cx, &dir);
            } else {
                // a single file works too
                let _ = self.ui.robot_view(cx, ids!(viewport)).open_path(cx, &dir);
            }
        }
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let mut load: Option<(&str, &str)> = None;
        for (btn, urdf, assets) in ROBOTS {
            if self
                .ui
                .button(cx, &[LiveId::from_str(btn)])
                .clicked(actions)
            {
                load = Some((urdf, assets));
            }
        }
        if let Some((urdf, assets)) = load {
            self.load_robot(cx, urdf, assets);
        }
        if self.ui.button(cx, ids!(light_btn)).clicked(actions) {
            self.toggle_light(cx);
        }
        if self.ui.button(cx, ids!(open_btn)).clicked(actions) {
            if let Some(dir) = Self::pick_folder() {
                self.open_folder(cx, &dir);
            }
        }
        if let Some(i) = self.ui.drop_down(cx, ids!(model_pick)).selected(actions) {
            if let Some(model) = self.found.get(i) {
                let path = model.path.to_string_lossy().to_string();
                let _ = self.ui.robot_view(cx, ids!(viewport)).open_path(cx, &path);
            }
        }
        // the widget reports load outcomes; a real host would surface these
        let view = self.ui.robot_view(cx, ids!(viewport));
        if let Some((links, joints)) = view.loaded(actions) {
            log!("app: model loaded — {} links, {} movable joints", links, joints);
        }
        if let Some((path, err)) = view.load_failed(actions) {
            error!("app: could not load {}: {}", path, err);
        }
    }
}

impl AppMain for App {
    fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
        #[cfg(target_arch = "wasm32")]
        register_embedded_assets();
        crate::makepad_widgets::script_mod(vm);
        crate::makepad_xr::script_mod(vm);
        makepad_urdf_player::script_mod(vm);
        self::script_mod(vm)
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Event::KeyDown(ke) = event {
            if ke.key_code == KeyCode::KeyL && !ke.is_repeat {
                self.toggle_light(cx);
            }
        }
        self.match_event(cx, event);
        self.ui.handle_event(cx, event, &mut Scope::empty());
        if let Event::MouseUp(_) = event {
            self.sync_light_label(cx);
        }
    }
}
