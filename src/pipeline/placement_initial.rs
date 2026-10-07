//! Validate a prior accepted assembly before starting a new geometry-based fill.
use super::*;
use crate::pipeline::placement_sizes::class_for_diameter;

// AI-FUNC-SUMMARY: Describe a refused initial-assembly import; no mutation.
fn invalid(message: impl Into<String>) -> RustMsptError {
    RustMsptError::InvalidConfig(format!("initial_particles: {}", message.into()))
}

// AI-FUNC-SUMMARY: Compare finite reconstructed values with a small relative/absolute tolerance.
fn close(a: f64, b: f64) -> bool {
    a.is_finite() && b.is_finite() && (a - b).abs() <= 1e-8 * b.abs().max(1.0)
}

// AI-FUNC-SUMMARY: Authenticate source/record/pore identities, reconstruct and independently check every existing particle at the explicitly declared inherited gap; retain exact order/transforms and plan only new material.
pub(super) fn load(
    config: &ResolvedPlacement,
    library: &ShapeLibrary,
    classes: &[SizeClass],
    void: Option<&VoidIndex>,
    state: &mut EngineState,
) -> Result<()> {
    let spec = config.initial_particles.as_ref().unwrap();
    let record = read_record(&spec.record)?;
    let report = read_report(&spec.report)?;
    let record_hash = sha256_file(&spec.record)?.0;
    if record.schema_version != RECORD_SCHEMA || report.schema_version != REPORT_SCHEMA {
        return Err(invalid("unsupported record/report schema"));
    }
    if !report
        .outputs
        .iter()
        .any(|entry| entry.role == "record" && entry.sha256.as_ref() == Some(&record_hash))
    {
        return Err(invalid("record digest does not match its original report"));
    }
    if record.frame.unit != config.unit
        || record.frame.axis_order != "xyz"
        || record.frame.handedness != "right"
        || record.frame.domain.min
            != [
                config.domain.min.x,
                config.domain.min.y,
                config.domain.min.z,
            ]
        || record.frame.domain.max
            != [
                config.domain.max.x,
                config.domain.max.y,
                config.domain.max.z,
            ]
        || record.sources.len() != library.sources.len()
        || record.particles.len() > MAX_PLANNED_PARTICLES
        || record.particles.len() != report.actual.particles
    {
        return Err(invalid("frame, source count or particle count differs"));
    }
    for (source, current) in record.sources.iter().zip(&library.sources) {
        if source.index != current.index || source.sha256 != current.sha256 {
            return Err(invalid("source geometry or ordering changed"));
        }
    }
    match (&config.void, &report.void) {
        (None, None) => {}
        (Some(current), Some(previous)) if sha256_file(&current.file)?.0 == previous.sha256_in => {}
        _ => return Err(invalid("frozen void differs")),
    }
    let mut validation = config.clone();
    validation.gap_particle_particle = spec.existing_gap;
    state.control.phase = "validating_initial_particles";
    for (index, particle) in record.particles.iter().enumerate() {
        let shell_index = library
            .shells
            .iter()
            .position(|shell| {
                shell.source_index == particle.source_shape.source_index
                    && shell.shell_index == particle.source_shape.shell_index
                    && shell.shell_sha256 == particle.source_shape.shell_sha256
            })
            .ok_or_else(|| invalid("source shell not found"))?;
        let shell = &library.shells[shell_index];
        let q = particle.rotation.quaternion;
        if particle.acceptance_index != index
            || particle.clipped.any
            || !particle.clipped.faces.is_empty()
            || particle.void_overlap_volume != 0.0
            || !particle.scale.is_finite()
            || particle.scale <= 0.0
            || !q.iter().all(|v| v.is_finite())
            || !close(q.iter().map(|v| v * v).sum(), 1.0)
            || !particle.translation.iter().all(|v| v.is_finite())
            || !close(
                particle.equivalent_diameter,
                shell.equivalent_diameter * particle.scale,
            )
            || particle.size_class != class_for_diameter(classes, particle.equivalent_diameter)
            || particle.source_shape.sha256 != library.sources[shell.source_index].sha256
        {
            return Err(invalid(format!(
                "invalid transform/metadata for particle {index}"
            )));
        }
        let proposal = Proposal {
            shell_index,
            scale: particle.scale,
            rotation: UnitQuat {
                w: q[0],
                x: q[1],
                y: q[2],
                z: q[3],
            },
            centre: Vec3::new(
                particle.translation[0],
                particle.translation[1],
                particle.translation[2],
            ),
            reach: shell.bounding_radius * particle.scale,
            fits_domain: true,
            stream_after: 0,
            guided_cell: None,
        };
        let mut evaluated = match evaluate_proposal(
            &validation,
            library,
            void,
            &state.placed,
            &state.grid,
            None,
            &proposal,
        ) {
            Evaluation::Rejected(reason) => {
                return Err(invalid(format!("particle {index} fails {reason:?}")))
            }
            Evaluation::Accepted(value) => value,
        };
        let bbox = evaluated.bbox;
        if !close(particle.volume.full, evaluated.volume_full)
            || !close(
                particle.volume.in_domain,
                evaluated.accepted.volume_in_domain,
            )
            || !close(
                particle.volume.in_domain_solid,
                evaluated.accepted.volume_in_domain,
            )
            || particle.triangle_range[0] != state.triangle_count
            || particle.triangle_range[1].checked_sub(particle.triangle_range[0])
                != Some(shell.canonical.faces.len())
            || !particle
                .bbox
                .min
                .iter()
                .zip([bbox.min.x, bbox.min.y, bbox.min.z])
                .all(|(&a, b)| close(a, b))
            || !particle
                .bbox
                .max
                .iter()
                .zip([bbox.max.x, bbox.max.y, bbox.max.z])
                .all(|(&a, b)| close(a, b))
        {
            return Err(invalid(format!(
                "particle {index} geometry/accounting differs"
            )));
        }
        // Preserve verified f64 accounting exactly; equivalent multiplication
        // orders in a previous executable may differ in the last bit.
        evaluated.accepted.volume_in_domain = particle.volume.in_domain;
        let draw = SizeDraw {
            diameter: particle.equivalent_diameter,
            class: particle.size_class,
            draw_index: index,
        };
        accept(
            state,
            shell_index,
            library,
            &draw,
            proposal.scale,
            proposal.rotation,
            proposal.centre,
            proposal.reach,
            particle.volume.full,
            bbox,
            0.0,
            evaluated.candidate,
            evaluated.accepted,
        );
        state.drawn_per_class[draw.class] += 1;
        state.placed_per_class[draw.class] += 1;
        if index % 100 == 0 {
            eprintln!(
                "[InitialParticles] validated={}/{}",
                index + 1,
                record.particles.len()
            );
            state.control.poll(
                config,
                true,
                state.placed.len(),
                state.attempts,
                state.volume_solid / state.basis_volume,
            );
        }
    }
    if !close(state.volume_solid, report.actual.volume_in_domain_solid) {
        return Err(invalid("imported total material volume differs"));
    }
    state.control.phase = "packing";
    eprintln!(
        "[InitialParticles] retained={} volume_solid={}",
        state.placed.len(),
        state.volume_solid
    );
    Ok(())
}
