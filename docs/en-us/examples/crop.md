# `crop` pipeline example

Current builds additionally emit `[Timing] crop stage=<name> seconds=<value>` for completed stages: `load`, `background`, `pca`, `transform_and_backend`, `trim`, `encode_write`, and `total_in_pool`. The captured output below predates these lines. Backend timing includes planning, initialization, transfers and permitted fallback; write timing includes flush but not fsync. Pool total excludes CLI/configuration/pool creation; values depend on the run.

## What it does

`crop` loads a raw CT slice stack (or a TIFF stack), thresholds it against an auto-detected
background level, finds the foreground volume's principal axes via PCA, rotates the volume to
align those axes with X/Y/Z, and crops it to the tightest axis-aligned bounding box (with an
optional edge trim to remove rotation-induced jagged borders). It is typically the first step in a
pipeline that starts from a raw tomography scan and needs a clean, axis-aligned volume before
further measurement or forging.

## Config

`data/input/crop_config.yaml` (the repository's real default config for this pipeline):

```yaml
input:
  # Options: raw | tiff
  type: "raw"
  # RAW: folder path. TIFF: file path or folder path.
  path: "data/input/ct_stack/"
  # Shared slice range for RAW/TIFF. Inclusive indices. -1 means start/end.
  slice_start: -1
  slice_end: -1

  # Required when type=raw.
  raw:
    width: 744
    height: 789
    bits: 16
    signed: false
    # little | big
    byte_order: "little"

output:
  # Output can be TIFF file (.tif/.tiff) or folder path.
  path: "data/output/cropped_ct.tiff"
  # Used only when output.path is a folder.
  folder_prefix: "crop"
  # Used only when output.path is a folder. Options: tif | tiff
  folder_extension: "tiff"

# Resampling mode during rotation: nearest | trilinear
interpolation: "trilinear"

# Optional XY border trim after rotation/crop to suppress jagged edges.
# -1: auto infer (0~2), 0: off, 1/2: manual trim pixels.
edge_trim: -1
```

`input.path` points at `data/input/ct_stack/`, a small real RAW CT slice sequence checked into the
repository (`IN_0001.raw`, `IN_0002.raw`, `IN_0003.raw` — three 744x789, 16-bit unsigned,
little-endian slices, ~1.12 MB each). This walkthrough uses that config unmodified, only
overriding `output.path` on the command line so nothing is written under `data/output/`.

| Field | Meaning |
|---|---|
| `input.type` | `"raw"` reads a folder of headerless `.raw` slices using `input.raw.*` to interpret bytes; `"tiff"` reads a TIFF file or folder of TIFF slices directly. |
| `input.raw.width`/`height`/`bits`/`signed`/`byte_order` | Required only for `type: "raw"`; describe how to reinterpret the raw bytes of each slice as a 2D pixel grid. |
| `input.slice_start`/`slice_end` | Inclusive slice index range; `-1` means "from the first slice" / "to the last slice". |
| `interpolation` | Resampling mode used when rotating the volume to align it with the PCA axes: `"nearest"` (fast, blocky) or `"trilinear"` (smoother, default here). |
| `edge_trim` | Extra XY border pixels trimmed after rotation/crop to remove interpolation artifacts at the rotated volume's edges. `-1` lets the pipeline infer 0–2 pixels automatically from the rotation angle. |

See `../reference/config.md` (`CropConfig` section) for the full field reference, and
`../algorithms/pca-volume-alignment-crop.md` for the PCA alignment and bounding-box algorithm this
pipeline implements.

## Running it

```bash
./target/release/rustmspt crop \
  --config data/input/crop_config.yaml \
  --output /tmp/rustmspt-doc-examples/cropped_ct.tiff
```

`--output` overrides `output.path` from the loaded YAML before the pipeline runs (see
`src/main.rs`'s `Commands::Crop` arm), so the real `data/input/crop_config.yaml` and
`data/input/ct_stack/` sample data can be used as-is without touching `data/output/`.

## Expected output

Real captured stdout from the run above:

```
[Info] Crop input loaded: shape=(744,789,3)
[Info] Background value detected: 0
[Info] Interpolation mode: Trilinear
[Info] Foreground voxels: 768151
[Info] Rotated bbox: min=(-364.546,-175.598,-1.000) max=(364.546,175.600,1.000)
[Info] Rotated integer bbox: x=[-365,365] y=[-176,176] z=[-1,1]
[Info] Edge trim pixels (xy): 2
[Info] Cropped output shape=(727,349,3)
[Info] Crop pipeline completed. Output written: /tmp/rustmspt-doc-examples/cropped_ct.tiff
```

The run completed in about 60 ms on this machine (`real 0m0.062s`) — the three-slice sample stack
is tiny compared to production CT stacks with hundreds of slices, so timing is not representative
of full-size runs.

The written file is a real 16-bit, little-endian, uncompressed multi-page TIFF:

```
/tmp/rustmspt-doc-examples/cropped_ct.tiff: TIFF image data, little-endian, direntries=14,
width=727, height=349, bps=16, compression=none, PhotometricInterpretation=BlackIsZero
```

Reading the log top to bottom: the loader reports the input volume shape as `(744, 789, 3)`
(width, height, slice count matching `input.raw.width`/`height` and the three files in
`ct_stack/`), auto-detects a background pixel value of `0`, thresholds against it to find `768151`
foreground voxels, computes the PCA-aligned rotated bounding box (`x=[-365,365] y=[-176,176]
z=[-1,1]` in the rotated frame), infers an edge trim of `2` pixels for this rotation angle, and
finally writes a cropped `727 x 349 x 3` volume — smaller than the original `744 x 789` in-plane
size because the rotated+trimmed bounding box is tighter than the original frame.

## Notes

- Since `edge_trim: -1`, the pipeline auto-selected a trim of 2 pixels for this rotation; setting
  `edge_trim: 0` disables trimming entirely (useful when the foreground touches the RAW slice
  border and any trim would clip real data), while `1`/`2` force a fixed manual trim.
- `input.type: "tiff"` accepts either a single multi-page TIFF file or a folder of single-page
  TIFF slices in place of the RAW-folder input used here — useful when the CT stack was already
  exported as TIFF rather than raw binary slices.
- Background detection and the PCA rotation are described in detail in
  `../algorithms/pca-volume-alignment-crop.md`; that document also explains how
  `edge_trim: -1`'s auto-inference heuristic is derived from the rotation angle.
- For the full `CropPipeline::run` behavior — including how `output.path` folder-vs-file mode is
  decided and how `folder_prefix`/`folder_extension` are used when writing per-slice TIFFs — see
  `../reference/pipeline-crop-and-splitfilter.md`.
