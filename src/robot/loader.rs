//! URDF robot loader
//!
//! Parses URDF files and loads STL meshes to create Robot instances.

use std::collections::HashSet;
use std::path::Path;

use crate::error::{RobotError, RobotWarning};
use crate::mesh::MeshData;
use super::model::{Robot, RobotLink, RobotJoint, JointType};

/// Result type for robot loading operations
pub type LoadResult<T> = Result<T, RobotError>;

/// Load a robot from a URDF file
///
/// # Arguments
/// * `urdf_path` - Path to the URDF file
/// * `assets_base` - Base directory for mesh files (STL)
///
/// # Returns
/// A Result containing the loaded Robot or a RobotError
pub fn load_robot<P: AsRef<Path>>(urdf_path: P, assets_base: &str) -> LoadResult<Robot> {
    let urdf_path = urdf_path.as_ref();

    // Read URDF file
    let urdf_content = std::fs::read_to_string(urdf_path)
        .map_err(|e| RobotError::UrdfReadError {
            path: urdf_path.to_path_buf(),
            source: e,
        })?;

    // Parse URDF
    let urdf = urdf_rs::read_from_string(&urdf_content)
        .map_err(|e| RobotError::UrdfParseError(e.to_string()))?;

    // Build robot from parsed URDF
    build_robot_from_urdf(&urdf, assets_base)
}

/// Load a robot from URDF content string
pub fn load_robot_from_string(urdf_content: &str, assets_base: &str) -> LoadResult<Robot> {
    let urdf = urdf_rs::read_from_string(urdf_content)
        .map_err(|e| RobotError::UrdfParseError(e.to_string()))?;

    build_robot_from_urdf(&urdf, assets_base)
}

/// Build a Robot struct from parsed URDF
fn build_robot_from_urdf(urdf: &urdf_rs::Robot, assets_base: &str) -> LoadResult<Robot> {
    let mut robot = Robot::new(urdf.name.clone());
    let mut warnings: Vec<RobotWarning> = Vec::new();

    let mut bounds_min = glam::Vec3::splat(f32::MAX);
    let mut bounds_max = glam::Vec3::splat(f32::MIN);

    // Parse links
    for (idx, link) in urdf.links.iter().enumerate() {
        let mut robot_link = RobotLink::new(link.name.clone());

        let mut link_meshes = Vec::new();
        for visual in &link.visual {
            // Extract material color if present
            if robot_link.color.is_none() {
                if let Some(ref material) = visual.material {
                    if let Some(ref color) = material.color {
                        robot_link.color = Some([
                            color.rgba[0] as f32,
                            color.rgba[1] as f32,
                            color.rgba[2] as f32,
                            color.rgba[3] as f32,
                        ]);
                    }
                }
            }

            if let urdf_rs::Geometry::Mesh { filename, scale } = &visual.geometry {
                match load_mesh_for_visual(filename, scale, visual, assets_base) {
                    Ok((mesh, updated_bounds)) => {
                        // Update global bounds
                        for i in 0..3 {
                            bounds_min[i] = bounds_min[i].min(updated_bounds.0[i]);
                            bounds_max[i] = bounds_max[i].max(updated_bounds.1[i]);
                        }
                        link_meshes.push(mesh);
                    }
                    Err(_) => {
                        warnings.push(RobotWarning::MissingMesh {
                            link_name: link.name.clone(),
                            path: filename.clone(),
                        });
                    }
                }
            }
        }

        if !link_meshes.is_empty() {
            let mut combined = MeshData::combine(link_meshes);
            combined.make_double_sided();
            robot_link.mesh_data = Some(combined);
        }

        robot.link_map.insert(link.name.clone(), idx);
        robot.links.push(robot_link);
    }

    // Compute scale and center from bounds
    let center = (bounds_min + bounds_max) * 0.5;
    robot.scale = 1.0;
    robot.center = center;

    // Parse joints
    let mut child_links: HashSet<String> = HashSet::new();

    for joint in urdf.joints.iter() {
        let robot_joint = parse_joint(joint, &mut warnings);
        child_links.insert(joint.child.link.clone());
        robot.joints.push(robot_joint);
    }

    // Find root link (link that is not a child of any joint)
    for link in &robot.links {
        if !child_links.contains(&link.name) {
            robot.root_link = link.name.clone();
            break;
        }
    }

    if robot.root_link.is_empty() {
        return Err(RobotError::NoRootLink);
    }

    // Validate joint references
    for joint in &robot.joints {
        if !robot.link_map.contains_key(&joint.parent_link) {
            return Err(RobotError::InvalidJointReference {
                joint_name: joint.name.clone(),
                missing_link: joint.parent_link.clone(),
            });
        }
        if !robot.link_map.contains_key(&joint.child_link) {
            return Err(RobotError::InvalidJointReference {
                joint_name: joint.name.clone(),
                missing_link: joint.child_link.clone(),
            });
        }
    }

    // Check for cycles in the kinematic chain
    if let Some(cycle) = detect_cycle(&robot) {
        return Err(RobotError::CycleDetected { joints: cycle });
    }

    // Initialize link transforms
    robot.link_transforms = vec![glam::Mat4::IDENTITY; robot.links.len()];

    // Log warnings
    for warning in &warnings {
        eprintln!("Warning: {}", warning);
    }

    // Log summary
    let links_with_mesh = robot.links.iter().filter(|l| l.has_mesh()).count();
    eprintln!(
        "=== Robot: {} links, {} with meshes, {} joints",
        robot.links.len(),
        links_with_mesh,
        robot.joints.len()
    );
    eprintln!("=== Root link: {}", robot.root_link);

    Ok(robot)
}

