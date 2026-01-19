//! GPU geometry for 3D mesh rendering
//!
//! Provides GeometryMesh3D which manages GPU buffers for mesh data.

use makepad_widgets::*;
use super::mesh::MeshData;

live_design! {
    pub GeometryMesh3D = {{GeometryMesh3D}} {}
}

/// Counter for unique geometry IDs
static GEOMETRY_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Custom geometry for 3D meshes
///
/// This struct manages GPU geometry buffers for mesh rendering.
/// Each instance has a unique ID to allow multiple geometries with the same type.
#[derive(Live, LiveRegister, Clone)]
pub struct GeometryMesh3D {
    #[rust] pub geometry_ref: Option<GeometryRef>,
    #[rust] pub mesh_data: Option<MeshData>,
    /// Unique ID for this geometry instance
    #[rust] pub instance_id: u64,
}

impl GeometryMesh3D {
    /// Create a new empty instance (for cloning purposes)
    pub fn new_empty() -> Self {
        let id = GEOMETRY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self {
            geometry_ref: None,
            mesh_data: None,
            instance_id: id,
        }
    }

    /// Load mesh from STL file
    pub fn load_stl(&mut self, cx: &mut Cx, path: &str) -> Result<(), String> {
        let mut mesh = MeshData::from_stl(path)?;
        mesh.normalize();
        self.upload_mesh(cx, mesh);
        Ok(())
    }

    /// Load all robot meshes from assets directory
    pub fn load_robot(&mut self, cx: &mut Cx, assets_dir: &str) -> Result<(), String> {
        let mesh = MeshData::load_robot_meshes(assets_dir)?;
        self.upload_mesh(cx, mesh);
        Ok(())
    }

    /// Load test cube
    pub fn load_test_cube(&mut self, cx: &mut Cx, size: f32) {
        let mesh = MeshData::test_cube(size);
        self.upload_mesh(cx, mesh);
    }

    /// Upload pre-built mesh data to GPU
    pub fn upload_mesh_data(&mut self, cx: &mut Cx, mesh: MeshData) {
        self.upload_mesh(cx, mesh);
    }

    /// Upload mesh data to GPU with unique fingerprint
    fn upload_mesh(&mut self, cx: &mut Cx, mesh: MeshData) {
        // Create geometry reference with unique fingerprint if not exists
        if self.geometry_ref.is_none() {
            let mut fp = GeometryFingerprint::new(LiveType::of::<Self>());
            fp.push(self.instance_id as f32);
            self.geometry_ref = Some(cx.get_geometry_ref(fp));
        }

        // Upload geometry data
        if let Some(ref gr) = self.geometry_ref {
            gr.0.update(cx, mesh.indices.clone(), mesh.vertices.clone());
        }

        self.mesh_data = Some(mesh);
    }

    /// Check if geometry has been uploaded
    pub fn is_uploaded(&self) -> bool {
        self.geometry_ref.is_some() && self.mesh_data.is_some()
    }

    /// Get the mesh data if available
    pub fn get_mesh_data(&self) -> Option<&MeshData> {
        self.mesh_data.as_ref()
    }
}

impl LiveHook for GeometryMesh3D {
    fn after_apply(&mut self, cx: &mut Cx, _apply: &mut Apply, _index: usize, _nodes: &[LiveNode]) {
        // Initialize with a default cube for shader compilation
        if self.geometry_ref.is_none() {
            let mesh = MeshData::test_cube(1.0);

            let mut fp = GeometryFingerprint::new(LiveType::of::<Self>());
            fp.push(self.instance_id as f32);

            self.geometry_ref = Some(cx.get_geometry_ref(fp));

            if let Some(ref gr) = self.geometry_ref {
                gr.0.update(cx, mesh.indices.clone(), mesh.vertices.clone());
            }

            self.mesh_data = Some(mesh);
        }
    }
}

impl GeometryFields for GeometryMesh3D {
    fn geometry_fields(&self, fields: &mut Vec<GeometryField>) {
        fields.push(GeometryField {
            id: live_id!(geom_pos),
            ty: ShaderTy::Vec3,
        });
        fields.push(GeometryField {
            id: live_id!(geom_id),
            ty: ShaderTy::Float,
        });
        fields.push(GeometryField {
            id: live_id!(geom_normal),
            ty: ShaderTy::Vec3,
        });
        fields.push(GeometryField {
            id: live_id!(geom_uv),
            ty: ShaderTy::Vec2,
        });
    }

    fn get_geometry_id(&self) -> Option<GeometryId> {
        self.geometry_ref.as_ref().map(|gr| gr.0.geometry_id())
    }

    fn live_type_check(&self) -> LiveType {
        LiveType::of::<Self>()
    }
}

impl Default for GeometryMesh3D {
    fn default() -> Self {
        let id = GEOMETRY_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Self {
            geometry_ref: None,
            mesh_data: None,
            instance_id: id,
        }
    }
}
