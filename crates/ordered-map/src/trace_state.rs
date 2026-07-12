//! Reversible, transactionally validated state used by operation traces.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{
    CanonicalEntry, CanonicalSnapshot, EntryId, MAX_TRACE_EVENTS, MapError, MetricOrdinal, Metrics,
    StatePatchRecord, StructureEntityId, StructureNode, StructureSnapshot, TraceEvent,
};

pub(crate) const MAX_PATCH_RECORDS: usize = 1_000_000;
const METRIC_ORDINALS: [MetricOrdinal; 10] = [
    MetricOrdinal::Comparisons,
    MetricOrdinal::NodeVisits,
    MetricOrdinal::BitTests,
    MetricOrdinal::Rotations,
    MetricOrdinal::Recolors,
    MetricOrdinal::Splits,
    MetricOrdinal::Merges,
    MetricOrdinal::RebuildItems,
    MetricOrdinal::Allocations,
    MetricOrdinal::Frees,
];

pub(crate) enum TraceTarget<'a> {
    Events(&'a mut Vec<TraceEvent>),
    Recorder(&'a mut OrderedMapTraceRecorder),
}

impl TraceTarget<'_> {
    pub(crate) const fn records_patches(&self) -> bool {
        matches!(self, Self::Recorder(_))
    }

    pub(crate) fn record(&mut self, event: TraceEvent) -> Result<(), MapError> {
        match self {
            Self::Events(events) => {
                if events.len() >= MAX_TRACE_EVENTS {
                    return Err(MapError::ResourceLimit("trace event count"));
                }
                events.push(event);
                Ok(())
            }
            Self::Recorder(recorder) => recorder.record(event),
        }
    }

    pub(crate) fn transition(
        &mut self,
        event: TraceEvent,
        build: impl FnOnce(&TraceState) -> Result<Vec<StatePatchRecord>, MapError>,
    ) -> Result<(), MapError> {
        match self {
            Self::Events(events) => {
                if events.len() >= MAX_TRACE_EVENTS {
                    return Err(MapError::ResourceLimit("trace event count"));
                }
                events.push(event);
                Ok(())
            }
            Self::Recorder(recorder) => {
                let records = build(recorder.state())?;
                recorder.record_transition(event, records)
            }
        }
    }
}

/// Validated event log and reversible patch table for one operation.
#[derive(Clone, Debug)]
pub struct OrderedMapTraceRecorder {
    events: Vec<TraceEvent>,
    patches: Vec<StatePatchRecord>,
    state: TraceState,
    max_events: usize,
    max_patches: usize,
}

impl OrderedMapTraceRecorder {
    /// Starts an operation trace at one independently projected state boundary.
    ///
    /// # Errors
    ///
    /// Rejects an invalid base snapshot.
    pub fn new(
        structure: &StructureSnapshot,
        canonical: &CanonicalSnapshot,
    ) -> Result<Self, MapError> {
        Self::new_with_limits(structure, canonical, MAX_TRACE_EVENTS, MAX_PATCH_RECORDS)
    }

    pub(crate) fn new_with_limits(
        structure: &StructureSnapshot,
        canonical: &CanonicalSnapshot,
        max_events: usize,
        max_patches: usize,
    ) -> Result<Self, MapError> {
        Ok(Self {
            events: Vec::new(),
            patches: Vec::new(),
            state: TraceState::from_snapshots(structure, canonical)?,
            max_events,
            max_patches,
        })
    }

    /// Records an event that does not change visible state.
    ///
    /// # Errors
    ///
    /// Rejects an unrepresentable patch-table offset.
    pub fn record(&mut self, mut event: TraceEvent) -> Result<(), MapError> {
        if self.events.len() >= self.max_events {
            return Err(MapError::ResourceLimit("trace event count"));
        }
        event.patch_start = u32::try_from(self.patches.len())
            .map_err(|_| MapError::TraceState("state patch offset overflow"))?;
        event.patch_count = 0;
        self.events.push(event);
        Ok(())
    }

