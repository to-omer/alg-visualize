//! Scenario-driven WebAssembly session adapter.

#![forbid(unsafe_code)]

use std::io::{self, Write};

use ordered_map::{
    AlgorithmInstance, CanonicalSnapshot, Operation as ModelOperation, OperationResult, OrderedMap,
    StatePatchRecord, StructureSnapshot, TraceEvent,
};
use serde::Serialize;
use visualizer_core::dsl::{parse_initial, parse_operations, validate_document_size};
use visualizer_core::generator::{
    InitialGeneratorSpec, OperationGeneratorSpec, generate_initial, generate_operations,
};
use visualizer_core::jcs::canonicalize;
use visualizer_core::scenario::Entry;
use visualizer_core::scenario::{Operation, ScenarioV1, decode_ordered_map};
use wasm_bindgen::prelude::*;

/// Strictly validates and RFC 8785-canonicalizes a Scenario.
///
/// # Errors
///
/// Rejects invalid JSON, unsupported contracts, and noncanonical bounded values.
#[wasm_bindgen]
pub fn canonical_scenario_json(source: &str) -> Result<String, JsError> {
    let scenario =
        decode_ordered_map(source.as_bytes()).map_err(|error| JsError::new(&error.to_string()))?;
    canonicalize_scenario(&scenario)
}

fn canonicalize_scenario(scenario: &ScenarioV1) -> Result<String, JsError> {
    let encoded = serde_json::to_vec(scenario).map_err(|error| JsError::new(&error.to_string()))?;
    let canonical = canonicalize(&encoded).map_err(|error| JsError::new(&error.to_string()))?;
    String::from_utf8(canonical).map_err(|error| JsError::new(&error.to_string()))
}

/// Validates an explicitly edited Scenario and declares the derived revisions
/// produced by this build.
///
/// # Errors
///
/// Rejects invalid Scenario input or canonical serialization failure.
#[wasm_bindgen]
pub fn canonical_edited_scenario_json(source: &str) -> Result<String, JsError> {
    let mut scenario =
        decode_ordered_map(source.as_bytes()).map_err(|error| JsError::new(&error.to_string()))?;
    scenario.declare_current_derived_revisions();
    canonicalize_scenario(&scenario)
}

/// Returns whether an untouched imported Scenario declares historical derived
/// output that this build does not reproduce.
///
/// # Errors
///
/// Rejects invalid Scenario input.
#[wasm_bindgen]
pub fn scenario_has_legacy_derived_revisions(source: &str) -> Result<bool, JsError> {
    let scenario =
        decode_ordered_map(source.as_bytes()).map_err(|error| JsError::new(&error.to_string()))?;
    Ok(scenario.has_legacy_derived_revisions())
}

/// Parses strict initial-entry DSL and returns a JSON entry array.
///
/// # Errors
///
/// Returns a stable source diagnostic for invalid DSL.
#[wasm_bindgen]
pub fn parse_initial_dsl_json(source: &str) -> Result<String, JsError> {
    let entries = parse_initial(source.as_bytes()).map_err(|error| {
        JsError::new(&serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()))
    })?;
    serde_json::to_string(&entries).map_err(|error| JsError::new(&error.to_string()))
}

/// Parses strict operation DSL and returns a JSON operation array.
///
/// # Errors
///
/// Returns a stable source diagnostic for invalid DSL.
#[wasm_bindgen]
pub fn parse_operations_dsl_json(source: &str) -> Result<String, JsError> {
    let operations = parse_operations(source.as_bytes()).map_err(|error| {
        JsError::new(&serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()))
    })?;
    serde_json::to_string(&operations).map_err(|error| JsError::new(&error.to_string()))
}

/// Validates the shared byte budget of the initial and operation DSL streams.
///
/// # Errors
///
/// Returns a stable resource-limit diagnostic when their combined UTF-8 bytes
/// exceed the manual-input limit.
#[wasm_bindgen]
pub fn validate_dsl_document_size(initial: &str, operations: &str) -> Result<(), JsError> {
    validate_document_size(initial.as_bytes(), operations.as_bytes()).map_err(|error| {
        JsError::new(&serde_json::to_string(&error).unwrap_or_else(|_| error.to_string()))
    })
}

/// Materializes an initial generator spec and its provenance.
///
/// # Errors
///
/// Rejects invalid or infeasible generator settings.
#[wasm_bindgen]
pub fn generate_initial_json(spec_json: &str) -> Result<String, JsError> {
    let spec: InitialGeneratorSpec =
        serde_json::from_str(spec_json).map_err(|error| JsError::new(&error.to_string()))?;
    let generated = generate_initial(&spec).map_err(|error| JsError::new(&error.to_string()))?;
    serde_json::to_string(&generated).map_err(|error| JsError::new(&error.to_string()))
}

