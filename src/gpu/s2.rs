use crate::types::{BoundingBox, Mesh};

const WORKGROUP_SIZE: u32 = crate::compute::mc_memory::MC_BLOCK_SAMPLES as u32;
const MAX_RADII: usize = crate::compute::mc_memory::MC_RADIUS_BATCH;
const COALESCE_FACES: usize = 8;
const MAX_UPLOAD_RUNS: usize = 64;
const PARAMS_BYTES: u64 = crate::compute::mc_memory::MC_PARAMS_BYTES;
const UNCERTAIN_WORDS: usize = crate::compute::mc_memory::MC_UNCERTAIN_WORDS;

// AI-FUNC-SUMMARY: Report triangle-buffer upload traffic of one MC pipeline: full and partial update counts plus byte totals.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GpuUploadStats {
    pub full_uploads: u64,
    pub partial_uploads: u64,
    pub unchanged_updates: u64,
    pub total_bytes: u64,
    pub last_bytes: u64,
}

// AI-FUNC-SUMMARY: GPU-accelerated Monte Carlo S2 pipeline using wgpu compute shaders.
// Holds the wgpu device, queue, compute pipeline, and pre-allocated buffers for triangle
// data, parameters, and per-radius workgroup output (hit/valid partial counts). Call `calculate_s2_gpu`
// to dispatch the Monte Carlo kernel in radius batches of at most MAX_RADII radii each.
pub struct GpuS2Pipeline {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    triangle_buffer: wgpu::Buffer,
    params_buffer: wgpu::Buffer,
    out_hits_buffer: wgpu::Buffer,
    out_valids_buffer: wgpu::Buffer,
    staging_hits: wgpu::Buffer,
    staging_valids: wgpu::Buffer,
    uncertain_buffer: wgpu::Buffer,
    uncertain_staging: wgpu::Buffer,
    reference: super::certify::CertReference,
    cert_stats: super::certify::GpuCertificationStats,
    #[cfg(test)]
    record_all: bool,
    #[cfg(test)]
    last_uncertain: Vec<(u32, [f32; 3], [f32; 3])>,
    #[cfg(test)]
    reference_samples: bool,
    #[cfg(test)]
    test_dispatch_limit: Option<u32>,
    #[cfg(test)]
    last_cpu_reduce: std::time::Duration,
    #[cfg(test)]
    last_readback_bytes: u64,
    #[cfg(test)]
    test_radius_batch: Option<usize>,
    pending_upload_bytes: u64,
    num_triangles: u32,
    resident: Vec<f32>,
    upload_stats: GpuUploadStats,
    bind_group_layout: wgpu::BindGroupLayout,
}

// AI-FUNC-SUMMARY: Build f32 triangle position buffer with coordinates normalized to bbox origin.
// Inputs: mesh reference, bounding box for normalization.
// Returns: Vec<f32> with 9 floats per triangle, all positions shifted by -bbox.min in f64 before conversion to f32.
// Side effects: None.
// Notes: Normalization improves f32 precision by keeping coordinates small (0..size instead of 100..1200+).
fn build_triangle_buffer(mesh: &Mesh, bbox: BoundingBox) -> Vec<f32> {
    let ox = bbox.min.x;
    let oy = bbox.min.y;
    let oz = bbox.min.z;
    let mut buf = Vec::with_capacity(mesh.faces.len() * 9);
    for face in &mesh.faces {
        let a = mesh.vertices[face.a];
        let b = mesh.vertices[face.b];
        let c = mesh.vertices[face.c];
        buf.extend_from_slice(&[(a.x - ox) as f32, (a.y - oy) as f32, (a.z - oz) as f32]);
        buf.extend_from_slice(&[(b.x - ox) as f32, (b.y - oy) as f32, (b.z - oz) as f32]);
        buf.extend_from_slice(&[(c.x - ox) as f32, (c.y - oy) as f32, (c.z - oz) as f32]);
    }
    buf
}

