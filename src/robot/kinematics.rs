//! Forward kinematics for robot arm chains
//!
//! Computes world-space transforms for each link based on joint angles.

use std::collections::HashSet;
use super::model::Robot;

/// Forward kinematics solver
pub struct ForwardKinematics;

impl ForwardKinematics {
    /// Update all link transforms based on current joint angles
    ///
    /// Uses a breadth-first traversal from the root link, computing
    /// the world transform of each link as it goes.
    pub fn update(robot: &mut Robot) {
        Self::compute_transforms(robot);
    }

    /// Compute forward kinematics for the entire robot
    fn compute_transforms(robot: &mut Robot) {
        let root_offset = glam::Mat4::IDENTITY;

        // Set root link transform
        if let Some(&root_idx) = robot.link_map.get(&robot.root_link) {
            robot.link_transforms[root_idx] = root_offset;
        }

        // Track which links have been processed
        let mut processed = HashSet::new();
        processed.insert(robot.root_link.clone());

        // Propagate transforms through the kinematic chain
        // We iterate multiple times to handle arbitrary tree structures
        for _ in 0..robot.joints.len() {
            for joint in &robot.joints {
                // Skip if parent hasn't been processed yet
                if !processed.contains(&joint.parent_link) {
                    continue;
                }
                // Skip if child has already been processed
                if processed.contains(&joint.child_link) {
                    continue;
                }

                // Get parent transform
                let parent_transform = if let Some(&parent_idx) = robot.link_map.get(&joint.parent_link)
                {
                    robot.link_transforms[parent_idx]
                } else {
                    root_offset
                };

                // Compute joint transform
                let joint_transform = Self::compute_joint_transform(joint);

                // Child transform = parent * joint
                let child_transform = parent_transform * joint_transform;

                // Store child transform
                if let Some(&child_idx) = robot.link_map.get(&joint.child_link) {
                    robot.link_transforms[child_idx] = child_transform;
                }

                processed.insert(joint.child_link.clone());
            }
        }
    }

    /// Compute the transform for a single joint
    fn compute_joint_transform(joint: &super::model::RobotJoint) -> glam::Mat4 {
        // URDF RPY is extrinsic XYZ (roll, pitch, yaw)
        // = Rz(yaw) * Ry(pitch) * Rx(roll)
        // glam intrinsic ZYX with (yaw, pitch, roll) gives the same result
        let origin_rotation = glam::Quat::from_euler(
            glam::EulerRot::ZYX,
            joint.origin_rpy.z,
            joint.origin_rpy.y,
            joint.origin_rpy.x,
        );

        // Joint rotation around axis
        let joint_rotation = glam::Quat::from_axis_angle(joint.axis, joint.angle);

        // Combined rotation: origin rotation first, then joint rotation
        let rotation = origin_rotation * joint_rotation;

        // Build transform matrix
        glam::Mat4::from_rotation_translation(rotation, joint.origin_xyz)
    }

    /// Get the world transform of a specific link
    pub fn get_link_transform(robot: &Robot, link_name: &str) -> Option<glam::Mat4> {
        robot.link_map.get(link_name).map(|&idx| robot.link_transforms[idx])
    }

    /// Get the world position of a specific link
    pub fn get_link_position(robot: &Robot, link_name: &str) -> Option<glam::Vec3> {
        Self::get_link_transform(robot, link_name).map(|transform| {
            glam::Vec3::new(transform.w_axis.x, transform.w_axis.y, transform.w_axis.z)
        })
    }

    /// Get the world orientation (quaternion) of a specific link
    pub fn get_link_orientation(robot: &Robot, link_name: &str) -> Option<glam::Quat> {
        Self::get_link_transform(robot, link_name)
            .map(|transform| glam::Quat::from_mat4(&transform))
    }

    /// Compute the end-effector position (last link in chain)
    pub fn get_end_effector_position(robot: &Robot) -> Option<glam::Vec3> {
        // Find the link that is the child of the last joint
        if let Some(last_joint) = robot.joints.last() {
            Self::get_link_position(robot, &last_joint.child_link)
        } else {
            // No joints, return root link position
            Self::get_link_position(robot, &robot.root_link)
        }
    }

    /// Compute Jacobian matrix for velocity kinematics (future use)
    /// Returns a 6xN matrix where N is the number of movable joints
    #[allow(dead_code)]
    pub fn compute_jacobian(_robot: &Robot) -> Vec<[f32; 6]> {
        // TODO: Implement Jacobian computation for inverse kinematics
        Vec::new()
    }
}

/// Extension trait to add FK methods directly to Robot
pub trait ForwardKinematicsMethods {
    fn update_forward_kinematics(&mut self);
    fn get_link_world_transform(&self, link_name: &str) -> Option<glam::Mat4>;
    fn get_link_world_position(&self, link_name: &str) -> Option<glam::Vec3>;
}

impl ForwardKinematicsMethods for Robot {
    fn update_forward_kinematics(&mut self) {
        ForwardKinematics::update(self);
    }

    fn get_link_world_transform(&self, link_name: &str) -> Option<glam::Mat4> {
        ForwardKinematics::get_link_transform(self, link_name)
    }

    fn get_link_world_position(&self, link_name: &str) -> Option<glam::Vec3> {
        ForwardKinematics::get_link_position(self, link_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::robot::model::{Robot, RobotLink, RobotJoint, JointType};

    fn create_simple_robot() -> Robot {
        let mut robot = Robot::new("test_robot".to_string());

        // Add two links
        robot.links.push(RobotLink::new("base".to_string()));
        robot.links.push(RobotLink::new("link1".to_string()));

        robot.link_map.insert("base".to_string(), 0);
        robot.link_map.insert("link1".to_string(), 1);

        // Add a revolute joint
        let mut joint = RobotJoint::new(
            "joint1".to_string(),
            "base".to_string(),
            "link1".to_string(),
        );
        joint.joint_type = JointType::Revolute;
        joint.origin_xyz = glam::Vec3::new(0.0, 0.0, 1.0);
        joint.axis = glam::Vec3::Y;
        robot.joints.push(joint);

        robot.root_link = "base".to_string();
        robot.link_transforms = vec![glam::Mat4::IDENTITY; 2];

        robot
    }

    #[test]
    fn test_fk_identity_at_zero() {
        let mut robot = create_simple_robot();
        robot.joints[0].angle = 0.0;

        ForwardKinematics::update(&mut robot);

        // Base should be at identity
        let base_transform = robot.link_transforms[0];
        assert!((base_transform - glam::Mat4::IDENTITY).abs_diff_eq(glam::Mat4::ZERO, 0.001));

        // Link1 should be translated by (0, 0, 1)
        let link1_pos = ForwardKinematics::get_link_position(&robot, "link1").unwrap();
        assert!((link1_pos.z - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_fk_rotation() {
        let mut robot = create_simple_robot();
        robot.joints[0].angle = std::f32::consts::FRAC_PI_2; // 90 degrees

        ForwardKinematics::update(&mut robot);

        // The link1 position is at the joint origin (0, 0, 1)
        // Rotating the joint changes orientation, not the link frame position
        let link1_pos = ForwardKinematics::get_link_position(&robot, "link1").unwrap();
        assert!((link1_pos.z - 1.0).abs() < 0.01);

        // Verify the orientation changed by checking the rotation
        let link1_rot = ForwardKinematics::get_link_orientation(&robot, "link1").unwrap();
        let expected_rot = glam::Quat::from_rotation_y(std::f32::consts::FRAC_PI_2);
        assert!(link1_rot.dot(expected_rot).abs() > 0.99);
    }
}