/// Materializes an operation generator spec from an initial entry sequence.
///
/// # Errors
///
/// Rejects invalid initial JSON and invalid or infeasible generator settings.
#[wasm_bindgen]
pub fn generate_operations_json(spec_json: &str, initial_json: &str) -> Result<String, JsError> {
    let spec: OperationGeneratorSpec =
        serde_json::from_str(spec_json).map_err(|error| JsError::new(&error.to_string()))?;
    let initial: Vec<Entry> =
        serde_json::from_str(initial_json).map_err(|error| JsError::new(&error.to_string()))?;
    let generated =
        generate_operations(&spec, &initial).map_err(|error| JsError::new(&error.to_string()))?;
    serde_json::to_string(&generated).map_err(|error| JsError::new(&error.to_string()))
}

/// One reversible operation delta returned to the Worker.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionFrame {
    base_item_index: usize,
    item_index: usize,
    item_count: usize,
    initial_build: bool,
    result: OperationResult,
    trace: Vec<TraceEvent>,
    patches: Vec<StatePatchRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentSessionFrame {
    item_index: usize,
    item_count: usize,
    structure: StructureSnapshot,
    canonical: CanonicalSnapshot,
}

#[derive(Clone, Debug)]
struct WorkItem {
    initial_build: bool,
    operation: ModelOperation,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SeekProgress {
    cursor: usize,
    done: bool,
    target: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame: Option<CurrentSessionFrame>,
}

const CHECKPOINT_INTERVAL: usize = 2_048;
const MAX_CHECKPOINTS: usize = 32;
const MAX_CHECKPOINT_BYTES: usize = 64 * 1024 * 1024;
const MAX_FRAME_JSON_BYTES: usize = 32 * 1024 * 1024 - 16;

struct BoundedJsonWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl BoundedJsonWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::with_capacity(16 * 1024),
            limit,
        }
    }
}

impl Write for BoundedJsonWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(buffer.len()) > self.limit {
            return Err(io::Error::other("frame JSON byte limit exceeded"));
        }
        self.bytes
            .try_reserve(buffer.len())
            .map_err(|_| io::Error::other("frame JSON allocation failed"))?;
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn serialize_frame(value: &impl Serialize) -> Result<String, JsError> {
    serialize_frame_with_limit(value, MAX_FRAME_JSON_BYTES).map_err(|error| JsError::new(&error))
}

fn serialize_frame_with_limit(value: &impl Serialize, limit: usize) -> Result<String, String> {
    let mut writer = BoundedJsonWriter::new(limit);
    serde_json::to_writer(&mut writer, value).map_err(|error| error.to_string())?;
    String::from_utf8(writer.bytes).map_err(|error| error.to_string())
}

#[derive(Clone, Debug)]
struct Checkpoint {
    cursor: usize,
    estimated_bytes: usize,
    algorithm: AlgorithmInstance,
}

#[derive(Clone, Copy, Debug)]
struct StagedNext;

#[derive(Clone, Debug)]
struct StagedSeek {
    algorithm: AlgorithmInstance,
    cursor: usize,
    target: usize,
}

/// Stateful, statically dispatched ordered-map session owned by one Worker job.
#[wasm_bindgen]
pub struct WasmSession {
    scenario: ScenarioV1,
    algorithm: AlgorithmInstance,
    work: Vec<WorkItem>,
    cursor: usize,
    staged_seek: Option<StagedSeek>,
    checkpoints: Vec<Checkpoint>,
    checkpoint_bytes: usize,
    index_algorithm: Option<AlgorithmInstance>,
    index_cursor: usize,
    staged_next: Option<StagedNext>,
}

#[wasm_bindgen]
impl WasmSession {
    /// Strictly decodes a Scenario and creates its selected algorithm instance.
    ///
    /// When `show_build` is false, initial inserts are applied before this
    /// constructor returns and are not exposed as timeline items.
    ///
    /// # Errors
    ///
    /// Returns a JavaScript error for invalid JSON, unsupported revisions,
    /// noncanonical keys, invalid configuration, or bounded runtime failure.
    #[wasm_bindgen(constructor)]
    pub fn new(scenario_json: &str) -> Result<WasmSession, JsError> {
        let scenario = decode_ordered_map(scenario_json.as_bytes())
            .map_err(|error| JsError::new(&error.to_string()))?;
        Self::from_scenario(scenario).map_err(|error| JsError::new(&error))
    }

    /// Stable selected algorithm identifier.
    pub fn algorithm_id(&self) -> String {
        self.algorithm.id().to_owned()
    }

    /// Number of visible timeline items.
    pub fn item_count(&self) -> usize {
        self.work.len()
    }

    /// Number of items committed since the current reset/seek.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Highest timeline boundary covered by the background seek index.
    pub fn seek_coverage(&self) -> usize {
        self.index_cursor
    }