    /// Atomically validates and records one visible state transition.
    ///
    /// # Errors
    ///
    /// Rejects an empty, oversized, malformed, or stale transaction without
    /// changing the recorder.
    pub fn record_transition(
        &mut self,
        mut event: TraceEvent,
        records: Vec<StatePatchRecord>,
    ) -> Result<(), MapError> {
        if self.events.len() >= self.max_events {
            return Err(MapError::ResourceLimit("trace event count"));
        }
        if records.is_empty() {
            return Err(MapError::TraceState("state transition is empty"));
        }
        let patch_end = self
            .patches
            .len()
            .checked_add(records.len())
            .ok_or(MapError::TraceState("state patch record count overflow"))?;
        if patch_end > self.max_patches {
            return Err(MapError::ResourceLimit("state patch record count"));
        }
        self.state
            .apply_forward(&records)
            .map_err(|error| match error {
                MapError::TraceState(message) => MapError::TraceEventState {
                    catalog_id: event.catalog_id,
                    message,
                },
                error => error,
            })?;
        event.patch_start = u32::try_from(self.patches.len())
            .map_err(|_| MapError::TraceState("state patch offset overflow"))?;
        event.patch_count = u32::try_from(records.len())
            .map_err(|_| MapError::TraceState("state patch span overflow"))?;
        self.patches.extend(records);
        self.events.push(event);
        Ok(())
    }

    /// Verifies that event replay reaches independently projected final state.
    ///
    /// # Errors
    ///
    /// Returns an error if any visible final state was omitted or misstated.
    pub fn verify_final(
        &self,
        structure: &StructureSnapshot,
        canonical: &CanonicalSnapshot,
    ) -> Result<(), MapError> {
        let expected = TraceState::from_snapshots(structure, canonical)?;
        if self.state.root != expected.root {
            return Err(MapError::TraceState(
                "trace root does not match final snapshot",
            ));
        }
        if self.state.nodes != expected.nodes {
            return Err(MapError::TraceState(
                "trace nodes do not match final snapshot",
            ));
        }
        if self.state.entries != expected.entries {
            return Err(MapError::TraceState(
                "trace entries do not match final snapshot",
            ));
        }
        if self.state.metrics != expected.metrics {
            return Err(MapError::TraceState(
                "trace metrics do not match final snapshot",
            ));
        }
        Ok(())
    }

    /// Returns the validated event and patch tables.
    pub fn into_parts(self) -> (Vec<TraceEvent>, Vec<StatePatchRecord>) {
        (self.events, self.patches)
    }

    /// Returns the current recorder-owned shadow state.
    pub const fn state(&self) -> &TraceState {
        &self.state
    }
}

#[derive(Clone, Copy)]
enum Direction {
    Forward,
    Reverse,
}

/// Indexed ordered-map state that can apply one event patch in either direction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceState {
    root: Option<StructureEntityId>,
    nodes: BTreeMap<StructureEntityId, StructureNode>,
    incoming: BTreeMap<StructureEntityId, BTreeSet<(StructureEntityId, u32)>>,
    entries: BTreeMap<EntryId, CanonicalEntry>,
    entry_owners: BTreeMap<EntryId, BTreeSet<StructureEntityId>>,
    entry_keys: BTreeMap<u64, EntryId>,
    metrics: Metrics,
}

impl TraceState {
    /// Builds an indexed trace state from independent full snapshots.
    ///
    /// # Errors
    ///
    /// Rejects duplicate or self-inconsistent entity identities.
    pub fn from_snapshots(
        structure: &StructureSnapshot,
        canonical: &CanonicalSnapshot,
    ) -> Result<Self, MapError> {
        let mut nodes = BTreeMap::new();
        for node in &structure.nodes {
            if nodes.insert(node.id, node.clone()).is_some() {
                return Err(MapError::TraceState("duplicate structure entity"));
            }
        }
        if structure
            .root
            .is_some_and(|root| !nodes.contains_key(&root))
        {
            return Err(MapError::TraceState("root references missing entity"));
        }
        let mut entries = BTreeMap::new();
        let mut entry_keys = BTreeMap::new();
        for entry in &canonical.entries {
            if entries.insert(entry.id, entry.clone()).is_some() {
                return Err(MapError::TraceState("duplicate canonical entry"));
            }
            if entry_keys.insert(entry.key, entry.id).is_some() {
                return Err(MapError::TraceState("duplicate canonical key"));
            }
        }
        let incoming = incoming_links(&nodes);
        let entry_owners = entry_owners(&nodes);
        let state = Self {
            root: structure.root,
            nodes,
            incoming,
            entries,
            entry_owners,
            entry_keys,
            metrics: canonical.metrics,
        };
        state.validate_references()?;
        Ok(state)
    }

