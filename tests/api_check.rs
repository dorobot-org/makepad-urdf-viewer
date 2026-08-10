// compile-only check that the README's documented API exists as written
#![allow(unused)]
use makepad_urdf_player::robot_view::{RobotViewAction, RobotViewRef, RobotViewWidgetRefExt};
use makepad_urdf_player::robot::{load_robot, set_virtual_assets, ForwardKinematics, Robot};
use makepad_widgets::*;

fn api(cx: &mut Cx, ui: &WidgetRef, actions: &Actions) {
    let viewer: RobotViewRef = ui.robot_view(cx, ids!(viewer));
    let _: Result<(), String> = viewer.load_robot(cx, "a.urdf", "data");
    viewer.clear_robot(cx);
    let _: Option<(usize, usize)> = viewer.loaded(actions);
    let _: Option<(String, String)> = viewer.load_failed(actions);
    viewer.set_joint_angles(cx, &[0.0]);
    let _: Vec<f32> = viewer.joint_angles();
    let _: usize = viewer.movable_joint_count();
    viewer.reset_view(cx);
    let _: bool = viewer.is_light_on();
    viewer.set_light_on(cx, true);
    viewer.set_light_angles(cx, 0.8, 0.6);
    let _: bool = viewer.is_animating();
    viewer.set_animating(cx, true);
    // documented: as_robot_view on a generic WidgetRef
    let _alt: RobotViewRef = ui.as_robot_view();
    // documented: inspecting through borrow(). The Ref borrows from `viewer`,
    // so bind the handle first (this is what the README shows).
    let borrowed = viewer.borrow();
    if let Some(inner) = borrowed.as_ref() {
        let _: Option<&Robot> = inner.robot();
        let _: &[usize] = inner.movable_joints();
        let _: (f32, f32) = inner.light_angles();
    }
    drop(borrowed);
}

fn model_api() -> Result<(), Box<dyn std::error::Error>> {
    let mut robot: Robot = load_robot("data/so100.urdf", "data")?;
    ForwardKinematics::update(&mut robot);
    let _: usize = robot.num_links();
    let _: usize = robot.num_movable_joints();
    let _: Option<(&str, f32, f32, f32)> = robot.get_joint_info(0);
    let _: Option<glam::Mat4> = robot.get_link_transform(3);
    let _: (glam::Vec3, glam::Vec3) = robot.bounds_world();
    robot.set_joint_angle(0, 0.5);
    robot.set_joint_angle_unclamped(0, 9.0);
    robot.reset_joints();
    Ok(())
}
