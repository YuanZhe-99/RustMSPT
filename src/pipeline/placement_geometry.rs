//! Exact geometry is reconstructible from shared source shells and immutable transforms.
use crate::error::{Result, RustMsptError};
use crate::geometry::{box_mesh, mesh_bbox, to_parry_trimesh, transform_shell, UnitQuat};
use crate::types::{Mesh, Vec3};
use parry3d_f64::shape::TriMesh;
use std::collections::BTreeMap;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

pub struct PreparedGeometry {
    pub mesh: Mesh,
    pub shape: Option<TriMesh>,
}
struct Entry {
    geometry: Arc<PreparedGeometry>,
    bytes: usize,
    touched: u64,
}
#[derive(Default)]
struct CacheState {
    entries: BTreeMap<usize, Entry>,
    bytes: usize,
    clock: u64,
    hits: u64,
    loads: u64,
    evictions: u64,
}
pub struct GeometryCache {
    budget: usize,
    state: Mutex<CacheState>,
}
impl GeometryCache {
    // AI-FUNC-SUMMARY: Create a byte-budgeted exact-geometry LRU; zero disables retained entries, not exact checks.
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            state: Mutex::new(CacheState::default()),
        }
    }
    // AI-FUNC-SUMMARY: Acquire or reconstruct one immutable world mesh; serialized construction merges concurrent loads; returned Arc pins it until the query completes.
    fn get(&self, handle: &GeometryHandle) -> Arc<PreparedGeometry> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.clock += 1;
        let clock = state.clock;
        if state.entries.contains_key(&handle.id) {
            state.hits += 1;
            let entry = state.entries.get_mut(&handle.id).unwrap();
            entry.touched = clock;
            return entry.geometry.clone();
        }
        state.loads += 1;
        let mesh = handle.reconstruct();
        // Conservative accounting allowance for welded vertices, indices, QBVH,
        // topology and allocator overhead; this is not an operating-system RSS cap.
        let bytes = mesh
            .vertices
            .capacity()
            .saturating_mul(96)
            .saturating_add(mesh.faces.capacity().saturating_mul(512));
        let shape = to_parry_trimesh(&mesh);
        let geometry = Arc::new(PreparedGeometry { mesh, shape });
        while state.bytes.saturating_add(bytes) > self.budget && !state.entries.is_empty() {
            let Some((&key, _)) = state
                .entries
                .iter()
                .filter(|(_, e)| Arc::strong_count(&e.geometry) == 1)
                .min_by_key(|(_, e)| e.touched)
            else {
                break;
            };
            let old = state.entries.remove(&key).unwrap();
            state.bytes -= old.bytes;
            state.evictions += 1;
            // Existing query owners keep their Arc; eviction never invalidates them.
        }
        if state.bytes.saturating_add(bytes) <= self.budget {
            state.bytes += bytes;
            state.entries.insert(
                handle.id,
                Entry {
                    geometry: geometry.clone(),
                    bytes,
                    touched: clock,
                },
            );
        }
        geometry
    }
    // AI-FUNC-SUMMARY: Return diagnostic retained-cache usage/counters; excludes source library, active pins and output buffers.
    pub fn summary(&self) -> String {
        let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        format!(
            "estimated_bytes={} limit_bytes={} entries={} hits={} reconstructions={} evictions={}",
            s.bytes,
            self.budget,
            s.entries.len(),
            s.hits,
            s.loads,
            s.evictions
        )
    }
}
/// Only a shared canonical mesh and transform remain resident for a cold particle.
pub struct GeometryHandle {
    id: usize,
    canonical: Arc<Mesh>,
    scale: f64,
    rotation: UnitQuat,
    translation: Vec3,
    cache: Arc<GeometryCache>,
    pub simplified_collision: bool,
}
impl GeometryHandle {
    // AI-FUNC-SUMMARY: Store an immutable source reference and world transform; performs no mesh allocation.
    pub fn new(
        id: usize,
        canonical: Arc<Mesh>,
        scale: f64,
        rotation: UnitQuat,
        translation: Vec3,
        cache: Arc<GeometryCache>,
        simplified_collision: bool,
    ) -> Self {
        Self {
            id,
            canonical,
            scale,
            rotation,
            translation,
            cache,
            simplified_collision,
        }
    }
    // AI-FUNC-SUMMARY: Pin exact geometry for this operation through the shared bounded cache.
    pub fn get(&self) -> Arc<PreparedGeometry> {
        self.cache.get(self)
    }
    // AI-FUNC-SUMMARY: Reconstruct one particle in f64 with the same transform routine as acceptance; no STL float32 round trip.
    pub fn reconstruct(&self) -> Mesh {
        transform_shell(&self.canonical, self.scale, self.rotation, self.translation)
    }
    // AI-FUNC-SUMMARY: Build a conservative 12-triangle oriented source box; its empty space may cause escalation, never false acceptance of intersecting solids.
    pub fn proxy(&self) -> PreparedGeometry {
        let bb = mesh_bbox(&self.canonical).expect("validated source shell");
        let margin = 1e-12 * bb.size().x.max(bb.size().y).max(bb.size().z).max(1.0);
        let mesh = transform_shell(
            &box_mesh(bb.expanded(margin)),
            self.scale,
            self.rotation,
            self.translation,
        );
        let shape = to_parry_trimesh(&mesh);
        PreparedGeometry { mesh, shape }
    }
}

// AI-FUNC-SUMMARY: Stream exact reconstructed particles into binary STL in acceptance/face order; bounds memory by one particle and avoids collision-cache/BVH allocations; checks STL count limit.
pub fn write_particles_stl(
    path: &Path,
    placed: &[super::placement_feasibility::PlacedParticle],
    triangles: usize,
) -> Result<()> {
    let count = u32::try_from(triangles).map_err(|_| {
        RustMsptError::InvalidMesh("particle STL exceeds u32 triangle count".into())
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut writer = BufWriter::with_capacity(64 * 1024, std::fs::File::create(path)?);
    let mut header = [0u8; 80];
    header[..9].copy_from_slice(b"particles");
    writer.write_all(&header)?;
    writer.write_all(&count.to_le_bytes())?;
    for p in placed {
        let mesh = match &p.geometry {
            Some(g) => g.reconstruct(),
            None => p.mesh.clone(),
        };
        for f in &mesh.faces {
            writer.write_all(&[0u8; 12])?;
            for v in [mesh.vertices[f.a], mesh.vertices[f.b], mesh.vertices[f.c]] {
                for c in [v.x, v.y, v.z] {
                    writer.write_all(&(c as f32).to_le_bytes())?;
                }
            }
            writer.write_all(&[0u8; 2])?;
        }
    }
    writer.flush()?;
    Ok(())
}