/// Load mesh for a visual element
fn load_mesh_for_visual(
    filename: &str,
    scale: &Option<urdf_rs::Vec3>,
    visual: &urdf_rs::Visual,
    assets_base: &str,
) -> Result<(MeshData, (glam::Vec3, glam::Vec3)), String> {
    // Try full relative path first, then fall back to just filename
    let full_path = format!("{}/{}", assets_base, filename);
    let filename_only = Path::new(filename)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(filename);
    let fallback_path = format!("{}/{}", assets_base, filename_only);

    let mesh_path = if Path::new(&full_path).exists() {
        full_path
    } else {
        fallback_path
    };

    let mut mesh = MeshData::from_stl(&mesh_path)?;

    // Apply mesh scale if specified in URDF
    if let Some(s) = scale {
        let scale_x = s.0[0] as f32;
        let scale_y = s.0[1] as f32;
        let scale_z = s.0[2] as f32;
        let uniform_scale = (scale_x + scale_y + scale_z) / 3.0;
        if (uniform_scale - 1.0).abs() > 0.001 {
            mesh.apply_scale(uniform_scale);
        }
    }

    // Apply visual origin transform
    let vis_xyz = glam::Vec3::new(
        visual.origin.xyz.0[0] as f32,
        visual.origin.xyz.0[1] as f32,
        visual.origin.xyz.0[2] as f32,
    );
    let vis_rpy = glam::Vec3::new(
        visual.origin.rpy.0[0] as f32,
        visual.origin.rpy.0[1] as f32,
        visual.origin.rpy.0[2] as f32,
    );

    if vis_xyz != glam::Vec3::ZERO || vis_rpy != glam::Vec3::ZERO {
        // URDF RPY is extrinsic XYZ (roll, pitch, yaw)
        // = Rz(yaw) * Ry(pitch) * Rx(roll)
        // glam intrinsic ZYX with (yaw, pitch, roll) gives the same result
        let vis_rot = glam::Quat::from_euler(glam::EulerRot::ZYX, vis_rpy.z, vis_rpy.y, vis_rpy.x);
        let vis_transform = glam::Mat4::from_rotation_translation(vis_rot, vis_xyz);
        let cols = vis_transform.to_cols_array();
        let makepad_transform = makepad_widgets::Mat4 { v: cols };
        mesh.apply_transform(&makepad_transform);
    }

    let bounds = (
        glam::Vec3::from_array(mesh.bounds_min),
        glam::Vec3::from_array(mesh.bounds_max),
    );

    Ok((mesh, bounds))
}

