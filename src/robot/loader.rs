//! URDF robot loader
//!
//! Parses URDF files and loads STL meshes to create Robot instances.

use std::collections::HashSet;
use std::path::Path;

use crate::error::{RobotError, RobotWarning};

/// In-memory assets (wasm builds embed URDFs + STLs; keyed by file basename).
static VIRTUAL_ASSETS: std::sync::OnceLock<
    std::collections::HashMap<&'static str, &'static [u8]>,
> = std::sync::OnceLock::new();

/// Register embedded assets; call once at startup before any load.
pub fn set_virtual_assets(
    map: std::collections::HashMap<&'static str, &'static [u8]>,
) {
    let _ = VIRTUAL_ASSETS.set(map);
}

fn virtual_asset<P: AsRef<Path>>(path: P) -> Option<&'static [u8]> {
    let base = path.as_ref().file_name()?.to_str()?;
    VIRTUAL_ASSETS.get()?.get(base).copied()
}
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
    if let Some(bytes) = virtual_asset(urdf_path) {
        let content = std::str::from_utf8(bytes)
            .map_err(|e| RobotError::UrdfParseError(e.to_string()))?;
        return load_robot_from_string(content, assets_base);
    }
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

            // Primitives are built procedurally; only meshes hit the disk.
            let primitive = match &visual.geometry {
                urdf_rs::Geometry::Box { size } => Some(MeshData::urdf_box(
                    size[0] as f32,
                    size[1] as f32,
                    size[2] as f32,
                )),
                urdf_rs::Geometry::Cylinder { radius, length } => Some(
                    MeshData::urdf_cylinder(*radius as f32, *length as f32, 32),
                ),
                urdf_rs::Geometry::Sphere { radius } => {
                    Some(MeshData::urdf_sphere(*radius as f32, 16, 32))
                }
                // urdf-rs also models Capsule; approximate it with a cylinder
                // rather than dropping the link entirely.
                urdf_rs::Geometry::Capsule { radius, length } => Some(
                    MeshData::urdf_cylinder(*radius as f32, *length as f32, 32),
                ),
                urdf_rs::Geometry::Mesh { .. } => None,
            };
            if let Some(mut mesh) = primitive {
                apply_visual_origin(&mut mesh, visual);
                let b = (
                    glam::Vec3::from(mesh.bounds_min),
                    glam::Vec3::from(mesh.bounds_max),
                );
                for i in 0..3 {
                    bounds_min[i] = bounds_min[i].min(b.0[i]);
                    bounds_max[i] = bounds_max[i].max(b.1[i]);
                }
                link_meshes.push(mesh);
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
                    Err(e) => {
                        warnings.push(RobotWarning::MissingMesh {
                            link_name: link.name.clone(),
                            path: filename.clone(),
                        });
                        eprintln!("Failed to load mesh {}: {}", filename, e);
                        // Fallback to a small red cube for visibility
                        let mut cube = MeshData::test_cube(0.05);
                        cube.make_double_sided();
                        link_meshes.push(cube);
                        // Mark as red in the link color if not already set
                        if robot_link.color.is_none() {
                            robot_link.color = Some([1.0, 0.0, 0.0, 1.0]);
                        }
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
/// A loadable model found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelFile {
    /// Full path to open with [`load_any`].
    pub path: std::path::PathBuf,
    /// File name, for showing in a list.
    pub name: String,
    /// Path relative to the scanned folder, for disambiguating duplicates.
    pub relative: String,
    pub kind: ModelKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelKind {
    /// A `.urdf` robot description.
    Urdf,
    /// A bare mesh (`.stl` / `.obj`) with no articulation.
    Mesh,
}

/// Find everything in `dir` this crate can open: `.urdf`, `.stl` and `.obj`.
///
/// Walks up to `max_depth` levels (0 = just `dir` itself). Meshes that sit in
/// a folder alongside a URDF are almost always that robot's parts rather than
/// standalone models, so they are omitted — otherwise opening a robot folder
/// buries the one URDF under fifty STLs. Results are sorted URDFs first, then
/// by name.
pub fn scan_folder(dir: impl AsRef<Path>, max_depth: usize) -> Vec<ModelFile> {
    let root = dir.as_ref();
    let mut out = Vec::new();
    let mut dirs_with_urdf = std::collections::HashSet::new();
    walk(root, root, 0, max_depth, &mut out, &mut dirs_with_urdf);

    // Hide a robot's own part meshes so the URDF is not buried under them —
    // but only where they clearly belong to it: in the same directory, or in
    // a conventionally named subdirectory of one. A blanket "anywhere below a
    // URDF" rule is too greedy: one URDF at the top of the scanned folder
    // would hide every mesh in every unrelated sibling directory.
    const PART_DIRS: [&str; 4] = ["meshes", "mesh", "visual", "collision"];
    out.retain(|m| {
        if m.kind == ModelKind::Urdf {
            return true;
        }
        let Some(parent) = m.path.parent() else { return true };
        if dirs_with_urdf.contains(parent) {
            return false; // sits right next to a URDF
        }
        let named_like_parts = parent
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| PART_DIRS.contains(&n.to_lowercase().as_str()))
            .unwrap_or(false);
        if named_like_parts {
            if let Some(grandparent) = parent.parent() {
                if dirs_with_urdf.contains(grandparent) {
                    return false; // robot/meshes/*.stl next to robot/*.urdf
                }
            }
        }
        true
    });

    out.sort_by(|a, b| {
        (a.kind != ModelKind::Urdf)
            .cmp(&(b.kind != ModelKind::Urdf))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

fn walk(
    root: &Path,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    out: &mut Vec<ModelFile>,
    dirs_with_urdf: &mut std::collections::HashSet<std::path::PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // skip the usual noise
            let skip = path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with('.') || n == "target" || n == "node_modules")
                .unwrap_or(false);
            if !skip {
                subdirs.push(path);
            }
            continue;
        }
        let kind = match extension_of(&path.to_string_lossy()).as_deref() {
            Some("urdf") => ModelKind::Urdf,
            Some("stl") | Some("obj") => ModelKind::Mesh,
            _ => continue,
        };
        if kind == ModelKind::Urdf {
            if let Some(parent) = path.parent() {
                dirs_with_urdf.insert(parent.to_path_buf());
            }
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .to_string();
        out.push(ModelFile { path, name, relative, kind });
    }
    if depth < max_depth {
        for sub in subdirs {
            walk(root, &sub, depth + 1, max_depth, out, dirs_with_urdf);
        }
    }
}

/// Open a `.urdf`, `.stl` or `.obj` by path, choosing by extension.
///
/// A bare mesh becomes a single fixed link, so the viewer can show mesh files
/// with no URDF at all. Assets resolve against the file's own folder.
pub fn load_any(path: impl AsRef<Path>) -> LoadResult<Robot> {
    let path = path.as_ref();
    let assets = path
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());
    match extension_of(&path.to_string_lossy()).as_deref() {
        Some("stl") | Some("obj") => load_mesh_as_robot(path),
        _ => load_robot(path, &assets),
    }
}

/// Wrap a single mesh file as a one-link robot.
pub fn load_mesh_as_robot(path: impl AsRef<Path>) -> LoadResult<Robot> {
    let path = path.as_ref();
    let name = path
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("mesh")
        .to_string();
    let mut mesh = mesh_from_path(&path.to_string_lossy()).map_err(|reason| {
        RobotError::MeshLoadError {
            path: path.to_path_buf(),
            reason,
        }
    })?;
    mesh.make_double_sided();

    let mut robot = Robot::new(name.clone());
    let mut link = RobotLink::new(name.clone());
    link.mesh_data = Some(mesh);
    robot.link_map.insert(name.clone(), 0);
    robot.root_link = name;
    robot.links.push(link);
    robot.link_transforms = vec![glam::Mat4::IDENTITY];
    Ok(robot)
}

/// `package://pkg/rel/path.stl` -> `rel/path.stl`, `file:///a/b.stl` -> `/a/b.stl`.
/// Anything else is returned unchanged.
///
/// The package name is dropped rather than resolved: without a ROS
/// environment there is nowhere to look it up, and the caller's assets
/// directory is the stand-in for the package root.
fn strip_uri_scheme(filename: &str) -> &str {
    if let Some(rest) = filename.strip_prefix("package://") {
        // drop the package name, keep the path inside it
        return rest.split_once('/').map(|(_pkg, path)| path).unwrap_or(rest);
    }
    if let Some(rest) = filename.strip_prefix("model://") {
        return rest.split_once('/').map(|(_pkg, path)| path).unwrap_or(rest);
    }
    if let Some(rest) = filename.strip_prefix("file://") {
        return rest;
    }
    filename
}

/// Pick a mesh parser by file extension. STL (binary or ASCII) and OBJ are
/// supported; COLLADA/.dae is not, and reports that plainly instead of
/// failing as a corrupt STL.
fn mesh_from_path(path: &str) -> Result<MeshData, String> {
    match extension_of(path).as_deref() {
        Some("obj") => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| format!("Failed to open OBJ file: {}", e))?;
            MeshData::from_obj_str(&text)
        }
        Some("dae") => Err(format!(
            "COLLADA (.dae) meshes are not supported; convert {} to STL or OBJ",
            path
        )),
        _ => MeshData::from_stl(path),
    }
}