// AI-FUNC-SUMMARY:
// Purpose: Pack shader parameters into a byte buffer matching the WGSL Params struct layout.
// Inputs: triangle count, radii slice, seed, bounding box (used for normalized size), sample count per radius.
// Returns: Vec<u8> matching the WGSL Params struct with std430 alignment. Bbox min is always (0,0,0);
// the word after it carries radius_base, the global radius of batch slot 0; `tail` appends the certification
// block (exact f32 mesh early-out bounds and flags) after the 128 radii.
// Side effects: None.
fn pack_params(
    num_triangles: u32,
    radii: &[f32],
    seed: u32,
    bbox: BoundingBox,
    samples_per_radius: u32,
    radius_base: u32,
    tail: &[u8],
) -> Vec<u8> {
    let mut buf = Vec::with_capacity(PARAMS_BYTES as usize);

    buf.extend_from_slice(&num_triangles.to_le_bytes());
    buf.extend_from_slice(&(radii.len() as u32).to_le_bytes());
    buf.extend_from_slice(&samples_per_radius.to_le_bytes());
    buf.extend_from_slice(&seed.to_le_bytes());

    let size = bbox.size();
    buf.extend_from_slice(&0.0f32.to_le_bytes());
    buf.extend_from_slice(&0.0f32.to_le_bytes());
    buf.extend_from_slice(&0.0f32.to_le_bytes());
    buf.extend_from_slice(&radius_base.to_le_bytes());

    buf.extend_from_slice(&(size.x as f32).to_le_bytes());
    buf.extend_from_slice(&(size.y as f32).to_le_bytes());
    buf.extend_from_slice(&(size.z as f32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    let (dx, dy, dz) = super::super::geometry::s2::RAY_DIR_GPU;
    buf.extend_from_slice(&(dx as f32).to_le_bytes());
    buf.extend_from_slice(&(dy as f32).to_le_bytes());
    buf.extend_from_slice(&(dz as f32).to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    for i in 0..MAX_RADII {
        if i < radii.len() {
            buf.extend_from_slice(&radii[i].to_le_bytes());
        } else {
            buf.extend_from_slice(&0.0f32.to_le_bytes());
        }
    }
    buf.extend_from_slice(tail);
    buf
}

// AI-FUNC-SUMMARY:
// Purpose: Validate global logical sample ids and the largest radius batch's workgroup/buffer limits before allocation.
// Inputs: inclusive r_max, requested samples per radius, radius batch size (1..=MAX_RADII), device limits.
// Returns: (samples per radius, partial-count slots of the largest batch, its two-dimensional dispatch) or an error.
// Side effects: None.
// Notes: Any r_max is accepted as long as (r_max + 1) * samples fits the u32 logical id space the RNG is keyed on.
fn dispatch_plan(
    r_max: usize,
    samples: usize,
    batch: usize,
    limits: &wgpu::Limits,
) -> Result<(u32, u32, [u32; 2]), String> {
    if batch == 0 || batch > MAX_RADII {
        return Err(format!("GPU MC radius batch must be 1..={MAX_RADII}"));
    }
    let radii = u32::try_from(r_max)
        .ok()
        .and_then(|r| r.checked_add(1))
        .ok_or("GPU MC radius count exceeds u32")?;
    let samples = u32::try_from(samples.max(200))
        .map_err(|_| "GPU MC sample count exceeds u32".to_string())?;
    radii
        .checked_mul(samples)
        .ok_or("GPU MC invocation count overflow")?;
    let total = radii.min(batch as u32) * samples.div_ceil(WORKGROUP_SIZE);
    let grid = super::runtime::grid_plan([total, 1, 1], 1, limits)?;
    Ok((samples, total, grid.dispatch))
}

// AI-FUNC-SUMMARY: Check storage and buffer byte limits before GPU allocation; returns success or a capacity error.
fn check_buffer_size(bytes: u64, limits: &wgpu::Limits) -> Result<(), String> {
    if bytes > limits.max_buffer_size || bytes > u64::from(limits.max_storage_buffer_binding_size) {
        return Err(format!(
            "GPU MC buffer of {bytes} bytes exceeds device limits"
        ));
    }
    Ok(())
}

// AI-FUNC-SUMMARY: Check triangle count and byte capacity before flattening or uploading geometry; returns success or an overflow/capacity error.
fn check_mesh_capacity(mesh: &Mesh, limits: &wgpu::Limits) -> Result<(), String> {
    u32::try_from(mesh.faces.len()).map_err(|_| "GPU MC triangle count exceeds u32")?;
    let bytes = mesh
        .faces
        .len()
        .checked_mul(36)
        .ok_or("GPU MC triangle byte count overflow")?;
    check_buffer_size((bytes as u64).max(4), limits)
}

// AI-FUNC-SUMMARY: Triangle storage usage; COPY_SRC lets tests read the resident contents back; returns buffer usages; side effects: None.
fn triangle_usage() -> wgpu::BufferUsages {
    wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC
}

// AI-FUNC-SUMMARY:
// Purpose: Find the triangle ranges whose uploaded f32 bits differ between two equal-length triangle buffers.
// Inputs: resident and next 9-float-per-triangle buffers of equal length.
// Returns: Some(sorted disjoint face ranges, gaps of up to COALESCE_FACES merged) or None when a full write is cheaper.
// Side effects: None.
// Notes: Compares bit patterns, so the uploaded bytes are identical to a full write; None when more than
// MAX_UPLOAD_RUNS runs remain or more than half the faces would be written.
fn changed_face_runs(resident: &[f32], next: &[f32]) -> Option<Vec<std::ops::Range<usize>>> {
    let faces = next.len() / 9;
    let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
    let mut written = 0usize;
    for face in 0..faces {
        let span = face * 9..face * 9 + 9;
        let same = resident[span.clone()]
            .iter()
            .zip(&next[span])
            .all(|(a, b)| a.to_bits() == b.to_bits());
        if same {
            continue;
        }
        match runs.last_mut() {
            Some(last) if face - last.end <= COALESCE_FACES => {
                written += face + 1 - last.end;
                last.end = face + 1;
            }
            _ => {
                if runs.len() == MAX_UPLOAD_RUNS {
                    return None;
                }
                runs.push(face..face + 1);
                written += 1;
            }
        }
    }
    (written * 2 <= faces).then_some(runs)
}

impl GpuS2Pipeline {
    // AI-FUNC-SUMMARY:
    // Purpose: Initialize wgpu device/queue and create the Monte Carlo S2 compute pipeline.
    // Inputs: mesh reference for triangle buffer pre-upload, bounding box for coordinate normalization.
    // Returns: Pipeline or initialization, triangle-capacity or scoped device error.
    // Side effects: Performs heavy wgpu device initialization and uploads normalized triangle data to GPU.
    pub fn new(mesh: &Mesh, bbox: BoundingBox) -> Result<Self, String> {
        Self::new_with_shader(mesh, bbox, include_str!("shaders/s2_monte_carlo.wgsl"))
    }

    // AI-FUNC-SUMMARY: Construct one MC pipeline from supplied shader source; production uses reduced counts and tests may supply the frozen per-sample reference.
    fn new_with_shader(
        mesh: &Mesh,
        bbox: BoundingBox,
        shader_source: &str,
    ) -> Result<Self, String> {
        let shared = super::context::shared_device()?;
        let (device, queue) = (shared.device().clone(), shared.queue().clone());

        super::runtime::scoped(&device.clone(), || {
            check_mesh_capacity(mesh, &device.limits())?;
            let (pipeline, bind_group_layout) = shared.cached_pipeline("s2_monte_carlo", shader_source, |device| {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("s2_monte_carlo"),
                    source: wgpu::ShaderSource::Wgsl(shader_source.into()),
                });

                let bind_group_layout =
                    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("s2_mc_bgl"),
                        entries: &[
                            wgpu::BindGroupLayoutEntry {
                                binding: 0,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 1,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 2,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 3,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                },
                                count: None,
                            },
                            wgpu::BindGroupLayoutEntry {
                                binding: 4,
                                visibility: wgpu::ShaderStages::COMPUTE,
                                ty: wgpu::BindingType::Buffer {
                                    ty: wgpu::BufferBindingType::Storage { read_only: false },
                                    has_dynamic_offset: false,
                                    min_binding_size: None,
                                },
                                count: None,
                            },
                        ],
                    });

                let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("s2_mc_pl"),
                    bind_group_layouts: &[&bind_group_layout],
                    push_constant_ranges: &[],
                });

                let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some("s2_mc_pipeline"),
                    layout: Some(&pipeline_layout),
                    module: &shader,
                    entry_point: Some("main"),
                    compilation_options: Default::default(),
                    cache: None,
                });
                (pipeline, bind_group_layout)
            })?;

            let tri_data = build_triangle_buffer(mesh, bbox);
            let tri_bytes = bytemuck::cast_slice::<f32, u8>(&tri_data);
            let triangle_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("triangles"),
                size: (tri_bytes.len() as u64).max(4),
                usage: triangle_usage(),
                mapped_at_creation: false,
            });
            queue.write_buffer(&triangle_buffer, 0, tri_bytes);
            let initial_bytes = tri_bytes.len() as u64;

            let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("params"),
                size: PARAMS_BYTES,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });

            let out_size = 4;
            let out_hits_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_hits"),
                size: out_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let out_valids_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("out_valids"),
                size: out_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });

            let staging = |label| {
                device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some(label),
                    size: 4,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            };
            let staging_hits = staging("staging_hits");
            let staging_valids = staging("staging_valids");
            let (uncertain_buffer, uncertain_staging) = uncertain_buffers(
                &device,
                crate::compute::mc_memory::MC_UNCERTAIN_INITIAL,
            );

            Ok(Self {
                device,
                queue,
                pipeline,
                triangle_buffer,
                params_buffer,
                out_hits_buffer,
                out_valids_buffer,
                staging_hits,
                staging_valids,
                uncertain_buffer,
                uncertain_staging,
                reference: super::certify::CertReference::new(mesh, bbox.min),
                cert_stats: Default::default(),
                #[cfg(test)]
                record_all: false,
                #[cfg(test)]
                last_uncertain: Vec::new(),
                #[cfg(test)]
                reference_samples: false,
                #[cfg(test)]
                test_dispatch_limit: None,
                #[cfg(test)]
                last_cpu_reduce: std::time::Duration::ZERO,
                #[cfg(test)]
                last_readback_bytes: 0,
                #[cfg(test)]
                test_radius_batch: None,
                pending_upload_bytes: initial_bytes,
                num_triangles: mesh.faces.len() as u32,
                resident: tri_data,
                upload_stats: GpuUploadStats {
                    full_uploads: 1,
                    total_bytes: initial_bytes,
                    last_bytes: initial_bytes,
                    ..Default::default()
                },
                bind_group_layout,
            })
        })
    }

    // AI-FUNC-SUMMARY: Check retained buffers, pending uploads and growth for the next mesh update/evaluation against an optional logical GPU budget before any allocation or upload.
    pub fn check_evaluation_budget(
        &self,
        mesh: &Mesh,
        r_max: usize,
        samples: usize,
        limit_mb: Option<u64>,
    ) -> Result<(), String> {
        let peak = crate::compute::mc_memory::mc_evaluation_peak(
            self.triangle_buffer.size(),
            self.out_hits_buffer.size(),
            self.pending_upload_bytes,
            self.uncertain_buffer.size(),
            mesh.faces.len(),
            r_max,
            samples,
        )?;
        crate::compute::mc_memory::check_mc_budget(peak, limit_mb)
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Make the GPU triangle buffer equal to a new mesh, uploading only the triangles that differ from what is resident.
    // Inputs: mesh and bounding box for normalized triangle data.
    // Returns: Success or a capacity/upload error.
    // Side effects: Writes changed triangle runs (or the whole buffer) to the GPU, grows the buffer when needed,
    // replaces the host shadow copy and updates upload statistics and pending-upload accounting.
    // Notes: The diff is against the host shadow of the resident contents, so it is correct whichever caller
    // uploaded last (islands sharing one instance, rejected moves, migration). A changed triangle count, a
    // grown buffer, more than MAX_UPLOAD_RUNS runs or more than half the bytes changed fall back to one full write.
    pub fn update_mesh(&mut self, mesh: &Mesh, bbox: BoundingBox) -> Result<(), String> {
        check_mesh_capacity(mesh, &self.device.limits())?;
        super::runtime::scoped(&self.device.clone(), || {
            let tri_data = build_triangle_buffer(mesh, bbox);
            let tri_bytes = bytemuck::cast_slice::<f32, u8>(&tri_data);
            let needed = tri_bytes.len() as u64;
            let mut grown = false;
            if needed > self.triangle_buffer.size() {
                self.triangle_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("triangles"),
                    size: needed,
                    usage: triangle_usage(),
                    mapped_at_creation: false,
                });
                grown = true;
            }
            let runs = if grown || tri_data.len() != self.resident.len() {
                None
            } else {
                changed_face_runs(&self.resident, &tri_data)
            };
            let written = match &runs {
                None => {
                    self.queue.write_buffer(&self.triangle_buffer, 0, tri_bytes);
                    needed
                }
                Some(runs) => {
                    let mut bytes = 0u64;
                    for run in runs {
                        let range = run.start * 36..run.end * 36;
                        self.queue.write_buffer(
                            &self.triangle_buffer,
                            range.start as u64,
                            &tri_bytes[range.clone()],
                        );
                        bytes += range.len() as u64;
                    }
                    bytes
                }
            };
            let pending = self
                .pending_upload_bytes
                .checked_add(written)
                .ok_or("GPU MC pending upload size overflow")?;
            self.pending_upload_bytes = pending;
            self.num_triangles = mesh.faces.len() as u32;
            self.resident = tri_data;
            self.reference = super::certify::CertReference::new(mesh, bbox.min);
            let stats = &mut self.upload_stats;
            match &runs {
                None => stats.full_uploads += 1,
                Some(runs) if runs.is_empty() => stats.unchanged_updates += 1,
                Some(_) => stats.partial_uploads += 1,
            }
            stats.total_bytes = stats.total_bytes.saturating_add(written);
            stats.last_bytes = written;
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY: Return this pipeline's triangle upload statistics; returns a copy; side effects: None.
    pub fn upload_stats(&self) -> GpuUploadStats {
        self.upload_stats
    }

    // AI-FUNC-SUMMARY: Return cumulative f32 certification counters (valid samples evaluated, samples recomputed on the CPU, list regrowths, host time); returns a copy; side effects: None.
    pub fn certification_stats(&self) -> super::certify::GpuCertificationStats {
        self.cert_stats
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Grow output and staging buffers if the new partial-count slot count exceeds capacity.
    // Inputs: radius/workgroup partial-count slots.
    // Returns: None.
    // Side effects: May reallocate GPU buffers if capacity is exceeded.
    fn ensure_output_capacity(&mut self, partials: u32) {
        if u64::from(partials) * 4 > self.out_hits_buffer.size() {
            self.resize_output_buffers(partials);
        }
    }

    // AI-FUNC-SUMMARY: Replace output and staging buffers with matching capacity; callers validate capacity and surround allocation with error scopes.
    fn resize_output_buffers(&mut self, partials: u32) {
        let needed = u64::from(partials) * 4;
        self.out_hits_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_hits"),
            size: needed,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.out_valids_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("out_valids"),
            size: needed,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        self.staging_hits = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_hits"),
            size: needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.staging_valids = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("staging_valids"),
            size: needed,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
    }

    // AI-FUNC-SUMMARY: Release retained output/readback and uncertain-list peak capacity (list back to its initial size) while preserving uploaded geometry and the compiled pipeline; return any device allocation error.
    pub fn release_output_capacity(&mut self) -> Result<(), String> {
        super::runtime::scoped(&self.device.clone(), || {
            self.resize_output_buffers(1);
            let (buffer, staging) = uncertain_buffers(
                &self.device,
                crate::compute::mc_memory::MC_UNCERTAIN_INITIAL,
            );
            self.uncertain_buffer = buffer;
            self.uncertain_staging = staging;
            Ok(())
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Compute Monte Carlo S2 two-point correlation on the GPU for all radii.
    // Inputs: bounding box, r_max (inclusive), sample count per radius.
    // Returns: S2 values indexed by radius or a capacity, execution or mapping error.
    // Side effects: Dispatches GPU compute work; maps staging buffers for readback.
    // Notes: r=0 is set to 0.0 (caller should overwrite with volume fraction). Classification is f32-certified: queries the GPU cannot prove are recomputed on the CPU, see certification_stats().
    pub fn calculate_s2_gpu(
        &mut self,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
    ) -> Result<Vec<f64>, String> {
        self.calculate_s2_gpu_counts(bbox, r_max, samples, rand::random::<u32>())
            .map(|counts| {
                counts
                    .into_iter()
                    .map(|[hits, valid]| {
                        if valid > 0 {
                            hits as f64 / valid as f64
                        } else {
                            0.0
                        }
                    })
                    .collect()
            })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Evaluate a fixed MC seed in radius batches and merge workgroup integer partials in u64.
    // Inputs: bounding box, inclusive r_max (any value whose logical ids fit u32), samples per radius, seed.
    // Returns: Per-radius [hits, valid] totals, or a capacity, execution or mapping error.
    // Side effects: Grows output/staging to the largest batch, writes params and reads back once per batch; re-evaluates
    // uncertain samples with the CPU f64 predicate (regrowing and re-dispatching an overflowed list) and updates certification counters.
    // Notes: Logical sample ids are global (radius * samples + sample), so counts are identical for any batch size.
    // Every count equals the CPU reference evaluated at the GPU's own f32 sample points in the origin-shifted frame.
    // The CPU merge touches each partial once: O((r_max + 1) * ceil(samples / 256)) additions in total.
    fn calculate_s2_gpu_counts(
        &mut self,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
        seed: u32,
    ) -> Result<Vec<[u64; 2]>, String> {
        #[allow(unused_mut)]
        let mut limits = self.device.limits();
        #[allow(unused_mut)]
        let mut batch = MAX_RADII;
        #[cfg(test)]
        {
            if let Some(limit) = self.test_dispatch_limit {
                limits.max_compute_workgroups_per_dimension = limit;
            }
            if let Some(size) = self.test_radius_batch {
                batch = size;
            }
        }
        #[allow(unused_mut)]
        let (samples_per_radius, partial_count, _) =
            dispatch_plan(r_max, samples, batch, &limits)?;
        #[allow(unused_mut)]
        let mut output_count = partial_count;
        #[allow(unused_mut)]
        let mut per_radius = samples_per_radius.div_ceil(WORKGROUP_SIZE);
        #[cfg(test)]
        if self.reference_samples {
            if r_max >= batch {
                return Err("per-sample reference shader supports a single radius batch".into());
            }
            output_count = (r_max as u32 + 1) * samples_per_radius;
            per_radius = samples_per_radius;
            check_buffer_size(u64::from(output_count) * 4, &self.device.limits())?;
        }
        let size = bbox.size();
        if [size.x, size.y, size.z]
            .iter()
            .any(|v| !(*v as f32).is_finite() || (*v as f32) <= 0.0)
        {
            return Err("GPU MC bbox must have finite positive f32 extents".into());
        }
        super::runtime::scoped(&self.device.clone(), || {
            self.ensure_output_capacity(output_count);
            #[allow(unused_mut)]
            let mut flags = 0u32;
            #[cfg(test)]
            {
                if self.record_all {
                    flags |= 1;
                }
                self.last_uncertain.clear();
            }
            let tail = self.reference.params_tail(flags);
            #[cfg(test)]
            let mut reduce_total = std::time::Duration::ZERO;
            #[cfg(test)]
            let mut readback_total = 0u64;
            let mut out = vec![[0u64; 2]; r_max + 1];
            let sp = per_radius as usize;
            for base in (0..=r_max).step_by(batch) {
                let last = (base + batch - 1).min(r_max);
                let radii: Vec<f32> = (base..=last).map(|r| r as f32).collect();
                #[allow(unused_mut)]
                let mut batch_outputs = radii.len() as u32 * per_radius;
                #[allow(unused_mut)]
                let mut dispatch =
                    super::runtime::grid_plan([batch_outputs, 1, 1], 1, &limits)?.dispatch;
                #[cfg(test)]
                if self.reference_samples {
                    batch_outputs = output_count;
                    dispatch = [output_count.div_ceil(WORKGROUP_SIZE), 1];
                }
                let param_data = pack_params(
                    self.num_triangles,
                    &radii,
                    seed,
                    bbox,
                    samples_per_radius,
                    base as u32,
                    &tail,
                );
                let pending = self
                    .pending_upload_bytes
                    .checked_add(param_data.len() as u64)
                    .ok_or("GPU MC pending upload size overflow")?;
                self.queue.write_buffer(&self.params_buffer, 0, &param_data);
                self.pending_upload_bytes = pending;

                let readback_size = u64::from(batch_outputs) * 4;
                let uncertain_count = loop {
                    let count = self.dispatch_batch(dispatch, readback_size)?;
                    let capacity =
                        (self.uncertain_buffer.size() - 4) / (UNCERTAIN_WORDS as u64 * 4);
                    if u64::from(count) <= capacity {
                        break count as usize;
                    }
                    let bytes = crate::compute::mc_memory::mc_uncertain_bytes(count as usize);
                    check_buffer_size(bytes, &self.device.limits())?;
                    let (buffer, staging) = uncertain_buffers(&self.device, count as usize);
                    self.uncertain_buffer = buffer;
                    self.uncertain_staging = staging;
                    self.cert_stats.list_regrowths += 1;
                };

                let hits_u32 = super::runtime::read_u32_prefix(
                    &self.device,
                    &self.staging_hits,
                    readback_size,
                )?;
                let valids_u32 = super::runtime::read_u32_prefix(
                    &self.device,
                    &self.staging_valids,
                    readback_size,
                )?;
                let entries = if uncertain_count > 0 {
                    super::runtime::read_u32_prefix(
                        &self.device,
                        &self.uncertain_staging,
                        crate::compute::mc_memory::mc_uncertain_bytes(uncertain_count),
                    )?
                } else {
                    Vec::new()
                };
                self.pending_upload_bytes = 0;
                #[cfg(test)]
                let reduce_start = std::time::Instant::now();
                #[cfg(test)]
                {
                    readback_total += readback_size * 2;
                }
                for (slot, r) in (base..=last).enumerate() {
                    let start = slot * sp;
                    let end = ((slot + 1) * sp).min(batch_outputs as usize);
                    if start >= end {
                        continue;
                    }
                    let total_hits: u64 = hits_u32[start..end].iter().map(|&v| v as u64).sum();
                    let total_valid: u64 =
                        valids_u32[start..end].iter().map(|&v| v as u64).sum();
                    out[r] = [total_hits, total_valid];
                    self.cert_stats.queries += total_valid;
                }
                #[cfg(test)]
                {
                    reduce_total += reduce_start.elapsed();
                }
                self.resolve_uncertain(&entries, samples_per_radius, base..=last, &mut out)?;
            }

            #[cfg(test)]
            {
                self.last_cpu_reduce = reduce_total;
                self.last_readback_bytes = readback_total;
            }
            Ok(out)
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Run one radius batch: clear the uncertain counter, dispatch the MC kernel, copy partial counts and the whole uncertain list to staging, and read the uncertain counter.
    // Inputs: two-dimensional dispatch, byte size of each partial-count readback.
    // Returns: The number of uncertain samples the kernel reported (may exceed list capacity), or a mapping error.
    // Side effects: Writes 4 bytes, submits one command buffer; hit/valid/list staging hold this batch's results.
    fn dispatch_batch(&mut self, dispatch: [u32; 2], readback_size: u64) -> Result<u32, String> {
        self.queue
            .write_buffer(&self.uncertain_buffer, 0, &0u32.to_le_bytes());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("s2_mc_bg"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.triangle_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.params_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.out_hits_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.out_valids_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.uncertain_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("s2_mc_encoder"),
            });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("s2_mc_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(dispatch[0], dispatch[1], 1);
        }
        encoder.copy_buffer_to_buffer(&self.out_hits_buffer, 0, &self.staging_hits, 0, readback_size);
        encoder.copy_buffer_to_buffer(
            &self.out_valids_buffer,
            0,
            &self.staging_valids,
            0,
            readback_size,
        );
        encoder.copy_buffer_to_buffer(
            &self.uncertain_buffer,
            0,
            &self.uncertain_staging,
            0,
            self.uncertain_buffer.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        Ok(super::runtime::read_u32_prefix(&self.device, &self.uncertain_staging, 4)?[0])
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Re-evaluate the batch's uncertain samples at their exact f32 GPU points with the CPU f64 predicate and add their hits.
    // Inputs: raw list words (counter then 7-word records), samples per radius, the batch's global radii, per-radius totals.
    // Returns: Success, or an error if a record names a radius outside the batch.
    // Side effects: Increments hits in `out`; updates certification counters; serial by design (callers may hold a GPU mutex inside a Rayon worker).
    fn resolve_uncertain(
        &mut self,
        entries: &[u32],
        samples_per_radius: u32,
        radii: std::ops::RangeInclusive<usize>,
        out: &mut [[u64; 2]],
    ) -> Result<(), String> {
        if entries.len() <= 1 {
            return Ok(());
        }
        let start = std::time::Instant::now();
        let records: Vec<(u32, [f32; 3], [f32; 3])> = entries[1..]
            .chunks_exact(UNCERTAIN_WORDS)
            .map(|w| {
                let f = |i: usize| f32::from_bits(w[i]);
                (w[0], [f(1), f(2), f(3)], [f(4), f(5), f(6)])
            })
            .collect();
        let points: Vec<[f32; 3]> = records.iter().flat_map(|(_, p, q)| [*p, *q]).collect();
        let inside = self.reference.classify(&points);
        for (k, (id, _, _)) in records.iter().enumerate() {
            let r = (id / samples_per_radius) as usize;
            if !radii.contains(&r) {
                return Err(format!(
                    "GPU MC uncertain record {id} lies outside radius batch {radii:?}"
                ));
            }
            if inside[2 * k] && inside[2 * k + 1] {
                out[r][0] += 1;
            }
        }
        self.cert_stats.uncertain += records.len() as u64;
        self.cert_stats.cpu_recompute_seconds += start.elapsed().as_secs_f64();
        #[cfg(test)]
        self.last_uncertain.extend(records);
        Ok(())
    }
}

// AI-FUNC-SUMMARY: Allocate an uncertain-sample list (atomic counter plus 7-word records) and its map-read staging for `entries` records; callers check limits and hold an error scope.
fn uncertain_buffers(device: &wgpu::Device, entries: usize) -> (wgpu::Buffer, wgpu::Buffer) {
    let size = crate::compute::mc_memory::mc_uncertain_bytes(entries.max(1));
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("s2_mc_uncertain"),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let staging = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("s2_mc_uncertain_staging"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    (buffer, staging)
}

#[cfg(test)]
mod normalization_tests {
    use super::*;
    use crate::geometry::{box_mesh, translate_mesh};
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: Verify that large global translations preserve sub-unit triangle coordinates uploaded to the GPU.
    #[test]
    fn triangle_upload_preserves_local_geometry() {
        let mut bbox = BoundingBox {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(4.0, 4.0, 4.0),
        };
        let mut mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.25, 0.5, 0.75),
            max: Vec3::new(1.25, 2.5, 3.75),
        });
        let expected = build_triangle_buffer(&mesh, bbox);
        let shift = Vec3::new(1e9, -1e9, 1e9);
        translate_mesh(&mut mesh, shift);
        bbox.min = Vec3::new(shift.x, shift.y, shift.z);
        bbox.max = Vec3::new(shift.x + 4.0, shift.y + 4.0, shift.z + 4.0);
        assert_eq!(build_triangle_buffer(&mesh, bbox), expected);
    }
}

#[cfg(test)]
mod execution_tests {
    use super::*;
    use crate::geometry::box_mesh;
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: Verify checked radius/count/dispatch/storage limits without allocating GPU memory.
    #[test]
    fn dispatch_limits_reject_before_allocation() {
        let mut limits = wgpu::Limits::default();
        let b = MAX_RADII;
        assert!(dispatch_plan(127, 200, b, &limits).is_ok());
        assert_eq!(dispatch_plan(128, 200, b, &limits).unwrap().1, 128);
        assert_eq!(dispatch_plan(10_000, 200, b, &limits).unwrap().1, 128);
        assert!(dispatch_plan(0, 200, 0, &limits).is_err());
        assert!(dispatch_plan(0, 200, MAX_RADII + 1, &limits).is_err());
        assert!(dispatch_plan(usize::MAX, 200, b, &limits).is_err());
        assert!(dispatch_plan(0, usize::MAX, b, &limits).is_err());
        assert!(dispatch_plan(127, u32::MAX as usize, b, &limits).is_err());
        limits.max_compute_workgroups_per_dimension = 1;
        assert_eq!(dispatch_plan(0, 256, b, &limits).unwrap(), (256, 1, [1, 1]));
        assert!(dispatch_plan(0, 257, b, &limits).is_err());
        assert_eq!(dispatch_plan(5, 256, 1, &limits).unwrap(), (256, 1, [1, 1]));
        limits.max_storage_buffer_binding_size = 3;
        assert!(dispatch_plan(0, 256, b, &limits).is_err());
        limits.max_storage_buffer_binding_size = 4;
        limits.max_buffer_size = 3;
        assert!(dispatch_plan(0, 256, b, &limits).is_err());
    }

    // AI-FUNC-SUMMARY: Trigger a real invalid GPU mapping under error scopes, verify balanced recovery, and exercise empty geometry and logical radius-overflow refusal.
    #[test]
    fn mapping_errors_are_results_and_scopes_recover() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0));
        let mesh = box_mesh(bbox);
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP GPU unavailable: {error}");
                return;
            }
        };
        let error = super::super::runtime::scoped(&gpu.device, || {
            super::super::runtime::read_u32(&gpu.device, &gpu.params_buffer)
        });
        assert!(error.is_err());
        assert!(gpu
            .calculate_s2_gpu(bbox, usize::MAX, 200)
            .unwrap_err()
            .contains("radius count"));
        let tiny_bbox = BoundingBox::from_size(Vec3::new(1e-300, 1.0, 1.0));
        assert!(gpu
            .calculate_s2_gpu(tiny_bbox, 0, 200)
            .unwrap_err()
            .contains("bbox"));
        let empty = Mesh {
            vertices: vec![],
            faces: vec![],
        };
        gpu.update_mesh(&empty, bbox).unwrap();
        assert_eq!(gpu.calculate_s2_gpu(bbox, 1, 200).unwrap(), vec![0.0, 0.0]);
        let mut empty_gpu = GpuS2Pipeline::new(&empty, bbox).unwrap();
        assert_eq!(
            empty_gpu.calculate_s2_gpu(bbox, 1, 200).unwrap(),
            vec![0.0, 0.0]
        );
    }
    // AI-FUNC-SUMMARY: Check lazy output/staging allocation, buffer identity reuse, growth and live-prefix results after geometry changes.
    #[test]
    fn mc_staging_reuses_capacity_without_stale_results() {
        let bbox = BoundingBox {
            min: Vec3::new(0.0, 0.0, 0.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        };
        let empty = Mesh {
            vertices: Vec::new(),
            faces: Vec::new(),
        };
        let mut gpu = match GpuS2Pipeline::new(&empty, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        assert_eq!(gpu.out_hits_buffer.size(), 4);
        assert_eq!(gpu.out_valids_buffer.size(), 4);
        assert_eq!(gpu.staging_hits.size(), 4);
        assert_eq!(gpu.staging_valids.size(), 4);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 2, 200).unwrap(), vec![0.0; 3]);
        let hits = gpu.staging_hits.clone();
        let valids = gpu.staging_valids.clone();
        let output = gpu.out_hits_buffer.clone();
        assert_eq!(hits.size(), 3 * 4);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
        assert_eq!(gpu.staging_hits, hits);
        assert_eq!(gpu.staging_valids, valids);
        assert_eq!(gpu.out_hits_buffer, output);
        let enclosing = box_mesh(BoundingBox {
            min: Vec3::new(-1.0, -1.0, -1.0),
            max: Vec3::new(2.0, 2.0, 2.0),
        });
        gpu.update_mesh(&enclosing, bbox).unwrap();
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![1.0]);
        assert_eq!(gpu.staging_hits, hits);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 3, 400).unwrap()[0], 1.0);
        assert_ne!(gpu.staging_hits, hits);
        assert_eq!(gpu.staging_hits.size(), 4 * 2 * 4);
        gpu.update_mesh(&empty, bbox).unwrap();
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
        assert!(super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_hits, 0).is_err());
        assert!(super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_hits, 3).is_err());
        assert!(super::super::runtime::read_u32_prefix(
            &gpu.device,
            &gpu.staging_hits,
            gpu.staging_hits.size() + 4
        )
        .is_err());
        gpu.release_output_capacity().unwrap();
        assert_eq!(gpu.out_hits_buffer.size(), 4);
        assert_eq!(gpu.out_valids_buffer.size(), 4);
        assert_eq!(gpu.staging_hits.size(), 4);
        assert_eq!(gpu.staging_valids.size(), 4);
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
    }
    // AI-FUNC-SUMMARY: Verify budget rejection is allocation-free, queued geometry uploads (none for an unchanged mesh) are counted and cleared after completion, and retained large outputs require release before a small cap can be honored.
    #[test]
    fn mc_budget_tracks_live_capacity_and_uploads() {
        let bbox = BoundingBox::from_size(Vec3::new(1.0, 1.0, 1.0));
        let mesh = box_mesh(bbox);
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        assert_eq!(gpu.pending_upload_bytes, 432);
        let original = gpu.out_hits_buffer.clone();
        assert!(gpu.check_evaluation_budget(&mesh, 0, 200, Some(0)).is_err());
        assert_eq!(original, gpu.out_hits_buffer);
        gpu.update_mesh(&mesh, bbox).unwrap();
        assert_eq!(gpu.pending_upload_bytes, 432, "an unchanged mesh uploads nothing");
        let mut moved = mesh.clone();
        crate::geometry::translate_mesh(&mut moved, Vec3::new(0.25, 0.0, 0.0));
        gpu.update_mesh(&moved, bbox).unwrap();
        assert_eq!(gpu.pending_upload_bytes, 864);
        assert!(gpu.check_evaluation_budget(&mesh, 0, 200, Some(1)).is_ok());
        gpu.calculate_s2_gpu(bbox, 0, 200).unwrap();
        assert_eq!(gpu.pending_upload_bytes, 0);
        gpu.update_mesh(&Mesh::empty(), bbox).unwrap();
        super::super::runtime::scoped(&gpu.device.clone(), || {
            gpu.resize_output_buffers(65_535);
            Ok(())
        })
        .unwrap();
        assert!(gpu
            .check_evaluation_budget(&Mesh::empty(), 0, 200, Some(1))
            .is_err());
        gpu.release_output_capacity().unwrap();
        assert!(gpu
            .check_evaluation_budget(&Mesh::empty(), 0, 200, Some(1))
            .is_ok());
        assert_eq!(gpu.calculate_s2_gpu(bbox, 0, 200).unwrap(), vec![0.0]);
    }
    // AI-FUNC-SUMMARY: Compare reduced workgroup counts exactly with the frozen per-sample GPU shader under identical seeds, checking padding, radius boundaries and repeated geometry updates.
    #[test]
    fn workgroup_counts_match_per_sample_reference() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.5, 0.5, 0.5),
            max: Vec3::new(2.5, 2.5, 2.5),
        });
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        let mut reference = GpuS2Pipeline::new_with_shader(
            &mesh,
            bbox,
            include_str!("../../tests/fixtures/s2_monte_carlo_samples.wgsl"),
        )
        .unwrap();
        reference.reference_samples = true;
        for (r_max, samples) in [
            (0, 0),
            (3, 200),
            (3, 255),
            (3, 256),
            (3, 257),
            (3, 511),
            (3, 512),
            (3, 513),
            (3, 1000),
            (127, 257),
        ] {
            for seed in [0, u32::MAX] {
                let expected = reference
                    .calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                    .unwrap();
                let actual = gpu
                    .calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                    .unwrap();
                assert_eq!(
                    actual, expected,
                    "r_max={r_max} samples={samples} seed={seed}"
                );
                assert_eq!(
                    gpu.last_readback_bytes,
                    (r_max as u64 + 1) * samples.max(200).div_ceil(256) as u64 * 8
                );
                assert!(actual
                    .iter()
                    .all(|&[hits, valid]| hits <= valid && valid <= samples.max(200) as u64));
            }
        }
        gpu.update_mesh(&Mesh::empty(), bbox).unwrap();
        reference.update_mesh(&Mesh::empty(), bbox).unwrap();
        assert_eq!(
            gpu.calculate_s2_gpu_counts(bbox, 3, 257, 42).unwrap(),
            reference.calculate_s2_gpu_counts(bbox, 3, 257, 42).unwrap()
        );
    }

    // AI-FUNC-SUMMARY: Measure resident small/medium geometry MC evaluation and CPU integer merging with identical seeds; log actual readback bytes, alternating warm samples and total time for reduced versus frozen per-sample GPU output.
    #[test]
    #[ignore = "release GPU MC reduction benchmark"]
    fn workgroup_reduction_benchmark() {
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        for level in [0, 1] {
            let mesh = if level == 0 {
                box_mesh(BoundingBox {
                    min: Vec3::new(0.5, 0.5, 0.5),
                    max: Vec3::new(2.5, 2.5, 2.5),
                })
            } else {
                crate::geometry::icosphere_mesh(Vec3::new(1.5, 1.5, 1.5), 1.0, 1)
            };
            let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
                Ok(gpu) => gpu,
                Err(error) => {
                    eprintln!("SKIP: GPU MC unavailable: {error}");
                    return;
                }
            };
            let mut reference = GpuS2Pipeline::new_with_shader(
                &mesh,
                bbox,
                include_str!("../../tests/fixtures/s2_monte_carlo_samples.wgsl"),
            )
            .unwrap();
            reference.reference_samples = true;
            for samples in [200, 4000, 40000] {
                assert_eq!(
                    gpu.calculate_s2_gpu_counts(bbox, 3, samples, 42).unwrap(),
                    reference
                        .calculate_s2_gpu_counts(bbox, 3, samples, 42)
                        .unwrap()
                );
                let run = |p: &mut GpuS2Pipeline| {
                    let start = std::time::Instant::now();
                    let mut reduce = 0.0;
                    for i in 0..3 {
                        std::hint::black_box(
                            p.calculate_s2_gpu_counts(bbox, 3, samples, 42 + i).unwrap(),
                        );
                        reduce += p.last_cpu_reduce.as_secs_f64();
                    }
                    (start.elapsed().as_secs_f64(), reduce, p.last_readback_bytes)
                };
                for sample in 0..6 {
                    let (old, new) = if sample % 2 == 0 {
                        (run(&mut reference), run(&mut gpu))
                    } else {
                        let new = run(&mut gpu);
                        (run(&mut reference), new)
                    };
                    eprintln!("MC_REDUCE_BENCH faces={} samples={samples} sample={sample} repeats=3 old_seconds={:.9} new_seconds={:.9} old_reduce={:.9} new_reduce={:.9} old_bytes={} new_bytes={}",mesh.faces.len(),old.0,new.0,old.1,new.1,old.2,new.2);
                }
            }
        }
    }
    // AI-FUNC-SUMMARY: Validate two-dimensional dispatch capacity without allocating huge logical workloads, and compare forced multi-row/padded GPU execution with original sample counts at identical seeds.
    #[test]
    fn mc_two_dimensional_dispatch_preserves_samples() {
        let mut limits = wgpu::Limits::default();
        assert_eq!(
            dispatch_plan(0, 65_536 * 256, MAX_RADII, &limits).unwrap(),
            (65_536 * 256, 65_536, [65_535, 2])
        );
        limits.max_compute_workgroups_per_dimension = 3;
        assert_eq!(dispatch_plan(3, 257, MAX_RADII, &limits).unwrap(), (257, 8, [3, 3]));
        assert!(dispatch_plan(4, 257, MAX_RADII, &limits).is_err());
        assert_eq!(dispatch_plan(4, 257, 2, &limits).unwrap(), (257, 4, [3, 2]));
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let mesh = box_mesh(BoundingBox {
            min: Vec3::new(0.5, 0.5, 0.5),
            max: Vec3::new(2.5, 2.5, 2.5),
        });
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut gpu =
            GpuS2Pipeline::new(&mesh, bbox).expect("available GPU must compile the 2D MC shader");
        let mut reference = GpuS2Pipeline::new_with_shader(
            &mesh,
            bbox,
            include_str!("../../tests/fixtures/s2_monte_carlo_samples.wgsl"),
        )
        .unwrap();
        reference.reference_samples = true;
        for (r_max, samples) in [(0, 200), (3, 257), (1, 769), (0, 2049)] {
            for seed in [0, 42] {
                let expected = reference
                    .calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                    .unwrap();
                for limit in [3, 4] {
                    gpu.test_dispatch_limit = Some(limit);
                    assert_eq!(
                        gpu.calculate_s2_gpu_counts(bbox, r_max, samples, seed)
                            .unwrap(),
                        expected,
                        "limit={limit} r={r_max} s={samples}"
                    );
                }
            }
        }
        // Keep spare output capacity so a missing padded-group guard cannot hide behind robust out-of-bounds writes.
        gpu.test_dispatch_limit = Some(3);
        gpu.calculate_s2_gpu_counts(bbox, 0, 2049, 42).unwrap();
        let before = super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_valids, 36)
            .unwrap()[8];
        gpu.calculate_s2_gpu_counts(bbox, 3, 257, 42).unwrap();
        let after = super::super::runtime::scoped(&gpu.device, || {
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("padded MC output guard oracle"),
                });
            encoder.copy_buffer_to_buffer(&gpu.out_valids_buffer, 0, &gpu.staging_valids, 0, 36);
            gpu.queue.submit(Some(encoder.finish()));
            super::super::runtime::read_u32_prefix(&gpu.device, &gpu.staging_valids, 36)
        })
        .unwrap()[8];
        assert_eq!(
            after, before,
            "padded workgroup wrote into retained spare capacity"
        );
    }
}

