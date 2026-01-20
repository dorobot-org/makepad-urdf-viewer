//! Robot data model structures

use std::collections::HashMap;
use crate::mesh::MeshData;

/// A link in the robot (rigid body)
#[derive(Clone, Debug)]
pub struct RobotLink {
    /// Link name from URDF
    pub name: String,
    /// Visual mesh data (if any)
    pub mesh_data: Option<MeshData>,
    /// RGBA color from URDF material
    pub color: Option<[f32; 4]>,
}

impl RobotLink {
    /// Create a new link with just a name
    pub fn new(name: String) -> Self {
        Self {
            name,
            mesh_data: None,
            color: None,
        }
    }

    /// Check if this link has visual geometry
    pub fn has_mesh(&self) -> bool {
        self.mesh_data.is_some()
    }
}

/// Joint types supported by URDF
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum JointType {
    /// Fixed joint - no movement
    #[default]
    Fixed,
    /// Revolute joint - rotation with limits
    Revolute,
    /// Continuous joint - unlimited rotation
    Continuous,
    /// Prismatic joint - linear movement (TODO)
    Prismatic,
}

/// A joint connecting two links
#[derive(Clone, Debug)]
pub struct RobotJoint {
    /// Joint name from URDF
    pub name: String,
    /// Parent link name
    pub parent_link: String,
    /// Child link name
    pub child_link: String,
    /// Position offset from parent
    pub origin_xyz: glam::Vec3,
    /// Rotation offset (roll, pitch, yaw)
    pub origin_rpy: glam::Vec3,
    /// Rotation axis (for revolute/continuous)
    pub axis: glam::Vec3,
    /// Current joint angle
    pub angle: f32,
    /// Lower limit (radians)
    pub limit_lower: f32,
    /// Upper limit (radians)
    pub limit_upper: f32,
    /// Joint type
    pub joint_type: JointType,
}

impl RobotJoint {
    /// Create a new joint
    pub fn new(name: String, parent: String, child: String) -> Self {
        Self {
            name,
            parent_link: parent,
            child_link: child,
            origin_xyz: glam::Vec3::ZERO,
            origin_rpy: glam::Vec3::ZERO,
            axis: glam::Vec3::Y,
            angle: 0.0,
            limit_lower: -std::f32::consts::PI,
            limit_upper: std::f32::consts::PI,
            joint_type: JointType::Fixed,
        }
    }

    /// Check if this joint is movable
    pub fn is_movable(&self) -> bool {
        matches!(self.joint_type, JointType::Revolute | JointType::Continuous)
    }

    /// Set angle with limit clamping
    pub fn set_angle(&mut self, angle: f32) {
        match self.joint_type {
            JointType::Continuous => {
                // No limits for continuous joints
                self.angle = angle;
            }
            JointType::Revolute => {
                self.angle = angle.clamp(self.limit_lower, self.limit_upper);
            }
            _ => {
                // Fixed and other joints don't move
            }
        }
    }

    /// Set angle without clamping (for episode playback with real recorded data)
    pub fn set_angle_unclamped(&mut self, angle: f32) {
        if self.is_movable() {
            self.angle = angle;
        }
    }
}

/// Robot structure with forward kinematics
#[derive(Clone)]
pub struct Robot {
    /// Robot name from URDF
    pub name: String,
    /// All links in the robot
    pub links: Vec<RobotLink>,
    /// All joints in the robot
    pub joints: Vec<RobotJoint>,
    /// Map from link name to index
    pub link_map: HashMap<String, usize>,
    /// Name of the root link
    pub root_link: String,
    /// World transforms for each link (computed by FK)
    pub link_transforms: Vec<glam::Mat4>,
    /// Scale factor for visualization
    pub scale: f32,
    /// Center point of the robot
    pub center: glam::Vec3,
}

impl Robot {
    /// Create a new empty robot
    pub fn new(name: String) -> Self {
        Self {
            name,
            links: Vec::new(),
            joints: Vec::new(),
            link_map: HashMap::new(),
            root_link: String::new(),
            link_transforms: Vec::new(),
            scale: 1.0,
            center: glam::Vec3::ZERO,
        }
    }

    /// Get number of joints
    pub fn num_joints(&self) -> usize {
        self.joints.len()
    }

    /// Get number of movable joints
    pub fn num_movable_joints(&self) -> usize {
        self.joints.iter().filter(|j| j.is_movable()).count()
    }