    /// Applies one event transaction from its declared `before` values.
    ///
    /// # Errors
    ///
    /// Rejects malformed ordering, identity mismatches, stale preconditions, and
    /// dangling state without applying any part of the patch.
    pub fn apply_forward(&mut self, records: &[StatePatchRecord]) -> Result<(), MapError> {
        self.apply(records, Direction::Forward)
    }

    /// Reverses one event transaction from its declared `after` values.
    ///
    /// # Errors
    ///
    /// Rejects malformed ordering, identity mismatches, stale preconditions, and
    /// dangling state without applying any part of the patch.
    pub fn apply_reverse(&mut self, records: &[StatePatchRecord]) -> Result<(), MapError> {
        self.apply(records, Direction::Reverse)
    }

    /// Materializes the current physical projection in canonical identity order.
    pub fn structure_snapshot(&self) -> StructureSnapshot {
        StructureSnapshot {
            root: self.root,
            nodes: self.nodes.values().cloned().collect(),
        }
    }

    /// Materializes the current logical contents and metrics in key order.
    pub fn canonical_snapshot(&self) -> CanonicalSnapshot {
        let mut entries: Vec<_> = self.entries.values().cloned().collect();
        entries.sort_by_key(|entry| entry.key);
        CanonicalSnapshot {
            entries,
            metrics: self.metrics,
        }
    }

    /// Returns the current projection root.
    pub const fn root(&self) -> Option<StructureEntityId> {
        self.root
    }

    /// Returns one current structural entity.
    pub fn node(&self, id: StructureEntityId) -> Option<&StructureNode> {
        self.nodes.get(&id)
    }

    pub(crate) fn unique_incoming_node(
        &self,
        target: StructureEntityId,
    ) -> Result<&StructureNode, MapError> {
        let incoming = self
            .incoming
            .get(&target)
            .ok_or(MapError::TraceState("rotation root has no incoming link"))?;
        if incoming.len() != 1 {
            return Err(MapError::TraceState(
                "rotation root has multiple incoming links",
            ));
        }
        let (source, _) = incoming
            .first()
            .ok_or(MapError::TraceState("rotation root has no incoming link"))?;
        self.nodes
            .get(source)
            .ok_or(MapError::TraceState("rotation parent is missing"))
    }

    /// Iterates current structural entities in canonical identity order.
    pub fn nodes(&self) -> impl ExactSizeIterator<Item = &StructureNode> {
        self.nodes.values()
    }

    /// Returns one current canonical entry.
    pub fn entry(&self, id: EntryId) -> Option<&CanonicalEntry> {
        self.entries.get(&id)
    }

    /// Returns one current cumulative metric.
    pub const fn metric_value(&self, ordinal: MetricOrdinal) -> u64 {
        self.metric(ordinal)
    }

    /// Computes the canonical reversible difference to independent snapshots.
    ///
    /// # Errors
    ///
    /// Rejects invalid target snapshots or cumulative metrics that decrease.
    pub fn diff_to_snapshots(
        &self,
        structure: &StructureSnapshot,
        canonical: &CanonicalSnapshot,
    ) -> Result<Vec<StatePatchRecord>, MapError> {
        let after = Self::from_snapshots(structure, canonical)?;
        let mut records = Vec::new();
        if self.root != after.root {
            records.push(StatePatchRecord::Root {
                before: self.root,
                after: after.root,
            });
        }
        let node_ids: BTreeSet<_> = self
            .nodes
            .keys()
            .chain(after.nodes.keys())
            .copied()
            .collect();
        for id in node_ids {
            let before = self.nodes.get(&id);
            let after_value = after.nodes.get(&id);
            if before != after_value {
                records.push(StatePatchRecord::Node {
                    id,
                    before: before.cloned().map(Box::new),
                    after: after_value.cloned().map(Box::new),
                });
            }
        }
        let entry_ids: BTreeSet<_> = self
            .entries
            .keys()
            .chain(after.entries.keys())
            .copied()
            .collect();
        for id in entry_ids {
            let before = self.entries.get(&id);
            let after_value = after.entries.get(&id);
            if before != after_value {
                records.push(StatePatchRecord::Entry {
                    id,
                    before: before.cloned().map(Box::new),
                    after: after_value.cloned().map(Box::new),
                });
            }
        }
        for ordinal in METRIC_ORDINALS {
            let before = self.metric(ordinal);
            let after_value = after.metric(ordinal);
            if after_value < before {
                return Err(MapError::TraceState("cumulative metric decreased"));
            }
            if before != after_value {
                records.push(StatePatchRecord::Metric {
                    ordinal,
                    before,
                    after: after_value,
                });
            }
        }
        Ok(records)
    }