fn mesh_from_bytes(name: &str, bytes: &[u8]) -> Result<MeshData, String> {
    match extension_of(name).as_deref() {
        Some("obj") => {
            let text = std::str::from_utf8(bytes).map_err(|e| format!("OBJ not utf8: {}", e))?;
            MeshData::from_obj_str(text)
        }
        Some("dae") => Err(format!(
            "COLLADA (.dae) meshes are not supported; convert {} to STL or OBJ",
            name
        )),
        _ => MeshData::from_stl_bytes(bytes),
    }
}

fn extension_of(path: &str) -> Option<String> {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
}

/// Place a visual's geometry by its `<origin>`. Applies to procedural
/// primitives and loaded meshes alike, so both land in the same place.
fn apply_visual_origin(mesh: &mut MeshData, visual: &urdf_rs::Visual) {
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
        mesh.apply_transform(&vis_transform.to_cols_array());
    }
}

fn load_mesh_for_visual(
    filename: &str,
    scale: &Option<urdf_rs::Vec3>,
    visual: &urdf_rs::Visual,
    assets_base: &str,
) -> Result<(MeshData, (glam::Vec3, glam::Vec3)), String> {
    // ROS URDFs address meshes as package://<pkg>/rel/path.stl (and some use
    // file://). Strip the scheme so the remainder can be resolved against the
    // caller's assets directory.
    let relative = strip_uri_scheme(filename);

    let full_path = format!("{}/{}", assets_base, relative);
    let filename_only = Path::new(relative)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(relative);
    let fallback_path = format!("{}/{}", assets_base, filename_only);

    let mesh_path = if Path::new(&full_path).exists() {
        full_path
    } else {
        fallback_path
    };

    let mut mesh = match virtual_asset(filename_only) {
        Some(bytes) => mesh_from_bytes(filename_only, bytes)?,
        None => mesh_from_path(&mesh_path)?,
    };

    // Debug: print mesh bounds before scaling
    eprintln!("=== Mesh {}: bounds before scale: {:?} to {:?}", filename, mesh.bounds_min, mesh.bounds_max);

    // Apply mesh scale if specified in URDF
    if let Some(s) = scale {
        let scale_x = s.0[0] as f32;
        let scale_y = s.0[1] as f32;
        let scale_z = s.0[2] as f32;
        let uniform_scale = (scale_x + scale_y + scale_z) / 3.0;
        eprintln!("=== Applying scale: {} (from {:?})", uniform_scale, s);
        if (uniform_scale - 1.0).abs() > 0.001 {
            mesh.apply_scale(uniform_scale);
            eprintln!("=== Mesh {}: bounds after scale: {:?} to {:?}", filename, mesh.bounds_min, mesh.bounds_max);
        }
    }

    apply_visual_origin(&mut mesh, visual);

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