    /// Get number of links
    pub fn num_links(&self) -> usize {
        self.links.len()
    }

    /// Get number of links with meshes
    pub fn num_visual_links(&self) -> usize {
        self.links.iter().filter(|l| l.has_mesh()).count()
    }

    /// Set a joint angle by index
    pub fn set_joint_angle(&mut self, idx: usize, angle: f32) {
        if let Some(joint) = self.joints.get_mut(idx) {
            joint.set_angle(angle);
        }
    }

    /// Get joint information by index
    pub fn get_joint_info(&self, idx: usize) -> Option<(&str, f32, f32, f32)> {
        self.joints.get(idx).map(|j| {
            (j.name.as_str(), j.angle, j.limit_lower, j.limit_upper)
        })
    }

    /// Get all joint angles
    pub fn get_joint_angles(&self) -> Vec<f32> {
        self.joints.iter().map(|j| j.angle).collect()
    }

    /// Set all joint angles
    pub fn set_joint_angles(&mut self, angles: &[f32]) {
        for (joint, &angle) in self.joints.iter_mut().zip(angles.iter()) {
            joint.set_angle(angle);
        }
    }

    /// Set all joint angles without clamping (for episode playback)
    pub fn set_joint_angles_unclamped(&mut self, angles: &[f32]) {
        for (joint, &angle) in self.joints.iter_mut().zip(angles.iter()) {
            joint.set_angle_unclamped(angle);
        }
    }

    /// Reset all joints to zero
    pub fn reset_joints(&mut self) {
        for joint in &mut self.joints {
            joint.angle = 0.0;
        }
    }

    /// Get link index by name
    pub fn get_link_index(&self, name: &str) -> Option<usize> {
        self.link_map.get(name).copied()
    }

    /// Get link by index
    pub fn get_link(&self, idx: usize) -> Option<&RobotLink> {
        self.links.get(idx)
    }

    /// Get joint by index
    pub fn get_joint(&self, idx: usize) -> Option<&RobotJoint> {
        self.joints.get(idx)
    }

    /// Get mutable joint by index
    pub fn get_joint_mut(&mut self, idx: usize) -> Option<&mut RobotJoint> {
        self.joints.get_mut(idx)
    }

    /// Get transform for a link
    pub fn get_link_transform(&self, idx: usize) -> Option<glam::Mat4> {
        self.link_transforms.get(idx).copied()
    }

    /// Get bounding box of the robot (min, max)
    pub fn bounds(&self) -> (glam::Vec3, glam::Vec3) {
        let mut min = glam::Vec3::splat(f32::MAX);
        let mut max = glam::Vec3::splat(f32::MIN);

        for link in &self.links {
            if let Some(ref mesh) = link.mesh_data {
                for i in 0..3 {
                    min[i] = min[i].min(mesh.bounds_min[i]);
                    max[i] = max[i].max(mesh.bounds_max[i]);
                }
            }
        }

        (min, max)
    }

    /// Get extent (size) of the robot
    pub fn extent(&self) -> glam::Vec3 {
        let (min, max) = self.bounds();
        max - min
    }
}

