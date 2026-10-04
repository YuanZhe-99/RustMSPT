# Aggregate Pack target search and mixed stages

- Implemented: optional mode remains disabled; primary and mixed modes; integer/auto variants.
- Implemented: true internal VF target, finite strategy/trial budget, deterministic rotations/lateral/pair moves and shrinking objective; preserve best valid template on stop.
- Implemented: global*local transform reconstruction and separate internal/global status.
- Implemented: smaller-block fallback with reserved global attempt budget and explicit stages/draw accounting.
- Implemented: repository data/input config templates and independent A configs.
- Validation: 13 aggregate tests passed, including independent geometry/clearance/reconstruction, thread determinism, mixed fallback and auto count. Broader placement regression: 121 distinct tests passed. Actual-shape 16/32/64 comparison complete; max internal VF 28.16%, all miss requested 50% and retain best valid templates.
- Formal 40% RVE is not started. Target density remains an optimization request, not a guarantee.