    /// Serializes the current full scene without applying another operation.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn current_frame_json(&self) -> Result<String, JsError> {
        serialize_frame(&self.current_frame()?)
    }

    /// Applies and serializes the next visible item without publishing its cursor.
    ///
    /// Returns `undefined` after the end boundary. The algorithm is advanced only
    /// inside the Worker's synchronous request turn; the committed cursor remains
    /// unchanged until [`WasmSession::commit_staged_next`] is called. A caller
    /// that cannot publish the packet must call [`WasmSession::discard_staged_next`]
    /// to reconstruct the last committed boundary.
    ///
    /// # Errors
    ///
    /// Returns an error for bounded algorithm failure or serialization failure.
    pub fn stage_next_json(&mut self) -> Result<Option<String>, JsError> {
        self.stage_next_json_with_limit(MAX_FRAME_JSON_BYTES)
            .map_err(|error| JsError::new(&error))
    }

    /// Publishes the previously staged item after packet transfer succeeds.
    pub fn commit_staged_next(&mut self) {
        if self.staged_next.take().is_none() {
            return;
        }
        self.cursor += 1;
        self.maybe_store_active_checkpoint();
    }

    /// Reconstructs the committed boundary after packet publication fails.
    ///
    /// # Errors
    ///
    /// Returns an error if the last committed boundary cannot be reconstructed.
    pub fn discard_staged_next(&mut self) -> Result<(), JsError> {
        if self.staged_next.take().is_some() {
            self.restore_current_boundary()
                .map_err(|error| JsError::new(&error))?;
        }
        Ok(())
    }

    fn stage_next_json_with_limit(
        &mut self,
        frame_json_limit: usize,
    ) -> Result<Option<String>, String> {
        if self.staged_seek.is_some() {
            return Err("cannot step while a seek is active".to_owned());
        }
        if self.staged_next.is_some() {
            return Err("a staged step is already pending".to_owned());
        }
        let Some(item) = self.work.get(self.cursor).cloned() else {
            return Ok(None);
        };
        let base_item_index = self.cursor;
        let recorded = match self
            .algorithm
            .apply_recorded_reconstructible(item.operation)
        {
            Ok(recorded) => recorded,
            Err(error) => {
                self.restore_current_boundary()?;
                return Err(error.to_string());
            }
        };
        let frame = SessionFrame {
            base_item_index,
            item_index: self.cursor + 1,
            item_count: self.work.len(),
            initial_build: item.initial_build,
            result: recorded.result,
            trace: recorded.trace,
            patches: recorded.patches,
        };
        let json = match serialize_frame_with_limit(&frame, frame_json_limit) {
            Ok(json) => json,
            Err(error) => {
                self.restore_current_boundary()?;
                return Err(error);
            }
        };
        self.staged_next = Some(StagedNext);
        Ok(Some(json))
    }

    /// Rebuilds the session and commits exactly `target` items from its start.
    ///
    /// # Errors
    ///
    /// Rejects a target after the end boundary and propagates replay failure.
    pub fn seek_json(&mut self, target: usize) -> Result<String, JsError> {
        self.begin_seek(target)?;
        while self
            .staged_seek
            .as_ref()
            .is_some_and(|seek| seek.cursor != seek.target)
        {
            self.resume_seek_json(4_096)?;
        }
        let frame = self
            .staged_seek
            .as_ref()
            .ok_or_else(|| JsError::new("seek candidate is missing"))?;
        let json = serialize_frame(&CurrentSessionFrame {
            item_index: frame.cursor,
            item_count: self.work.len(),
            structure: frame.algorithm.structure_snapshot(),
            canonical: frame.algorithm.canonical_snapshot(),
        })?;
        self.commit_staged_seek();
        Ok(json)
    }

    /// Starts a cancellable seek without replaying more than the hidden initial build.
    ///
    /// Forward seeks continue from the current state. Backward seeks restore the
    /// effective initial boundary and are then resumed in bounded chunks.
    ///
    /// # Errors
    ///
    /// Rejects a target after the end boundary or a failed initial restore.
    pub fn begin_seek(&mut self, target: usize) -> Result<(), JsError> {
        if target > self.work.len() {
            return Err(JsError::new("seek target exceeds item count"));
        }
        let current_distance = target.checked_sub(self.cursor);
        let checkpoint = self
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.cursor <= target)
            .max_by_key(|checkpoint| checkpoint.cursor)
            .filter(|checkpoint| {
                current_distance.is_none_or(|distance| target - checkpoint.cursor < distance)
            })
            .cloned();
        let (algorithm, cursor) = if let Some(checkpoint) = checkpoint {
            (checkpoint.algorithm, checkpoint.cursor)
        } else if target < self.cursor {
            let replacement =
                Self::from_scenario(self.scenario.clone()).map_err(|error| JsError::new(&error))?;
            (replacement.algorithm, 0)
        } else {
            (self.algorithm.clone(), self.cursor)
        };
        self.staged_seek = Some(StagedSeek {
            algorithm,
            cursor,
            target,
        });
        Ok(())
    }

    /// Replays at most `max_items` and returns progress plus the final frame.
    ///
    /// # Errors
    ///
    /// Rejects a zero chunk, a missing seek, algorithm failure, or serialization failure.
    pub fn resume_seek_json(&mut self, max_items: usize) -> Result<String, JsError> {
        self.resume_seek_json_with_limit(max_items, MAX_FRAME_JSON_BYTES)
            .map_err(|error| JsError::new(&error))
    }

    /// Publishes the staged seek only after its final packet transfer succeeds.
    pub fn commit_staged_seek(&mut self) {
        let Some(staged) = self.staged_seek.take() else {
            return;
        };
        self.algorithm = staged.algorithm;
        self.cursor = staged.cursor;
        self.maybe_store_active_checkpoint();
    }

    /// Drops an in-progress or completed seek candidate without changing current state.
    pub fn discard_staged_seek(&mut self) {
        self.staged_seek = None;
    }

    fn resume_seek_json_with_limit(
        &mut self,
        max_items: usize,
        frame_json_limit: usize,
    ) -> Result<String, String> {
        if max_items == 0 {
            return Err("seek chunk must be positive".to_owned());
        }
        let seek = self
            .staged_seek
            .as_mut()
            .ok_or_else(|| "no active seek".to_owned())?;
        let stop = seek.target.min(seek.cursor.saturating_add(max_items));
        while seek.cursor < stop {
            let item = self
                .work
                .get(seek.cursor)
                .ok_or_else(|| "seek replay exceeded item count".to_owned())?;
            seek.algorithm
                .apply(item.operation.clone(), &mut Vec::new())
                .map_err(|error| error.to_string())?;
            if seek.algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                return Err("ordered-map resource limit exceeded: visual entity count".to_owned());
            }
            seek.cursor += 1;
        }
        let done = seek.cursor == seek.target;
        let frame = if done {
            Some(CurrentSessionFrame {
                item_index: seek.cursor,
                item_count: self.work.len(),
                structure: seek.algorithm.structure_snapshot(),
                canonical: seek.algorithm.canonical_snapshot(),
            })
        } else {
            None
        };
        serialize_frame_with_limit(
            &SeekProgress {
                cursor: seek.cursor,
                done,
                target: seek.target,
                frame,
            },
            frame_json_limit,
        )
    }

    /// Advances the independent background seek index by a bounded number of items.
    ///
    /// Returns `true` once every timeline boundary is covered.
    ///
    /// # Errors
    ///
    /// Rejects a zero chunk and propagates algorithm failures.
    pub fn resume_seek_index(&mut self, max_items: usize) -> Result<bool, JsError> {
        if max_items == 0 {
            return Err(JsError::new("seek-index chunk must be positive"));
        }
        let Some(mut algorithm) = self.index_algorithm.take() else {
            return Ok(true);
        };
        let stop = self
            .work
            .len()
            .min(self.index_cursor.saturating_add(max_items));
        while self.index_cursor < stop {
            let item = self
                .work
                .get(self.index_cursor)
                .ok_or_else(|| JsError::new("seek index exceeded item count"))?;
            algorithm
                .apply(item.operation.clone(), &mut Vec::new())
                .map_err(|error| JsError::new(&error.to_string()))?;
            if algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                return Err(JsError::new(
                    "ordered-map resource limit exceeded: visual entity count",
                ));
            }
            self.index_cursor += 1;
            if self.index_cursor.is_multiple_of(CHECKPOINT_INTERVAL) {
                let estimated_bytes = algorithm.estimated_bytes();
                self.store_checkpoint_with_limits(
                    self.index_cursor,
                    estimated_bytes,
                    MAX_CHECKPOINT_BYTES,
                    MAX_CHECKPOINTS,
                    |_| algorithm.clone(),
                );
            }
        }
        let done = self.index_cursor == self.work.len();
        if done {
            let estimated_bytes = algorithm.estimated_bytes();
            self.store_checkpoint_with_limits(
                self.index_cursor,
                estimated_bytes,
                MAX_CHECKPOINT_BYTES,
                MAX_CHECKPOINTS,
                |_| algorithm,
            );
        } else {
            self.index_algorithm = Some(algorithm);
        }
        Ok(done)
    }

    /// Returns the strict canonical Scenario representation retained by the
    /// session. This does not depend on Worker health.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    pub fn scenario_json(&self) -> Result<String, JsError> {
        let encoded = serde_json::to_string(&self.scenario)
            .map_err(|error| JsError::new(&error.to_string()))?;
        canonical_scenario_json(&encoded)
    }
}