/// Parse a URDF joint into RobotJoint
fn parse_joint(joint: &urdf_rs::Joint, warnings: &mut Vec<RobotWarning>) -> RobotJoint {
    let origin_xyz = glam::Vec3::new(
        joint.origin.xyz.0[0] as f32,
        joint.origin.xyz.0[1] as f32,
        joint.origin.xyz.0[2] as f32,
    );

    let origin_rpy = glam::Vec3::new(
        joint.origin.rpy.0[0] as f32,
        joint.origin.rpy.0[1] as f32,
        joint.origin.rpy.0[2] as f32,
    );

    let axis = glam::Vec3::new(
        joint.axis.xyz[0] as f32,
        joint.axis.xyz[1] as f32,
        joint.axis.xyz[2] as f32,
    )
    .normalize();

    // Parse joint type
    let joint_type = match joint.joint_type {
        urdf_rs::JointType::Fixed => JointType::Fixed,
        urdf_rs::JointType::Revolute => JointType::Revolute,
        urdf_rs::JointType::Continuous => JointType::Continuous,
        urdf_rs::JointType::Prismatic => {
            warnings.push(RobotWarning::UnsupportedJointType {
                joint_name: joint.name.clone(),
                joint_type: "prismatic".to_string(),
            });
            JointType::Prismatic
        }
        _ => {
            warnings.push(RobotWarning::UnsupportedJointType {
                joint_name: joint.name.clone(),
                joint_type: format!("{:?}", joint.joint_type),
            });
            JointType::Fixed
        }
    };

    let mut robot_joint = RobotJoint::new(
        joint.name.clone(),
        joint.parent.link.clone(),
        joint.child.link.clone(),
    );

    robot_joint.origin_xyz = origin_xyz;
    robot_joint.origin_rpy = origin_rpy;
    robot_joint.axis = axis;
    robot_joint.joint_type = joint_type;
    robot_joint.limit_lower = joint.limit.lower as f32;
    robot_joint.limit_upper = joint.limit.upper as f32;

    // Validate limits
    if robot_joint.limit_lower > robot_joint.limit_upper {
        warnings.push(RobotWarning::UnusualJointLimits {
            joint_name: joint.name.clone(),
            lower: robot_joint.limit_lower,
            upper: robot_joint.limit_upper,
        });
    }

    robot_joint
}

/// Detect cycles in the kinematic chain using DFS
fn detect_cycle(robot: &Robot) -> Option<Vec<String>> {
    use std::collections::HashMap;

    // Build adjacency list: parent_link -> [(joint_name, child_link)]
    let mut adjacency: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();
    for joint in &robot.joints {
        adjacency
            .entry(&joint.parent_link)
            .or_default()
            .push((&joint.name, &joint.child_link));
    }

    // DFS to detect cycles
    let mut visited: HashSet<&str> = HashSet::new();
    let mut in_stack: HashSet<&str> = HashSet::new();
    let mut path: Vec<String> = Vec::new();

    fn dfs<'a>(
        node: &'a str,
        adjacency: &HashMap<&'a str, Vec<(&'a str, &'a str)>>,
        visited: &mut HashSet<&'a str>,
        in_stack: &mut HashSet<&'a str>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        visited.insert(node);
        in_stack.insert(node);

        if let Some(neighbors) = adjacency.get(node) {
            for (joint_name, child) in neighbors {
                path.push(joint_name.to_string());

                if !visited.contains(child) {
                    if let Some(cycle) = dfs(child, adjacency, visited, in_stack, path) {
                        return Some(cycle);
                    }
                } else if in_stack.contains(child) {
                    // Found a cycle - return the path
                    return Some(path.clone());
                }

                path.pop();
            }
        }

        in_stack.remove(node);
        None
    }

    // Start DFS from root link
    if !robot.root_link.is_empty() {
        if let Some(cycle) = dfs(
            &robot.root_link,
            &adjacency,
            &mut visited,
            &mut in_stack,
            &mut path,
        ) {
            return Some(cycle);
        }
    }

    // Also check any disconnected components
    for link in &robot.links {
        if !visited.contains(link.name.as_str()) {
            if let Some(cycle) = dfs(
                &link.name,
                &adjacency,
                &mut visited,
                &mut in_stack,
                &mut path,
            ) {
                return Some(cycle);
            }
        }
    }

    None
}