#[cfg(test)]
mod batching_and_upload_tests {
    use super::*;
    use crate::geometry::{box_mesh, merge_meshes, translate_mesh};
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: Read the resident triangle buffer prefix back as raw u32 words for byte-exact oracles; returns the words or a mapping error.
    fn resident_words(gpu: &GpuS2Pipeline) -> Result<Vec<u32>, String> {
        let bytes = u64::from(gpu.num_triangles) * 36;
        super::super::runtime::scoped(&gpu.device, || {
            let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("triangle oracle"),
                size: bytes.max(4),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = gpu
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_buffer_to_buffer(&gpu.triangle_buffer, 0, &staging, 0, bytes);
            gpu.queue.submit(Some(encoder.finish()));
            super::super::runtime::read_u32_prefix(&gpu.device, &staging, bytes)
        })
    }

    // AI-FUNC-SUMMARY: Build two separated boxes in a 400-wide domain, the second shifted by `shift`; returns the merged mesh.
    fn two_boxes(shift: f64) -> Mesh {
        let a = box_mesh(BoundingBox {
            min: Vec3::new(50.0, 50.0, 50.0),
            max: Vec3::new(200.0, 350.0, 350.0),
        });
        let mut b = box_mesh(BoundingBox {
            min: Vec3::new(250.0, 50.0, 50.0),
            max: Vec3::new(350.0, 350.0, 350.0),
        });
        translate_mesh(&mut b, Vec3::new(shift, shift * 0.5, 0.0));
        merge_meshes(&[a, b])
    }