    pub(crate) fn diff_selected(
        &self,
        root_after: Option<StructureEntityId>,
        mut nodes_after: Vec<(StructureEntityId, Option<StructureNode>)>,
        mut entries_after: Vec<(EntryId, Option<CanonicalEntry>)>,
        metrics_after: Metrics,
    ) -> Result<Vec<StatePatchRecord>, MapError> {
        let mut records = Vec::new();
        if self.root != root_after {
            records.push(StatePatchRecord::Root {
                before: self.root,
                after: root_after,
            });
        }
        nodes_after.sort_by_key(|(id, _)| *id);
        for pair in nodes_after.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(MapError::TraceState("duplicate selected node"));
            }
        }
        for (id, after) in nodes_after {
            if after.as_ref().is_some_and(|node| node.id != id) {
                return Err(MapError::TraceState("selected node identity mismatch"));
            }
            let before = self.nodes.get(&id);
            if before != after.as_ref() {
                records.push(StatePatchRecord::Node {
                    id,
                    before: before.cloned().map(Box::new),
                    after: after.map(Box::new),
                });
            }
        }
        entries_after.sort_by_key(|(id, _)| *id);
        for pair in entries_after.windows(2) {
            if pair[0].0 == pair[1].0 {
                return Err(MapError::TraceState("duplicate selected entry"));
            }
        }
        for (id, after) in entries_after {
            if after.as_ref().is_some_and(|entry| entry.id != id) {
                return Err(MapError::TraceState("selected entry identity mismatch"));
            }
            let before = self.entries.get(&id);
            if before != after.as_ref() {
                records.push(StatePatchRecord::Entry {
                    id,
                    before: before.cloned().map(Box::new),
                    after: after.map(Box::new),
                });
            }
        }
        for ordinal in METRIC_ORDINALS {
            let before = self.metric(ordinal);
            let after_value = metric_value(&metrics_after, ordinal);
            if after_value < before {
                return Err(MapError::TraceState("cumulative metric decreased"));
            }
            if before != after_value {
                records.push(StatePatchRecord::Metric {
                    ordinal,
                    before,
                    after: after_value,
                });
            }
        }
        Ok(records)
    }

    fn apply(
        &mut self,
        records: &[StatePatchRecord],
        direction: Direction,
    ) -> Result<(), MapError> {
        self.validate_patch(records, direction)?;
        for record in records {
            match record {
                StatePatchRecord::Root { before, after } => {
                    self.root = select(direction, before, after);
                }
                StatePatchRecord::Node { id, before, after } => {
                    match select_boxed(direction, before.as_deref(), after.as_deref()) {
                        Some(node) => self.replace_node(*id, Some(node)),
                        None => self.replace_node(*id, None),
                    }
                }
                StatePatchRecord::Entry { id, before, after } => {
                    match select_boxed(direction, before.as_deref(), after.as_deref()) {
                        Some(entry) => self.replace_entry(*id, Some(entry)),
                        None => self.replace_entry(*id, None),
                    }
                }
                StatePatchRecord::Metric {
                    ordinal,
                    before,
                    after,
                } => {
                    self.set_metric(*ordinal, select(direction, before, after));
                }
            }
        }
        if let Err(error) = self.validate_changed_references(records) {
            for record in records.iter().rev() {
                self.apply_validated_record(record, reverse(direction));
            }
            return Err(error);
        }
        Ok(())
    }

    fn validate_patch(
        &self,
        records: &[StatePatchRecord],
        direction: Direction,
    ) -> Result<(), MapError> {
        let mut previous: Option<PatchOrderKey> = None;
        for record in records {
            let order = PatchOrderKey::from(record);
            if previous.as_ref().is_some_and(|value| value >= &order) {
                return Err(MapError::TraceState(
                    "patch records are duplicated or noncanonical",
                ));
            }
            previous = Some(order);
            match record {
                StatePatchRecord::Root { before, after } => {
                    if before == after {
                        return Err(MapError::TraceState("root patch is a no-op"));
                    }
                    if self.root != select(reverse(direction), before, after) {
                        return Err(MapError::TraceState("root patch precondition mismatch"));
                    }
                }
                StatePatchRecord::Node { id, before, after } => {
                    validate_node_value(*id, before.as_deref())?;
                    validate_node_value(*id, after.as_deref())?;
                    if before == after {
                        return Err(MapError::TraceState("node patch is a no-op"));
                    }
                    if self.nodes.get(id)
                        != select_boxed(reverse(direction), before.as_deref(), after.as_deref())
                    {
                        return Err(MapError::TraceState("node patch precondition mismatch"));
                    }
                }
                StatePatchRecord::Entry { id, before, after } => {
                    validate_entry_value(*id, before.as_deref())?;
                    validate_entry_value(*id, after.as_deref())?;
                    if before == after {
                        return Err(MapError::TraceState("entry patch is a no-op"));
                    }
                    if self.entries.get(id)
                        != select_boxed(reverse(direction), before.as_deref(), after.as_deref())
                    {
                        return Err(MapError::TraceState("entry patch precondition mismatch"));
                    }
                }
                StatePatchRecord::Metric {
                    ordinal,
                    before,
                    after,
                } => {
                    if after <= before {
                        return Err(MapError::TraceState(
                            "metric patch must increase monotonically",
                        ));
                    }
                    if self.metric(*ordinal) != select(reverse(direction), before, after) {
                        return Err(MapError::TraceState("metric patch precondition mismatch"));
                    }
                }
            }
        }
        Ok(())
    }

    fn apply_validated_record(&mut self, record: &StatePatchRecord, direction: Direction) {
        match record {
            StatePatchRecord::Root { before, after } => {
                self.root = select(direction, before, after);
            }
            StatePatchRecord::Node { id, before, after } => {
                match select_boxed(direction, before.as_deref(), after.as_deref()) {
                    Some(node) => self.replace_node(*id, Some(node)),
                    None => self.replace_node(*id, None),
                }
            }
            StatePatchRecord::Entry { id, before, after } => {
                match select_boxed(direction, before.as_deref(), after.as_deref()) {
                    Some(entry) => self.replace_entry(*id, Some(entry)),
                    None => self.replace_entry(*id, None),
                }
            }
            StatePatchRecord::Metric {
                ordinal,
                before,
                after,
            } => self.set_metric(*ordinal, select(direction, before, after)),
        }
    }

    fn validate_changed_references(&self, records: &[StatePatchRecord]) -> Result<(), MapError> {
        if self
            .root
            .is_some_and(|root| !self.nodes.contains_key(&root))
        {
            return Err(MapError::TraceState("root references missing entity"));
        }
        let mut affected_nodes = BTreeSet::new();
        let mut affected_entries = BTreeSet::new();
        for record in records {
            match record {
                StatePatchRecord::Node { id, before, after } => {
                    affected_nodes.insert(*id);
                    for node in [before.as_deref(), after.as_deref()].into_iter().flatten() {
                        affected_nodes.extend(node.links.iter().map(|link| link.target));
                        affected_entries.extend(node.entries.iter().copied());
                    }
                }
                StatePatchRecord::Entry { id, .. } => {
                    affected_entries.insert(*id);
                }
                StatePatchRecord::Root { before, after } => {
                    affected_nodes.extend([*before, *after].into_iter().flatten());
                }
                StatePatchRecord::Metric { .. } => {}
            }
        }
        let mut affected_primary = BTreeSet::new();
        for id in &affected_nodes {
            let incoming = self.incoming.get(id).is_some_and(|links| !links.is_empty());
            let Some(node) = self.nodes.get(id) else {
                if incoming {
                    return Err(MapError::TraceState("link references missing entity"));
                }
                continue;
            };
            if matches!(id, StructureEntityId::Node(_)) {
                affected_primary.insert(*id);
            }
            if node
                .links
                .iter()
                .any(|link| !self.nodes.contains_key(&link.target))
            {
                return Err(MapError::TraceState("link references missing entity"));
            }
            if node
                .entries
                .iter()
                .any(|entry| !self.entries.contains_key(entry))
            {
                return Err(MapError::TraceState("node references missing entry"));
            }
        }
        self.validate_affected_reachability(&affected_primary)?;
        for id in affected_entries {
            if !self.entries.contains_key(&id)
                && self
                    .entry_owners
                    .get(&id)
                    .is_some_and(|owners| !owners.is_empty())
            {
                return Err(MapError::TraceState("node references missing entry"));
            }
        }
        Ok(())
    }

    fn validate_affected_reachability(
        &self,
        affected: &BTreeSet<StructureEntityId>,
    ) -> Result<(), MapError> {
        if affected.is_empty() {
            return Ok(());
        }
        let mut closure = affected.clone();
        let mut pending = affected.iter().copied().collect::<Vec<_>>();
        let mut outgoing: BTreeMap<StructureEntityId, BTreeSet<StructureEntityId>> =
            BTreeMap::new();
        while let Some(target) = pending.pop() {
            for (source, _) in self.incoming.get(&target).into_iter().flatten() {
                outgoing.entry(*source).or_default().insert(target);
                if self.nodes.contains_key(source) && closure.insert(*source) {
                    pending.push(*source);
                }
            }
        }
        let mut reachable = BTreeSet::new();
        let mut forward = self.root.into_iter().collect::<Vec<_>>();
        while let Some(source) = forward.pop() {
            if !reachable.insert(source) {
                continue;
            }
            if let Some(targets) = outgoing.get(&source) {
                forward.extend(targets);
            }
        }
        if affected.iter().any(|id| !reachable.contains(id)) {
            return Err(MapError::TraceState(
                "primary node is unreachable from root",
            ));
        }
        Ok(())
    }

    fn validate_references(&self) -> Result<(), MapError> {
        if self
            .root
            .is_some_and(|root| !self.nodes.contains_key(&root))
        {
            return Err(MapError::TraceState("root references missing entity"));
        }
        for node in self.nodes.values() {
            for link in &node.links {
                if !self.nodes.contains_key(&link.target) {
                    return Err(MapError::TraceState("link references missing entity"));
                }
            }
            for entry in &node.entries {
                if !self.entries.contains_key(entry) {
                    return Err(MapError::TraceState("node references missing entry"));
                }
            }
        }
        let mut reachable = BTreeSet::new();
        let mut pending = self.root.into_iter().collect::<Vec<_>>();
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            let node = self
                .nodes
                .get(&id)
                .ok_or(MapError::TraceState("reachable entity is missing"))?;
            pending.extend(node.links.iter().map(|link| link.target));
        }
        if self
            .nodes
            .keys()
            .any(|id| matches!(id, StructureEntityId::Node(_)) && !reachable.contains(id))
        {
            return Err(MapError::TraceState(
                "primary node is unreachable from root",
            ));
        }
        Ok(())
    }

    fn replace_node(&mut self, id: StructureEntityId, after: Option<&StructureNode>) {
        if let Some(before) = self.nodes.remove(&id) {
            for link in before.links {
                let remove_target = if let Some(sources) = self.incoming.get_mut(&link.target) {
                    sources.remove(&(id, link.slot));
                    sources.is_empty()
                } else {
                    false
                };
                if remove_target {
                    self.incoming.remove(&link.target);
                }
            }
            for entry in before.entries {
                let remove_entry = if let Some(owners) = self.entry_owners.get_mut(&entry) {
                    owners.remove(&id);
                    owners.is_empty()
                } else {
                    false
                };
                if remove_entry {
                    self.entry_owners.remove(&entry);
                }
            }
        }
        if let Some(node) = after {
            for link in &node.links {
                self.incoming
                    .entry(link.target)
                    .or_default()
                    .insert((id, link.slot));
            }
            for entry in &node.entries {
                self.entry_owners.entry(*entry).or_default().insert(id);
            }
            self.nodes.insert(id, node.clone());
        }
    }

    fn replace_entry(&mut self, id: EntryId, after: Option<&CanonicalEntry>) {
        if let Some(before) = self.entries.remove(&id) {
            self.entry_keys.remove(&before.key);
        }
        if let Some(entry) = after {
            self.entry_keys.insert(entry.key, id);
            self.entries.insert(id, entry.clone());
        }
    }

    const fn metric(&self, ordinal: MetricOrdinal) -> u64 {
        metric_value(&self.metrics, ordinal)
    }

    const fn set_metric(&mut self, ordinal: MetricOrdinal, value: u64) {
        match ordinal {
            MetricOrdinal::Comparisons => self.metrics.comparisons = value,
            MetricOrdinal::NodeVisits => self.metrics.node_visits = value,
            MetricOrdinal::BitTests => self.metrics.bit_tests = value,
            MetricOrdinal::Rotations => self.metrics.rotations = value,
            MetricOrdinal::Recolors => self.metrics.recolors = value,
            MetricOrdinal::Splits => self.metrics.splits = value,
            MetricOrdinal::Merges => self.metrics.merges = value,
            MetricOrdinal::RebuildItems => self.metrics.rebuild_items = value,
            MetricOrdinal::Allocations => self.metrics.allocations = value,
            MetricOrdinal::Frees => self.metrics.frees = value,
        }
    }
}