/// Validate a robot structure and return any issues found
pub fn validate_robot(robot: &Robot) -> Vec<RobotWarning> {
    let mut warnings = Vec::new();

    // Check for links without meshes
    for link in &robot.links {
        if !link.has_mesh() {
            warnings.push(RobotWarning::MissingMesh {
                link_name: link.name.clone(),
                path: "no visual geometry".to_string(),
            });
        }
    }

    // Check for unusual joint limits
    for joint in &robot.joints {
        if joint.limit_lower > joint.limit_upper {
            warnings.push(RobotWarning::UnusualJointLimits {
                joint_name: joint.name.clone(),
                lower: joint.limit_lower,
                upper: joint.limit_upper,
            });
        }

        // Check for extremely large limits (> 2*PI for revolute)
        if matches!(joint.joint_type, JointType::Revolute) {
            let range = joint.limit_upper - joint.limit_lower;
            if range > std::f32::consts::TAU * 2.0 {
                warnings.push(RobotWarning::UnusualJointLimits {
                    joint_name: joint.name.clone(),
                    lower: joint.limit_lower,
                    upper: joint.limit_upper,
                });
            }
        }
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_invalid_path() {
        let result = load_robot("/nonexistent/path.urdf", "/assets");
        assert!(result.is_err());
        match result {
            Err(RobotError::UrdfReadError { .. }) => {}
            _ => panic!("Expected UrdfReadError"),
        }
    }

    #[test]
    fn test_detect_cycle_no_cycle() {
        let mut robot = Robot::new("test".to_string());
        robot.links.push(RobotLink::new("base".to_string()));
        robot.links.push(RobotLink::new("link1".to_string()));
        robot.links.push(RobotLink::new("link2".to_string()));
        robot.link_map.insert("base".to_string(), 0);
        robot.link_map.insert("link1".to_string(), 1);
        robot.link_map.insert("link2".to_string(), 2);

        robot.joints.push(RobotJoint::new(
            "j1".to_string(),
            "base".to_string(),
            "link1".to_string(),
        ));
        robot.joints.push(RobotJoint::new(
            "j2".to_string(),
            "link1".to_string(),
            "link2".to_string(),
        ));
        robot.root_link = "base".to_string();

        assert!(detect_cycle(&robot).is_none());
    }

    #[test]
    fn test_detect_cycle_with_cycle() {
        let mut robot = Robot::new("test".to_string());
        robot.links.push(RobotLink::new("a".to_string()));
        robot.links.push(RobotLink::new("b".to_string()));
        robot.links.push(RobotLink::new("c".to_string()));
        robot.link_map.insert("a".to_string(), 0);
        robot.link_map.insert("b".to_string(), 1);
        robot.link_map.insert("c".to_string(), 2);

        // Create a cycle: a -> b -> c -> a
        robot.joints.push(RobotJoint::new(
            "j1".to_string(),
            "a".to_string(),
            "b".to_string(),
        ));
        robot.joints.push(RobotJoint::new(
            "j2".to_string(),
            "b".to_string(),
            "c".to_string(),
        ));
        robot.joints.push(RobotJoint::new(
            "j3".to_string(),
            "c".to_string(),
            "a".to_string(),
        ));
        robot.root_link = "a".to_string();

        let cycle = detect_cycle(&robot);
        assert!(cycle.is_some());
    }

    #[test]
    fn test_validate_robot_joint_limits() {
        let mut robot = Robot::new("test".to_string());
        robot.links.push(RobotLink::new("base".to_string()));

        let mut joint = RobotJoint::new(
            "bad_joint".to_string(),
            "base".to_string(),
            "base".to_string(),
        );
        joint.joint_type = JointType::Revolute;
        joint.limit_lower = 1.0;
        joint.limit_upper = -1.0; // Invalid: lower > upper
        robot.joints.push(joint);

        let warnings = validate_robot(&robot);
        assert!(!warnings.is_empty());
        assert!(warnings.iter().any(|w| matches!(w, RobotWarning::UnusualJointLimits { .. })));
    }
}