    // AI-FUNC-SUMMARY: Verify radius batching reproduces single-batch integer counts exactly for any batch size, supports r_max beyond the shader array and leaves low-radius counts independent of r_max.
    #[test]
    fn radius_batches_match_single_batch_counts() {
        let bbox = BoundingBox::from_size(Vec3::new(400.0, 400.0, 400.0));
        let mesh = two_boxes(0.0);
        let mut gpu = match GpuS2Pipeline::new(&mesh, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        for seed in [0u32, 7, u32::MAX] {
            let single = gpu.calculate_s2_gpu_counts(bbox, 127, 257, seed).unwrap();
            for size in [1usize, 7, 64] {
                gpu.test_radius_batch = Some(size);
                assert_eq!(
                    gpu.calculate_s2_gpu_counts(bbox, 127, 257, seed).unwrap(),
                    single,
                    "batch={size} seed={seed}"
                );
            }
            gpu.test_radius_batch = None;
            let wide = gpu.calculate_s2_gpu_counts(bbox, 300, 257, seed).unwrap();
            assert_eq!(wide.len(), 301);
            assert_eq!(&wide[..128], &single[..]);
            assert_eq!(gpu.last_readback_bytes, 301 * 2 * 8);
            assert!(wide.iter().all(|&[h, v]| h <= v && v <= 257));
            assert!(wide[200][1] > 0 && wide[200][0] > 0);
            gpu.test_radius_batch = Some(13);
            assert_eq!(gpu.calculate_s2_gpu_counts(bbox, 300, 257, seed).unwrap(), wide);
            gpu.test_radius_batch = None;
        }
        assert_eq!(gpu.calculate_s2_gpu(bbox, 300, 257).unwrap().len(), 301);
    }

    // AI-FUNC-SUMMARY: Verify diffed partial uploads leave GPU triangle bytes and fixed-seed counts identical to a full re-upload across moves, restores, no-ops and layout changes, and record their byte counts.
    #[test]
    fn partial_triangle_upload_matches_full_upload() {
        let bbox = BoundingBox::from_size(Vec3::new(400.0, 400.0, 400.0));
        let base = two_boxes(0.0);
        let mut gpu = match GpuS2Pipeline::new(&base, bbox) {
            Ok(gpu) => gpu,
            Err(error) => {
                eprintln!("SKIP: GPU MC unavailable: {error}");
                return;
            }
        };
        let full_bytes = base.faces.len() as u64 * 36;
        assert_eq!(gpu.upload_stats().full_uploads, 1);
        let particle_bytes = 12 * 36;
        let mut last = base.clone();
        for (step, shift) in [3.5, -7.25, 0.0, 11.0, 11.0].into_iter().enumerate() {
            let next = two_boxes(shift);
            let before = gpu.upload_stats();
            gpu.update_mesh(&next, bbox).unwrap();
            let after = gpu.upload_stats();
            let expected = if next.vertices == last.vertices { 0 } else { particle_bytes };
            assert_eq!(after.last_bytes, expected, "step {step}");
            assert_eq!(after.full_uploads, before.full_uploads);
            assert_eq!(after.total_bytes, before.total_bytes + expected);
            let words = resident_words(&gpu).unwrap();
            let oracle: Vec<u32> = build_triangle_buffer(&next, bbox)
                .into_iter()
                .map(f32::to_bits)
                .collect();
            assert_eq!(words, oracle, "step {step}");
            let mut fresh = GpuS2Pipeline::new(&next, bbox).unwrap();
            assert_eq!(
                gpu.calculate_s2_gpu_counts(bbox, 140, 300, 99).unwrap(),
                fresh.calculate_s2_gpu_counts(bbox, 140, 300, 99).unwrap(),
                "step {step}"
            );
            eprintln!(
                "MC_UPLOAD step={step} bytes={} full_equivalent={full_bytes}",
                after.last_bytes
            );
            last = next;
        }
        let stats = gpu.upload_stats();
        assert_eq!(stats.unchanged_updates, 1);
        assert_eq!(stats.partial_uploads, 4);
        let single = box_mesh(BoundingBox {
            min: Vec3::new(10.0, 10.0, 10.0),
            max: Vec3::new(20.0, 20.0, 20.0),
        });
        gpu.update_mesh(&single, bbox).unwrap();
        assert_eq!(gpu.upload_stats().full_uploads, 2);
        assert_eq!(gpu.upload_stats().last_bytes, 12 * 36);
        let moved_bbox = BoundingBox {
            min: Vec3::new(-1.0, 0.0, 0.0),
            max: Vec3::new(400.0, 400.0, 400.0),
        };
        gpu.update_mesh(&single, moved_bbox).unwrap();
        assert_eq!(gpu.upload_stats().full_uploads, 3);
        let words = resident_words(&gpu).unwrap();
        let oracle: Vec<u32> = build_triangle_buffer(&single, moved_bbox)
            .into_iter()
            .map(f32::to_bits)
            .collect();
        assert_eq!(words, oracle);
    }

    // AI-FUNC-SUMMARY: Check run coalescing and full-write fallback thresholds of the host diff without a GPU.
    #[test]
    fn changed_face_runs_coalesce_and_fall_back() {
        let resident = vec![0.0f32; 9 * 100];
        let mut next = resident.clone();
        assert_eq!(changed_face_runs(&resident, &next), Some(vec![]));
        next[9 * 3] = 1.0;
        next[9 * 10 + 4] = 1.0;
        next[9 * 40] = -0.0;
        assert_eq!(changed_face_runs(&resident, &next), Some(vec![3..11, 40..41]));
        let mut many = resident.clone();
        for face in (0..100).step_by(10).take(9) {
            many[face * 9] = 2.0;
        }
        assert_eq!(changed_face_runs(&resident, &many).unwrap().len(), 9);
        let half: Vec<f32> = (0..9 * 100).map(|i| if i < 9 * 51 { 1.0 } else { 0.0 }).collect();
        assert_eq!(changed_face_runs(&resident, &half), None);
        let resident = vec![0.0f32; 9 * 10_000];
        let mut scattered = resident.clone();
        for face in (0..10_000).step_by(20).take(MAX_UPLOAD_RUNS + 1) {
            scattered[face * 9] = 1.0;
        }
        assert_eq!(changed_face_runs(&resident, &scattered), None);
    }
}

#[cfg(test)]
mod certification_tests {
    use super::*;
    use crate::geometry::point_inside_mesh;
    use crate::gpu::certify::fixtures::{adversarial_meshes, shifted};
    use crate::types::Vec3;

