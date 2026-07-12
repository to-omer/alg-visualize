//! Traceable, deterministic ordered-map implementations.

#![forbid(unsafe_code)]

mod aa;
pub mod avl;
mod binary_store;
mod binary_trace;
mod btree;
mod llrb;
pub mod model;
mod randomized;
mod registry;
mod scapegoat;
mod skip_list;
mod splay;
#[cfg(test)]
mod test_support;
mod trace_state;
mod veb;
mod wbt;
mod xfast;
mod yfast;

pub use aa::AaMap;
pub use avl::AvlMap;
pub use btree::BTreeMap;
pub use llrb::LlrbMap;
pub use model::{
    AuxiliaryId, CanonicalEntry, CanonicalSnapshot, EntryId, InvariantViolation, MAX_TRACE_EVENTS,
    MAX_VISUAL_ENTITIES, MapError, MetricOrdinal, Metrics, NodeId, Operation, OperationResult,
    OrderedMap, StatePatchRecord, StructureEntityId, StructureLink, StructureNode,
    StructureSnapshot, TraceEvent, TraceKind,
};
pub use randomized::{TreapMap, ZipMap};
pub use registry::{AlgorithmInstance, RecordedOperation};
pub use scapegoat::ScapegoatMap;
pub use skip_list::SkipListMap;
pub use splay::SplayMap;
pub use trace_state::{OrderedMapTraceRecorder, TraceState};
pub use veb::VebMap;
pub use wbt::WbtMap;
pub use xfast::XFastMap;
pub use yfast::YFastMap;
