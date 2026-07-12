//! Reports near-64-MiB compact arena codec and future-ID replay measurements.

use std::error::Error;
use std::rc::Rc;
use std::time::Instant;

use serde_json::json;
use visualizer_core::arena::{
    ArenaEncodeContinuation, EncodeProgress, GenerationalArena, RestoreProgress,
};

const SLOT_COUNT: u64 = 5_000_000;
const SLICE_RECORDS: usize = 4_096;
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;

fn encode(mut continuation: ArenaEncodeContinuation<'_>) -> Result<Vec<u8>, Box<dyn Error>> {
    loop {
        match continuation.resume(SLICE_RECORDS)? {
            EncodeProgress::Pending => {}
            EncodeProgress::Complete(bytes) => return Ok(bytes),
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let build_started = Instant::now();
    let mut uninterrupted = GenerationalArena::new();
    uninterrupted.try_reserve(usize::try_from(SLOT_COUNT)?)?;
    let mut keys = Vec::new();
    keys.try_reserve(usize::try_from(SLOT_COUNT)?)?;
    for value in 0..SLOT_COUNT {
        keys.push(uninterrupted.try_insert(value)?);
    }

    for key in keys.iter().step_by(3) {
        let _ = uninterrupted.remove(*key);
    }
    for value in SLOT_COUNT..SLOT_COUNT + SLOT_COUNT.div_ceil(3) {
        let _ = uninterrupted.try_insert(value)?;
    }
    for key in keys.iter().step_by(50) {
        let _ = uninterrupted.remove(*key);
    }
    let build_milliseconds = build_started.elapsed().as_millis();

    let snapshot_started = Instant::now();
    let snapshot = uninterrupted.snapshot();
    let snapshot_milliseconds = snapshot_started.elapsed().as_millis();

    let encode_started = Instant::now();
    let bytes = encode(GenerationalArena::begin_encode_snapshot(&snapshot)?)?;
    let encode_milliseconds = encode_started.elapsed().as_millis();
    let checkpoint_fits = bytes.len() <= MAX_CHECKPOINT_BYTES;

    let restore_started = Instant::now();
    let mut continuation = GenerationalArena::begin_restore_snapshot(Rc::from(bytes.clone()))?;
    let mut restored = loop {
        match continuation.resume(SLICE_RECORDS)? {
            RestoreProgress::Pending => {}
            RestoreProgress::Complete(arena) => break arena,
        }
    };
    let restore_milliseconds = restore_started.elapsed().as_millis();

    let mut future_ids_match = true;
    for value in 10_000_000..10_100_000 {
        if uninterrupted.try_insert(value)? != restored.try_insert(value)? {
            future_ids_match = false;
            break;
        }
    }
    let passed = checkpoint_fits && future_ids_match && uninterrupted.len() == restored.len();
    let report = json!({
        "artifactSchemaVersion": 1,
        "component": "project-owned-arena",
        "status": if passed { "passed" } else { "failed" },
        "fixture": {
            "slotCount": SLOT_COUNT,
            "sliceRecords": SLICE_RECORDS,
            "futureInsertCount": 100_000,
            "checkpointByteLimit": MAX_CHECKPOINT_BYTES
        },
        "observed": {
            "checkpointBytes": bytes.len(),
            "occupiedAfterContinuation": uninterrupted.len(),
            "futureIdsMatch": future_ids_match,
            "buildMilliseconds": build_milliseconds,
            "snapshotMilliseconds": snapshot_milliseconds,
            "encodeMilliseconds": encode_milliseconds,
            "restoreMilliseconds": restore_milliseconds
        }
    });
    serde_json::to_writer_pretty(std::io::stdout().lock(), &report)?;
    println!();

    if passed {
        Ok(())
    } else {
        Err("arena report validation failed".into())
    }
}