impl Default for Robot {
    fn default() -> Self {
        Self::new("unnamed".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_robot_link_new() {
        let link = RobotLink::new("test_link".to_string());
        assert_eq!(link.name, "test_link");
        assert!(link.mesh_data.is_none());
        assert!(link.color.is_none());
        assert!(!link.has_mesh());
    }

    #[test]
    fn test_joint_type_default() {
        assert_eq!(JointType::default(), JointType::Fixed);
    }

    #[test]
    fn test_robot_joint_new() {
        let joint = RobotJoint::new(
            "joint1".to_string(),
            "parent".to_string(),
            "child".to_string(),
        );
        assert_eq!(joint.name, "joint1");
        assert_eq!(joint.parent_link, "parent");
        assert_eq!(joint.child_link, "child");
        assert_eq!(joint.angle, 0.0);
        assert_eq!(joint.joint_type, JointType::Fixed);
    }

    #[test]
    fn test_joint_is_movable() {
        let mut joint = RobotJoint::new("j".to_string(), "p".to_string(), "c".to_string());

        joint.joint_type = JointType::Fixed;
        assert!(!joint.is_movable());

        joint.joint_type = JointType::Revolute;
        assert!(joint.is_movable());

        joint.joint_type = JointType::Continuous;
        assert!(joint.is_movable());

        joint.joint_type = JointType::Prismatic;
        assert!(!joint.is_movable());
    }

    #[test]
    fn test_joint_set_angle_revolute() {
        let mut joint = RobotJoint::new("j".to_string(), "p".to_string(), "c".to_string());
        joint.joint_type = JointType::Revolute;
        joint.limit_lower = -1.0;
        joint.limit_upper = 1.0;

        joint.set_angle(0.5);
        assert_eq!(joint.angle, 0.5);

        joint.set_angle(2.0); // Over limit
        assert_eq!(joint.angle, 1.0);

        joint.set_angle(-2.0); // Under limit
        assert_eq!(joint.angle, -1.0);
    }

    #[test]
    fn test_joint_set_angle_continuous() {
        let mut joint = RobotJoint::new("j".to_string(), "p".to_string(), "c".to_string());
        joint.joint_type = JointType::Continuous;

        joint.set_angle(10.0); // No limit for continuous
        assert_eq!(joint.angle, 10.0);
    }

    #[test]
    fn test_joint_set_angle_fixed() {
        let mut joint = RobotJoint::new("j".to_string(), "p".to_string(), "c".to_string());
        joint.joint_type = JointType::Fixed;

        joint.set_angle(1.0);
        assert_eq!(joint.angle, 0.0); // Fixed joints don't move
    }

    #[test]
    fn test_robot_new() {
        let robot = Robot::new("test_robot".to_string());
        assert_eq!(robot.name, "test_robot");
        assert!(robot.links.is_empty());
        assert!(robot.joints.is_empty());
        assert_eq!(robot.scale, 1.0);
    }

    #[test]
    fn test_robot_counts() {
        let mut robot = Robot::new("test".to_string());
        robot.links.push(RobotLink::new("l1".to_string()));
        robot.links.push(RobotLink::new("l2".to_string()));

        let mut j1 = RobotJoint::new("j1".to_string(), "l1".to_string(), "l2".to_string());
        j1.joint_type = JointType::Revolute;
        robot.joints.push(j1);

        let mut j2 = RobotJoint::new("j2".to_string(), "l2".to_string(), "l1".to_string());
        j2.joint_type = JointType::Fixed;
        robot.joints.push(j2);

        assert_eq!(robot.num_links(), 2);
        assert_eq!(robot.num_joints(), 2);
        assert_eq!(robot.num_movable_joints(), 1);
        assert_eq!(robot.num_visual_links(), 0); // No meshes
    }

    #[test]
    fn test_robot_joint_angles() {
        let mut robot = Robot::new("test".to_string());
        let mut j1 = RobotJoint::new("j1".to_string(), "l1".to_string(), "l2".to_string());
        j1.joint_type = JointType::Revolute;
        j1.limit_lower = -1.0;
        j1.limit_upper = 1.0;
        robot.joints.push(j1);

        robot.set_joint_angle(0, 0.5);
        assert_eq!(robot.joints[0].angle, 0.5);

        let angles = robot.get_joint_angles();
        assert_eq!(angles, vec![0.5]);

        robot.set_joint_angles(&[0.75]);
        assert_eq!(robot.joints[0].angle, 0.75);

        robot.reset_joints();
        assert_eq!(robot.joints[0].angle, 0.0);
    }

    #[test]
    fn test_robot_get_joint_info() {
        let mut robot = Robot::new("test".to_string());
        let mut j1 = RobotJoint::new("shoulder".to_string(), "l1".to_string(), "l2".to_string());
        j1.angle = 0.5;
        j1.limit_lower = -1.0;
        j1.limit_upper = 1.0;
        robot.joints.push(j1);

        let info = robot.get_joint_info(0).unwrap();
        assert_eq!(info.0, "shoulder");
        assert_eq!(info.1, 0.5);
        assert_eq!(info.2, -1.0);
        assert_eq!(info.3, 1.0);

        assert!(robot.get_joint_info(99).is_none());
    }

    #[test]
    fn test_robot_link_map() {
        let mut robot = Robot::new("test".to_string());
        robot.links.push(RobotLink::new("base".to_string()));
        robot.links.push(RobotLink::new("arm".to_string()));
        robot.link_map.insert("base".to_string(), 0);
        robot.link_map.insert("arm".to_string(), 1);

        assert_eq!(robot.get_link_index("base"), Some(0));
        assert_eq!(robot.get_link_index("arm"), Some(1));
        assert_eq!(robot.get_link_index("nonexistent"), None);
    }
}