    // AI-FUNC-SUMMARY: CPU f64 evaluation of the identical GPU sample points: record every valid sample's exact f32 p/q, classify both with point_inside_mesh on the origin-shifted mesh, and return per-radius [hits, valid].
    fn cpu_counts_of_gpu_points(
        gpu: &mut GpuS2Pipeline,
        bbox: BoundingBox,
        r_max: usize,
        samples: usize,
        seed: u32,
    ) -> Vec<[u64; 2]> {
        gpu.record_all = true;
        let recorded = gpu.calculate_s2_gpu_counts(bbox, r_max, samples, seed).unwrap();
        gpu.record_all = false;
        let spr = samples.max(200) as u32;
        let mut out = vec![[0u64; 2]; r_max + 1];
        let to_vec = |p: [f32; 3]| Vec3::new(p[0] as f64, p[1] as f64, p[2] as f64);
        for &(id, p, q) in &gpu.last_uncertain {
            let r = (id / spr) as usize;
            out[r][1] += 1;
            let mesh = gpu.reference.mesh();
            if point_inside_mesh(mesh, to_vec(p)) && point_inside_mesh(mesh, to_vec(q)) {
                out[r][0] += 1;
            }
        }
        assert_eq!(recorded, out, "record-all substitution must equal the direct CPU evaluation");
        out
    }