impl WasmSession {
    fn restore_current_boundary(&mut self) -> Result<(), String> {
        let target = self.cursor;
        let checkpoint = self
            .checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.cursor <= target)
            .max_by_key(|checkpoint| checkpoint.cursor)
            .cloned();
        let (mut algorithm, mut cursor) = if let Some(checkpoint) = checkpoint {
            (checkpoint.algorithm, checkpoint.cursor)
        } else {
            let replacement = Self::from_scenario(self.scenario.clone())?;
            (replacement.algorithm, 0)
        };
        while cursor < target {
            let item = self
                .work
                .get(cursor)
                .ok_or_else(|| "rollback replay exceeded item count".to_owned())?;
            algorithm
                .apply(item.operation.clone(), &mut Vec::new())
                .map_err(|error| error.to_string())?;
            if algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                return Err("ordered-map resource limit exceeded: visual entity count".to_owned());
            }
            cursor += 1;
        }
        self.algorithm = algorithm;
        Ok(())
    }

    fn current_frame(&self) -> Result<CurrentSessionFrame, JsError> {
        if self.algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
            return Err(JsError::new(
                "ordered-map resource limit exceeded: visual entity count",
            ));
        }
        Ok(CurrentSessionFrame {
            item_index: self.cursor,
            item_count: self.work.len(),
            structure: self.algorithm.structure_snapshot(),
            canonical: self.algorithm.canonical_snapshot(),
        })
    }

    fn from_scenario(scenario: ScenarioV1) -> Result<Self, String> {
        let seed = scenario
            .payload
            .algorithm_seed
            .parse::<u64>()
            .map_err(|_| "algorithm_seed is not a canonical u64".to_owned())?;
        let mut algorithm = AlgorithmInstance::from_spec(&scenario.payload.algorithm, seed)
            .map_err(|error| error.to_string())?;
        let initial = scenario
            .payload
            .initial
            .entries
            .iter()
            .map(|entry| {
                Ok(ModelOperation::Insert {
                    key: parse_key(&entry.key)?,
                    value: entry.value.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let mut work = Vec::with_capacity(
            scenario.payload.operations.items.len()
                + if scenario.payload.initial.show_build {
                    initial.len()
                } else {
                    0
                },
        );
        if scenario.payload.initial.show_build {
            work.extend(initial.into_iter().map(|operation| WorkItem {
                initial_build: true,
                operation,
            }));
        } else {
            for operation in initial {
                algorithm
                    .apply(operation, &mut Vec::new())
                    .map_err(|error| error.to_string())?;
                if algorithm.structure_entity_count() > ordered_map::MAX_VISUAL_ENTITIES {
                    return Err(
                        "ordered-map resource limit exceeded: visual entity count".to_owned()
                    );
                }
            }
        }
        for operation in &scenario.payload.operations.items {
            work.push(WorkItem {
                initial_build: false,
                operation: convert_operation(operation)?,
            });
        }
        let initial_estimated_bytes = algorithm.estimated_bytes();
        let checkpoint_allowed = initial_estimated_bytes <= MAX_CHECKPOINT_BYTES;
        let checkpoint_bytes = if checkpoint_allowed {
            initial_estimated_bytes
        } else {
            0
        };
        let checkpoints = if checkpoint_allowed {
            vec![Checkpoint {
                cursor: 0,
                estimated_bytes: initial_estimated_bytes,
                algorithm: algorithm.clone(),
            }]
        } else {
            Vec::new()
        };
        Ok(Self {
            scenario,
            index_algorithm: Some(algorithm.clone()),
            algorithm,
            work,
            cursor: 0,
            staged_seek: None,
            checkpoints,
            checkpoint_bytes,
            index_cursor: 0,
            staged_next: None,
        })
    }

    fn maybe_store_active_checkpoint(&mut self) {
        if self.cursor.is_multiple_of(CHECKPOINT_INTERVAL) || self.cursor == self.work.len() {
            let estimated_bytes = self.algorithm.estimated_bytes();
            self.store_checkpoint_with_limits(
                self.cursor,
                estimated_bytes,
                MAX_CHECKPOINT_BYTES,
                MAX_CHECKPOINTS,
                |session| session.algorithm.clone(),
            );
        }
    }

    fn store_checkpoint_with_limits(
        &mut self,
        cursor: usize,
        estimated_bytes: usize,
        max_bytes: usize,
        max_checkpoints: usize,
        factory: impl FnOnce(&Self) -> AlgorithmInstance,
    ) -> bool {
        if !self.admit_checkpoint_with_limits(cursor, estimated_bytes, max_bytes, max_checkpoints) {
            return false;
        }
        let algorithm = factory(self);
        self.insert_checkpoint(cursor, estimated_bytes, algorithm);
        true
    }

    fn admit_checkpoint_with_limits(
        &mut self,
        cursor: usize,
        estimated_bytes: usize,
        max_bytes: usize,
        max_checkpoints: usize,
    ) -> bool {
        if self
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.cursor == cursor)
        {
            return false;
        }
        if estimated_bytes > max_bytes || max_checkpoints == 0 {
            return false;
        }
        while self.checkpoints.len() >= max_checkpoints
            || self.checkpoint_bytes.saturating_add(estimated_bytes) > max_bytes
        {
            if self.checkpoints.is_empty() {
                return false;
            }
            let remove = self.checkpoint_to_evict();
            let removed = self.checkpoints.remove(remove);
            self.checkpoint_bytes = self
                .checkpoint_bytes
                .saturating_sub(removed.estimated_bytes);
        }
        true
    }

    fn insert_checkpoint(
        &mut self,
        cursor: usize,
        estimated_bytes: usize,
        algorithm: AlgorithmInstance,
    ) {
        self.checkpoints.push(Checkpoint {
            cursor,
            estimated_bytes,
            algorithm,
        });
        self.checkpoints
            .sort_unstable_by_key(|checkpoint| checkpoint.cursor);
        self.checkpoint_bytes = self.checkpoint_bytes.saturating_add(estimated_bytes);
    }

    fn checkpoint_to_evict(&self) -> usize {
        if self.checkpoints.len() <= 2 {
            return 0;
        }
        (1..self.checkpoints.len() - 1)
            .min_by_key(|&index| {
                self.checkpoints[index + 1].cursor - self.checkpoints[index - 1].cursor
            })
            .unwrap_or(1)
    }
}

fn parse_key(key: &str) -> Result<u64, String> {
    key.parse::<u64>()
        .map_err(|_| "operation key is not a canonical u64".to_owned())
}

fn convert_operation(operation: &Operation) -> Result<ModelOperation, String> {
    match operation {
        Operation::Insert { key, value } => Ok(ModelOperation::Insert {
            key: parse_key(key)?,
            value: value.clone(),
        }),
        Operation::Remove { key } => Ok(ModelOperation::Remove {
            key: parse_key(key)?,
        }),
        Operation::Get { key } => Ok(ModelOperation::Get {
            key: parse_key(key)?,
        }),
        Operation::LowerBound { key } => Ok(ModelOperation::LowerBound {
            key: parse_key(key)?,
        }),
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn scenario(show_build: bool) -> String {
        serde_json::json!({
            "schema_version": 1,
            "scenario_encoding_revision": "rfc8785-jcs/1",
            "plugin": "ordered-map",
            "reproducibility": { "declared": {
                "algorithm_revision": "ordered-map/1",
                "rng_version": 1,
                "plugin_result_revision": "ordered-map-result/1",
                "metrics_catalog_revision": "ordered-map-metrics/1",
                "trace_revision": "ordered-map-trace/3",
                "projection_revision": "ordered-map-projection/2",
                "layout_revision": "ordered-map-layout/1",
                "frame_encoding_revision": "scene-frame/5"
            }},
            "payload": {
                "algorithm": { "id": "avl", "config": {} },
                "algorithm_seed": "0",
                "initial": {
                    "entries": [
                        { "key": "8", "value": "root" },
                        { "key": "3", "value": "left" }
                    ],
                    "show_build": show_build
                },
                "operations": { "items": [
                    { "op": "insert", "key": "6", "value": "new" },
                    { "op": "lower_bound", "key": "4" }
                ] }
            }
        })
        .to_string()
    }

    fn commit_next(session: &mut WasmSession) -> String {
        let frame = session
            .stage_next_json()
            .expect("next item stages")
            .expect("timeline has a next item");
        session.commit_staged_next();
        frame
    }

    #[test]
    fn hidden_initial_build_is_present_in_first_scene_but_not_timeline() {
        let session = WasmSession::new(&scenario(false)).unwrap();
        assert_eq!(session.item_count(), 2);
        let frame: serde_json::Value =
            serde_json::from_str(&session.current_frame_json().unwrap()).unwrap();
        assert_eq!(frame["canonical"]["entries"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn staged_step_advances_cursor_only_after_commit_and_can_be_discarded() {
        let mut session = WasmSession::new(&scenario(false)).unwrap();
        let before = session.current_frame_json().unwrap();
        let staged = session
            .stage_next_json()
            .unwrap()
            .expect("timeline has a next item");

        assert_eq!(session.cursor(), 0);
        session.discard_staged_next().unwrap();
        assert_eq!(session.cursor(), 0);
        assert_eq!(session.current_frame_json().unwrap(), before);

        assert_eq!(session.stage_next_json().unwrap(), Some(staged));
        session.commit_staged_next();
        assert_eq!(session.cursor(), 1);
        assert_ne!(session.current_frame_json().unwrap(), before);
    }

    #[test]
    fn serialization_limit_failure_does_not_stage_or_advance() {
        let mut session = WasmSession::new(&scenario(false)).unwrap();
        let before = session.current_frame_json().unwrap();

        assert!(session.stage_next_json_with_limit(1).is_err());
        assert_eq!(session.cursor(), 0);
        assert!(session.staged_next.is_none());
        assert_eq!(session.current_frame_json().unwrap(), before);
    }

    #[test]
    fn serialization_failure_restores_a_nonzero_committed_boundary() {
        let mut session = WasmSession::new(&scenario(false)).unwrap();
        commit_next(&mut session);
        let committed = session.current_frame_json().unwrap();

        assert!(session.stage_next_json_with_limit(1).is_err());
        assert_eq!(session.cursor(), 1);
        assert!(session.staged_next.is_none());
        assert_eq!(session.current_frame_json().unwrap(), committed);

        commit_next(&mut session);
        assert_eq!(session.cursor(), 2);
    }

    #[test]
    fn visible_initial_build_and_seek_are_exact() {
        let mut session = WasmSession::new(&scenario(true)).unwrap();
        assert_eq!(session.item_count(), 4);
        let first: serde_json::Value = serde_json::from_str(&commit_next(&mut session)).unwrap();
        assert_eq!(first["initialBuild"], true);
        let at_three = session.seek_json(3).unwrap();
        assert_eq!(session.cursor(), 3);
        let replayed = session.seek_json(3).unwrap();
        assert_eq!(at_three, replayed);
    }

    #[test]
    fn trace_serialization_keeps_absent_option_fields_as_null() {
        let mut session = WasmSession::new(&scenario(false)).unwrap();
        commit_next(&mut session);
        let query_frame: serde_json::Value =
            serde_json::from_str(&commit_next(&mut session)).unwrap();
        let result_event = query_frame["trace"]
            .as_array()
            .and_then(|trace| trace.last())
            .unwrap();

        assert_eq!(
            result_event,
            &serde_json::json!({
                "catalog_id": 9,
                "kind": "result",
                "node": null,
                "target": null,
                "entry": null,
                "key": "4",
                "patch_start": 4,
                "patch_count": 0
            })
        );
    }

    #[test]
    fn double_rotation_crosses_the_wasm_boundary_as_two_patch_spans() {
        let mut value: serde_json::Value = serde_json::from_str(&scenario(false)).unwrap();
        value["payload"]["initial"]["entries"] = serde_json::json!([
            { "key": "3", "value": "three" },
            { "key": "1", "value": "one" }
        ]);
        value["payload"]["operations"]["items"] = serde_json::json!([
            { "op": "insert", "key": "2", "value": "two" }
        ]);
        let mut session = WasmSession::new(&value.to_string()).unwrap();
        let frame: serde_json::Value = serde_json::from_str(&commit_next(&mut session)).unwrap();
        let trace = frame["trace"].as_array().unwrap();
        let rotations: Vec<_> = trace
            .iter()
            .filter(|event| matches!(event["kind"].as_str(), Some("rotate-left" | "rotate-right")))
            .collect();

        assert_eq!(rotations.len(), 2);
        assert_eq!(rotations[0]["kind"], "rotate-left");
        assert_eq!(rotations[1]["kind"], "rotate-right");
        assert!(rotations[0]["patch_count"].as_u64().unwrap() >= 3);
        assert!(rotations[1]["patch_count"].as_u64().unwrap() >= 3);
        assert_eq!(
            rotations[0]["patch_start"].as_u64().unwrap()
                + rotations[0]["patch_count"].as_u64().unwrap(),
            rotations[1]["patch_start"].as_u64().unwrap()
        );

        let current: serde_json::Value =
            serde_json::from_str(&session.current_frame_json().unwrap()).unwrap();
        let root = &current["structure"]["root"];
        let root_node = current["structure"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == *root)
            .unwrap();
        assert_eq!(root_node["keys"], serde_json::json!(["2"]));
    }

    #[test]
    fn seek_resumes_in_bounded_chunks_and_restores_backward_state() {
        let mut session = WasmSession::new(&scenario(true)).unwrap();
        session.begin_seek(3).unwrap();
        let first: serde_json::Value =
            serde_json::from_str(&session.resume_seek_json(1).unwrap()).unwrap();
        assert_eq!(first["done"], false);
        assert_eq!(first["cursor"], 1);
        assert!(first.get("frame").is_none());

        let final_progress: serde_json::Value =
            serde_json::from_str(&session.resume_seek_json(2).unwrap()).unwrap();
        assert_eq!(final_progress["done"], true);
        assert_eq!(final_progress["frame"]["itemIndex"], 3);
        assert_eq!(session.cursor(), 0);
        session.commit_staged_seek();
        assert_eq!(session.cursor(), 3);

        session.begin_seek(1).unwrap();
        let backward: serde_json::Value =
            serde_json::from_str(&session.resume_seek_json(1).unwrap()).unwrap();
        assert_eq!(backward["done"], true);
        assert_eq!(backward["frame"]["itemIndex"], 1);
        session.commit_staged_seek();
        assert_eq!(session.cursor(), 1);
    }

    #[test]
    fn seek_serialization_failure_preserves_current_boundary() {
        let mut session = WasmSession::new(&scenario(true)).unwrap();
        let before = session.current_frame_json().unwrap();
        session.begin_seek(3).unwrap();

        assert!(session.resume_seek_json_with_limit(3, 1).is_err());
        assert_eq!(session.cursor(), 0);
        assert_eq!(session.current_frame_json().unwrap(), before);
        session.discard_staged_seek();
        assert_eq!(session.cursor(), 0);
        assert_eq!(session.current_frame_json().unwrap(), before);
    }

    #[test]
    fn full_u64_keys_and_metrics_cross_json_as_decimal_strings() {
        let mut value: serde_json::Value = serde_json::from_str(&scenario(false)).unwrap();
        value["payload"]["algorithm"] = serde_json::json!({
            "id": "veb",
            "config": { "word_bits": 64 }
        });
        value["payload"]["initial"]["entries"] = serde_json::json!([{
            "key": u64::MAX.to_string(),
            "value": "maximum"
        }]);
        let session = WasmSession::new(&value.to_string()).unwrap();
        let frame: serde_json::Value =
            serde_json::from_str(&session.current_frame_json().unwrap()).unwrap();
        assert_eq!(
            frame["canonical"]["entries"][0]["key"],
            u64::MAX.to_string()
        );
        assert!(frame["canonical"]["metrics"]["comparisons"].is_string());
    }

    #[test]
    fn persisted_scenario_is_strict_rfc8785_json() {
        let canonical = canonical_scenario_json(&scenario(false)).unwrap();
        assert!(!canonical.contains('\n'));
        assert!(canonical.starts_with("{\"payload\":"));
        assert_eq!(
            canonical,
            WasmSession::new(&scenario(false))
                .unwrap()
                .scenario_json()
                .unwrap()
        );
    }

    #[test]
    fn checkpoint_admission_evicts_before_clone_and_never_exceeds_budget() {
        let mut session = WasmSession::new(&scenario(false)).unwrap();
        session.checkpoints.clear();
        session.checkpoint_bytes = 0;
        let factory_calls = Cell::new(0);

        assert!(session.store_checkpoint_with_limits(1, 60, 100, 2, |view| {
            factory_calls.set(factory_calls.get() + 1);
            assert!(view.checkpoints.is_empty());
            view.algorithm.clone()
        }));
        assert_eq!(session.checkpoint_bytes, 60);
        assert!(session.store_checkpoint_with_limits(2, 50, 100, 2, |view| {
            factory_calls.set(factory_calls.get() + 1);
            assert!(view.checkpoints.is_empty(), "eviction happens before clone");
            view.algorithm.clone()
        }));
        assert_eq!(factory_calls.get(), 2);
        assert_eq!(session.checkpoint_bytes, 50);
        assert!(session.checkpoint_bytes <= 100);

        let before = session.checkpoints.len();
        assert!(
            !session.store_checkpoint_with_limits(3, 101, 100, 2, |view| {
                factory_calls.set(factory_calls.get() + 1);
                view.algorithm.clone()
            })
        );
        assert_eq!(factory_calls.get(), 2, "rejection does not clone");
        assert_eq!(session.checkpoints.len(), before);
        assert_eq!(session.checkpoint_bytes, 50);
    }

    #[test]
    fn background_index_is_bounded_and_checkpoint_seek_is_exact() {
        let mut value: serde_json::Value = serde_json::from_str(&scenario(false)).unwrap();
        value["payload"]["operations"]["items"] = serde_json::Value::Array(
            (0..5_000_u64)
                .map(|key| {
                    serde_json::json!({
                        "op": "insert",
                        "key": key.to_string(),
                        "value": format!("value-{key}")
                    })
                })
                .collect(),
        );
        let source = value.to_string();
        let mut indexed = WasmSession::new(&source).unwrap();
        let mut previous = 0;
        while !indexed.resume_seek_index(127).unwrap() {
            assert!(indexed.seek_coverage() - previous <= 127);
            previous = indexed.seek_coverage();
        }
        assert_eq!(indexed.seek_coverage(), 5_000);
        assert!(
            indexed
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.cursor == 4_096)
        );

        indexed.begin_seek(4_096).unwrap();
        assert_eq!(indexed.cursor(), 0);
        let indexed_frame: serde_json::Value =
            serde_json::from_str(&indexed.resume_seek_json(1).unwrap()).unwrap();
        let mut replayed = WasmSession::new(&source).unwrap();
        let replayed_frame: serde_json::Value =
            serde_json::from_str(&replayed.seek_json(4_096).unwrap()).unwrap();
        assert_eq!(indexed_frame["frame"], replayed_frame);
        indexed.commit_staged_seek();
        assert_eq!(indexed.cursor(), 4_096);
    }
}