#[cfg(test)]
include!("trace_state_tests.rs");

const fn metric_value(metrics: &Metrics, ordinal: MetricOrdinal) -> u64 {
    match ordinal {
        MetricOrdinal::Comparisons => metrics.comparisons,
        MetricOrdinal::NodeVisits => metrics.node_visits,
        MetricOrdinal::BitTests => metrics.bit_tests,
        MetricOrdinal::Rotations => metrics.rotations,
        MetricOrdinal::Recolors => metrics.recolors,
        MetricOrdinal::Splits => metrics.splits,
        MetricOrdinal::Merges => metrics.merges,
        MetricOrdinal::RebuildItems => metrics.rebuild_items,
        MetricOrdinal::Allocations => metrics.allocations,
        MetricOrdinal::Frees => metrics.frees,
    }
}

fn incoming_links(
    nodes: &BTreeMap<StructureEntityId, StructureNode>,
) -> BTreeMap<StructureEntityId, BTreeSet<(StructureEntityId, u32)>> {
    let mut incoming = BTreeMap::new();
    for node in nodes.values() {
        for link in &node.links {
            incoming
                .entry(link.target)
                .or_insert_with(BTreeSet::new)
                .insert((node.id, link.slot));
        }
    }
    incoming
}

fn entry_owners(
    nodes: &BTreeMap<StructureEntityId, StructureNode>,
) -> BTreeMap<EntryId, BTreeSet<StructureEntityId>> {
    let mut owners = BTreeMap::new();
    for node in nodes.values() {
        for entry in &node.entries {
            owners
                .entry(*entry)
                .or_insert_with(BTreeSet::new)
                .insert(node.id);
        }
    }
    owners
}