    // AI-FUNC-SUMMARY: Certified GPU MC hit/valid counts equal a CPU f64 evaluation of the identical sample points for fixed seeds on adversarial meshes (coincident duplicates, sub-1e-6 slab, shared faces, faces on grid planes, ray-through-vertex/edge octahedra, tiny features) at the origin and translated by 1e9, across radius batches; the frozen uncertified shader is shown to disagree on the slab.
    #[test]
    fn certified_mc_counts_equal_cpu_on_same_points() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let mut uncertain_total = 0u64;
        for (name, mesh) in adversarial_meshes(0.25) {
            for shift in [0.0, 1e9] {
                let (mesh, bbox) = shifted(&mesh, shift);
                let mut gpu = GpuS2Pipeline::new(&mesh, bbox).unwrap();
                for (seed, r_max, batch) in [(7u32, 3usize, None), (0xdead_beef, 5, Some(2))] {
                    gpu.test_radius_batch = batch;
                    let before = gpu.certification_stats();
                    let certified = gpu.calculate_s2_gpu_counts(bbox, r_max, 3000, seed).unwrap();
                    let after = gpu.certification_stats();
                    let reference = cpu_counts_of_gpu_points(&mut gpu, bbox, r_max, 3000, seed);
                    assert_eq!(certified, reference, "{name} shift={shift} seed={seed}");
                    uncertain_total += after.uncertain - before.uncertain;
                    eprintln!(
                        "MC_CERT mesh={name} shift={shift:e} seed={seed} uncertain={} valid={}",
                        after.uncertain - before.uncertain,
                        after.queries - before.queries
                    );
                    if name == "coincident_duplicates" {
                        assert!(after.uncertain > before.uncertain);
                    }
                }
                gpu.test_radius_batch = None;
                if name == "thin_slab" && shift == 0.0 {
                    let mut baseline = GpuS2Pipeline::new_with_shader(
                        &mesh,
                        bbox,
                        include_str!("../../tests/fixtures/s2_monte_carlo_uncertified.wgsl"),
                    )
                    .unwrap();
                    let raw = baseline.calculate_s2_gpu_counts(bbox, 3, 3000, 7).unwrap();
                    let exact = cpu_counts_of_gpu_points(&mut gpu, bbox, 3, 3000, 7);
                    eprintln!("MC_CERT uncertified thin_slab raw={raw:?} exact={exact:?}");
                    assert_ne!(raw, exact, "the uncertified f32 dedup must be shown to fail here");
                }
            }
        }
        assert!(uncertain_total > 0);
    }

