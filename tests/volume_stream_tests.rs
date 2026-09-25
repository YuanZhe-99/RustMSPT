use rayon::ThreadPoolBuilder;
use rustmspt::io::{
    load_raw_folder, load_tiff_or_folder_with_range, save_tiff_or_folder_with_ext, ByteOrder,
    RawFolderSpec, Volume3D, VolumeNumericType,
};

// AI-FUNC-SUMMARY: Verify ordered RAW slice ranges and all supported signed/unsigned bit depths and endian modes under 1/2/8 workers.
#[test]
fn raw_batches_preserve_order_range_and_types() {
    for (bits, signed, kind) in [
        (8, false, VolumeNumericType::U8),
        (8, true, VolumeNumericType::I8),
        (16, false, VolumeNumericType::U16),
        (16, true, VolumeNumericType::I16),
        (32, false, VolumeNumericType::U32),
        (32, true, VolumeNumericType::I32),
    ] {
        for little in [true, false] {
            let dir = tempfile::tempdir().unwrap();
            let mut expected = Vec::new();
            for z in (0..7).rev() {
                let values: Vec<i64> = (0..15)
                    .map(|x| if signed { z * 15 + x - 70 } else { z * 15 + x })
                    .collect();
                let mut bytes = Vec::new();
                for &value in &values {
                    let raw = value as u32;
                    let encoded = if little {
                        raw.to_le_bytes()
                    } else {
                        raw.to_be_bytes()
                    };
                    let count = bits as usize / 8;
                    bytes.extend_from_slice(if little {
                        &encoded[..count]
                    } else {
                        &encoded[4 - count..]
                    });
                }
                std::fs::write(dir.path().join(format!("s{z:03}.raw")), bytes).unwrap();
            }
            for z in 1..=5 {
                for x in 0..15 {
                    expected.push(if signed { z * 15 + x - 70 } else { z * 15 + x });
                }
            }
            let spec = RawFolderSpec {
                folder: dir.path().to_path_buf(),
                width: 5,
                height: 3,
                bits,
                signed,
                byte_order: if little {
                    ByteOrder::LittleEndian
                } else {
                    ByteOrder::BigEndian
                },
                slice_start: 1,
                slice_end: 5,
            };
            for workers in [1, 2, 8] {
                let pool = ThreadPoolBuilder::new()
                    .num_threads(workers)
                    .build()
                    .unwrap();
                let volume = pool.install(|| load_raw_folder(&spec)).unwrap();
                assert_eq!((volume.width, volume.height, volume.depth), (5, 3, 5));
                assert_eq!(volume.numeric_type, kind);
                assert_eq!(volume.data, expected);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Verify folder ranges select whole multi-page TIFF files in lexical order and preserve all numeric types across worker budgets.
#[test]
fn tiff_batches_preserve_multipage_order_and_types() {
    for kind in [
        VolumeNumericType::U8,
        VolumeNumericType::I8,
        VolumeNumericType::U16,
        VolumeNumericType::I16,
        VolumeNumericType::U32,
        VolumeNumericType::I32,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let mut expected = Vec::new();
        for z in 0..5 {
            let data: Vec<i64> = (0..42).map(|x| z * 42 + x).collect();
            // Use values valid even for signed eight-bit TIFF.
            let data: Vec<i64> = data.into_iter().map(|x| x % 120).collect();
            let volume = Volume3D {
                width: 7,
                height: 3,
                depth: 2,
                data,
                numeric_type: kind,
            };
            save_tiff_or_folder_with_ext(
                &volume,
                &dir.path().join(format!("s{z:03}.tiff")),
                None,
                None,
            )
            .unwrap();
            if (1..=3).contains(&z) {
                expected.extend_from_slice(&volume.data);
            }
        }
        for workers in [1, 2, 8] {
            let pool = ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            let volume = pool
                .install(|| load_tiff_or_folder_with_range(dir.path(), 1, 3))
                .unwrap();
            assert_eq!((volume.width, volume.height, volume.depth), (7, 3, 6));
            assert_eq!(volume.numeric_type, kind);
            assert_eq!(volume.data, expected);
        }
    }
}

// AI-FUNC-SUMMARY: Preserve the earliest file's validation error even when another file in the same parallel batch fails decoding first.
#[test]
fn tiff_batches_keep_ordered_validation_errors() {
    let dir = tempfile::tempdir().unwrap();
    for (name, width) in [("0.tif", 2), ("1.tif", 2), ("2.tif", 3)] {
        let volume = Volume3D {
            width,
            height: 2,
            depth: 1,
            data: vec![1; width * 2],
            numeric_type: VolumeNumericType::U8,
        };
        save_tiff_or_folder_with_ext(&volume, &dir.path().join(name), None, None).unwrap();
    }
    std::fs::write(dir.path().join("3.tif"), b"corrupt").unwrap();
    for workers in [1, 2, 8] {
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let error = pool
            .install(|| load_tiff_or_folder_with_range(dir.path(), -1, -1))
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("TIFF shape mismatch") && error.contains("2.tif"),
            "{error}"
        );
    }
}

// AI-FUNC-SUMMARY: Measure warm filesystem RAW and multi-page-file TIFF folder loading at one versus two decode workers, with complete voxel equality and fixed fixtures; excludes fixture generation and pool creation.
#[test]
#[ignore = "release bounded volume IO benchmark"]
fn volume_file_batches_benchmark() {
    for (size, depth) in [(256, 64), (512, 16), (1024, 4)] {
        let raw = tempfile::tempdir().unwrap();
        let tiff = tempfile::tempdir().unwrap();
        let plane: Vec<i64> = (0..size * size)
            .map(|i| ((i * 37) % 65536) as i64)
            .collect();
        let bytes: Vec<u8> = plane
            .iter()
            .flat_map(|&x| (x as u16).to_le_bytes())
            .collect();
        for z in 0..depth {
            std::fs::write(raw.path().join(format!("s{z:03}.raw")), &bytes).unwrap();
        }
        let volume = Volume3D {
            width: size,
            height: size,
            depth: 4,
            data: plane.repeat(4),
            numeric_type: VolumeNumericType::U16,
        };
        for z in 0..depth / 4 {
            save_tiff_or_folder_with_ext(
                &volume,
                &tiff.path().join(format!("s{z:03}.tif")),
                None,
                None,
            )
            .unwrap();
        }
        let expected = plane.repeat(depth);
        let spec = RawFolderSpec {
            folder: raw.path().to_path_buf(),
            width: size,
            height: size,
            bits: 16,
            signed: false,
            byte_order: ByteOrder::LittleEndian,
            slice_start: -1,
            slice_end: -1,
        };
        let serial = ThreadPoolBuilder::new().num_threads(1).build().unwrap();
        let parallel = ThreadPoolBuilder::new().num_threads(2).build().unwrap();
        for format in ["raw", "tiff"] {
            let run = |pool: &rayon::ThreadPool| {
                let start = std::time::Instant::now();
                let loaded = pool
                    .install(|| {
                        if format == "raw" {
                            load_raw_folder(&spec)
                        } else {
                            load_tiff_or_folder_with_range(tiff.path(), -1, -1)
                        }
                    })
                    .unwrap();
                let elapsed = start.elapsed().as_secs_f64();
                assert_eq!(loaded.data, expected);
                assert_eq!(loaded.depth, depth);
                elapsed
            };
            run(&serial);
            run(&parallel);
            for sample in 0..5 {
                let (old, new) = if sample % 2 == 0 {
                    (run(&serial), run(&parallel))
                } else {
                    let new = run(&parallel);
                    (run(&serial), new)
                };
                eprintln!("VOLUME_BATCH_BENCH format={format} size={size} depth={depth} sample={sample} serial={old:.9} bounded={new:.9}");
            }
        }
    }
}

// AI-FUNC-SUMMARY: Exercise production parallel RAW decoding above the 512-KiB cutoff with a ragged final batch and verify every voxel in filename order.
#[test]
fn large_raw_batches_preserve_every_voxel() {
    let dir = tempfile::tempdir().unwrap();
    let pixels = 513 * 512;
    for z in 0..5u16 {
        let bytes: Vec<u8> = (0..pixels)
            .flat_map(|i| ((i as u16).wrapping_add(z)).to_be_bytes())
            .collect();
        std::fs::write(dir.path().join(format!("{z:03}.raw")), bytes).unwrap();
    }
    let spec = RawFolderSpec {
        folder: dir.path().to_path_buf(),
        width: 513,
        height: 512,
        bits: 16,
        signed: false,
        byte_order: ByteOrder::BigEndian,
        slice_start: -1,
        slice_end: -1,
    };
    for workers in [1, 2, 8] {
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let volume = pool.install(|| load_raw_folder(&spec)).unwrap();
        assert_eq!(volume.depth, 5);
        for (z, slice) in volume.data.chunks(pixels).enumerate() {
            for (i, &value) in slice.iter().enumerate() {
                assert_eq!(value, (i as u16).wrapping_add(z as u16) as i64);
            }
        }
    }
}

// AI-FUNC-SUMMARY: Compare exact TIFF file bytes, fixed names and decoded values across bounded writer worker counts for every numeric type and an odd slice count.
#[test]
fn tiff_folder_writers_preserve_bytes_and_names() {
    for kind in [
        VolumeNumericType::U8,
        VolumeNumericType::I8,
        VolumeNumericType::U16,
        VolumeNumericType::I16,
        VolumeNumericType::U32,
        VolumeNumericType::I32,
    ] {
        let dir = tempfile::tempdir().unwrap();
        let signed = matches!(
            kind,
            VolumeNumericType::I8 | VolumeNumericType::I16 | VolumeNumericType::I32
        );
        let data: Vec<i64> = (0..30).map(|i| if signed { i - 15 } else { i }).collect();
        let volume = Volume3D {
            width: 3,
            height: 2,
            depth: 5,
            data,
            numeric_type: kind,
        };
        let mut reference = None;
        for workers in [1, 2, 8] {
            let output = dir.path().join(workers.to_string());
            let pool = ThreadPoolBuilder::new()
                .num_threads(workers)
                .build()
                .unwrap();
            pool.install(|| {
                save_tiff_or_folder_with_ext(&volume, &output, Some("scan"), Some("TIF"))
            })
            .unwrap();
            let bytes: Vec<Vec<u8>> = (0..5)
                .map(|z| std::fs::read(output.join(format!("scan_{z:04}.tif"))).unwrap())
                .collect();
            assert_eq!(std::fs::read_dir(&output).unwrap().count(), 5);
            if let Some(ref expected) = reference {
                assert_eq!(&bytes, expected);
            } else {
                reference = Some(bytes);
            }
            let restored = load_tiff_or_folder_with_range(&output, -1, -1).unwrap();
            assert_eq!(restored.data, volume.data);
            assert_eq!(restored.numeric_type, kind);
        }
    }
}

// AI-FUNC-SUMMARY: Ensure parallel TIFF write errors keep slice order, do not launch later batches and reject overflowing dimensions before creating output.
#[test]
fn tiff_folder_writer_errors_are_ordered_and_bounded() {
    for workers in [1, 2, 8] {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out");
        std::fs::create_dir_all(output.join("slice_0001.tiff")).unwrap();
        let volume = Volume3D {
            width: 1,
            height: 1,
            depth: 5,
            data: vec![256, 1, 2, 3, 4],
            numeric_type: VolumeNumericType::U8,
        };
        let pool = ThreadPoolBuilder::new()
            .num_threads(workers)
            .build()
            .unwrap();
        let error = pool
            .install(|| save_tiff_or_folder_with_ext(&volume, &output, None, None))
            .unwrap_err();
        assert!(
            error.to_string().contains("Value out of range for u8"),
            "{error}"
        );
        for z in 2..5 {
            assert!(!output.join(format!("slice_{z:04}.tiff")).exists());
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("overflow");
    let volume: Volume3D = Volume3D {
        width: usize::MAX,
        height: 2,
        depth: 1,
        data: vec![],
        numeric_type: VolumeNumericType::U8,
    };
    assert!(save_tiff_or_folder_with_ext(&volume, &output, None, None)
        .unwrap_err()
        .to_string()
        .contains("dimensions overflow"));
    assert!(!output.exists());
}

// AI-FUNC-SUMMARY: Measure warmed TIFF folder encoding/writing at one versus two workers, preserving exact file bytes; exclude fixture creation, pool construction and output comparisons, without fsync/durability claims.
#[test]
#[ignore = "release bounded TIFF writer benchmark"]
fn tiff_folder_writer_benchmark() {
    let serial = ThreadPoolBuilder::new().num_threads(1).build().unwrap();
    let parallel = ThreadPoolBuilder::new().num_threads(2).build().unwrap();
    for (size, depth) in [(32, 64), (256, 64), (1024, 4)] {
        let dir = tempfile::tempdir().unwrap();
        let data: Vec<i64> = (0..size * size * depth)
            .map(|i| ((i * 37) % 65536) as i64)
            .collect();
        let volume = Volume3D {
            width: size,
            height: size,
            depth,
            data,
            numeric_type: VolumeNumericType::U16,
        };
        let reference = dir.path().join("serial");
        let candidate = dir.path().join("bounded");
        let run = |pool: &rayon::ThreadPool, out: &std::path::Path| {
            let start = std::time::Instant::now();
            pool.install(|| save_tiff_or_folder_with_ext(&volume, out, None, None))
                .unwrap();
            start.elapsed().as_secs_f64()
        };
        run(&serial, &reference);
        run(&parallel, &candidate);
        for sample in 0..5 {
            let (old, new) = if sample % 2 == 0 {
                (run(&serial, &reference), run(&parallel, &candidate))
            } else {
                let new = run(&parallel, &candidate);
                (run(&serial, &reference), new)
            };
            for z in 0..depth {
                let file = format!("slice_{z:04}.tiff");
                assert_eq!(
                    std::fs::read(reference.join(&file)).unwrap(),
                    std::fs::read(candidate.join(&file)).unwrap()
                );
            }
            eprintln!("TIFF_WRITER_BENCH size={size} depth={depth} sample={sample} serial={old:.9} bounded={new:.9}");
        }
    }
}

// AI-FUNC-SUMMARY: Reject overflowing RAW dimensions/byte counts/output lengths without allocating or reading huge files, while preserving the first malformed-slice error before reservation.
#[test]
fn raw_output_planning_rejects_overflow_without_large_allocation() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["0.raw", "1.raw", "2.raw"] {
        std::fs::write(dir.path().join(name), [0u8]).unwrap();
    }
    for (width, height, bits, message) in [
        (usize::MAX, 2, 8, "RAW slice dimensions overflow"),
        (usize::MAX / 2 + 1, 1, 32, "RAW slice byte count overflow"),
        (usize::MAX / 2, 1, 8, "RAW volume dimensions overflow"),
        (512, 512, 16, "RAW slice size mismatch"),
    ] {
        let spec = RawFolderSpec {
            folder: dir.path().to_path_buf(),
            width,
            height,
            bits,
            signed: false,
            byte_order: ByteOrder::LittleEndian,
            slice_start: -1,
            slice_end: -1,
        };
        let error = load_raw_folder(&spec).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
        if message.contains("mismatch") {
            assert!(error.contains("0.raw"));
        }
    }
}