#[derive(Eq, Ord, PartialEq, PartialOrd)]
enum PatchOrderKey {
    Root,
    Node(StructureEntityId),
    Entry(EntryId),
    Metric(MetricOrdinal),
}

impl From<&StatePatchRecord> for PatchOrderKey {
    fn from(record: &StatePatchRecord) -> Self {
        match record {
            StatePatchRecord::Root { .. } => Self::Root,
            StatePatchRecord::Node { id, .. } => Self::Node(*id),
            StatePatchRecord::Entry { id, .. } => Self::Entry(*id),
            StatePatchRecord::Metric { ordinal, .. } => Self::Metric(*ordinal),
        }
    }
}

const fn reverse(direction: Direction) -> Direction {
    match direction {
        Direction::Forward => Direction::Reverse,
        Direction::Reverse => Direction::Forward,
    }
}

fn select<T: Copy>(direction: Direction, before: &T, after: &T) -> T {
    match direction {
        Direction::Forward => *after,
        Direction::Reverse => *before,
    }
}

fn select_boxed<'a, T>(
    direction: Direction,
    before: Option<&'a T>,
    after: Option<&'a T>,
) -> Option<&'a T> {
    match direction {
        Direction::Forward => after,
        Direction::Reverse => before,
    }
}

fn validate_node_value(
    id: StructureEntityId,
    value: Option<&StructureNode>,
) -> Result<(), MapError> {
    if value.is_some_and(|node| node.id != id) {
        return Err(MapError::TraceState("node patch identity mismatch"));
    }
    Ok(())
}

fn validate_entry_value(id: EntryId, value: Option<&CanonicalEntry>) -> Result<(), MapError> {
    if value.is_some_and(|entry| entry.id != id) {
        return Err(MapError::TraceState("entry patch identity mismatch"));
    }
    Ok(())
}