    // AI-FUNC-SUMMARY: Force an uncertain-list overflow from a one-record list; the batch is re-dispatched with a regrown list and yields the same exact counts.
    #[test]
    fn mc_uncertain_list_overflow_regrows() {
        if let Err(error) = super::super::context::try_init_gpu() {
            eprintln!("SKIP: GPU device unavailable: {error:?}");
            return;
        }
        let (_, mesh) = adversarial_meshes(0.25)
            .into_iter()
            .find(|(name, _)| *name == "coincident_duplicates")
            .unwrap();
        let bbox = BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0));
        let mut gpu = GpuS2Pipeline::new(&mesh, bbox).unwrap();
        let expected = gpu.calculate_s2_gpu_counts(bbox, 2, 2000, 11).unwrap();
        super::super::runtime::scoped(&gpu.device.clone(), || {
            let (buffer, staging) = uncertain_buffers(&gpu.device, 1);
            gpu.uncertain_buffer = buffer;
            gpu.uncertain_staging = staging;
            Ok(())
        })
        .unwrap();
        let before = gpu.certification_stats().list_regrowths;
        assert_eq!(gpu.calculate_s2_gpu_counts(bbox, 2, 2000, 11).unwrap(), expected);
        assert_eq!(gpu.certification_stats().list_regrowths, before + 1);
        gpu.release_output_capacity().unwrap();
        assert_eq!(
            gpu.uncertain_buffer.size(),
            crate::compute::mc_memory::mc_uncertain_bytes(crate::compute::mc_memory::MC_UNCERTAIN_INITIAL)
        );
    }

    // AI-FUNC-SUMMARY: Release benchmark: certified versus frozen uncertified MC on an ordinary sphere and data/input/particles.stl with identical seeds; one warmup then five alternating samples, logging seconds and recompute ratios.
    #[test]
    #[ignore = "release MC certification overhead benchmark"]
    fn certification_overhead_benchmark() {
        let sphere = crate::geometry::icosphere_mesh(Vec3::new(2.0, 2.0, 2.0), 1.3, 3);
        let mut cases = vec![("icosphere_l3", sphere, BoundingBox::from_size(Vec3::new(4.0, 4.0, 4.0)), 16usize)];
        if let Ok(particles) = crate::io::load_stl(std::path::Path::new("data/input/particles.stl")) {
            let bbox = crate::geometry::mesh_bbox(&particles).unwrap();
            cases.push(("particles", particles, bbox, 16));
        }
        for (name, mesh, bbox, r_max) in cases {
            let samples = 4000;
            let mut certified = GpuS2Pipeline::new(&mesh, bbox).expect("benchmark requires GPU");
            let mut baseline = GpuS2Pipeline::new_with_shader(
                &mesh,
                bbox,
                include_str!("../../tests/fixtures/s2_monte_carlo_uncertified.wgsl"),
            )
            .unwrap();
            let run = |gpu: &mut GpuS2Pipeline, seed: u32| {
                let start = std::time::Instant::now();
                let counts = gpu.calculate_s2_gpu_counts(bbox, r_max, samples, seed).unwrap();
                (start.elapsed().as_secs_f64(), counts)
            };
            run(&mut certified, 1);
            run(&mut baseline, 1);
            let warm = certified.certification_stats();
            for sample in 0..5u32 {
                let seed = 100 + sample;
                let (c, b) = if sample % 2 == 0 {
                    (run(&mut certified, seed), run(&mut baseline, seed))
                } else {
                    let b = run(&mut baseline, seed);
                    (run(&mut certified, seed), b)
                };
                let differing = c.1.iter().zip(&b.1).filter(|(x, y)| x != y).count();
                eprintln!(
                    "MC_CERT_BENCH mesh={name} faces={} r_max={r_max} samples={samples} sample={sample} certified_seconds={:.6} uncertified_seconds={:.6} radii_with_different_counts={differing}",
                    mesh.faces.len(), c.0, b.0
                );
            }
            let stats = certified.certification_stats();
            eprintln!(
                "MC_CERT_BENCH mesh={name} runs=5 uncertain={} valid={} ratio={:.3e} cpu_recompute={:.6}s",
                stats.uncertain - warm.uncertain,
                stats.queries - warm.queries,
                (stats.uncertain - warm.uncertain) as f64 / (stats.queries - warm.queries) as f64,
                stats.cpu_recompute_seconds - warm.cpu_recompute_seconds
            );
        }
    }
}
