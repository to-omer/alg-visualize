//! Shared local reversible-patch builders for binary ordered maps.

use crate::TraceState;

#[derive(Clone, Copy)]
pub(crate) enum RootUpdate {
    Preserve,
    Set(Option<NodeId>),
}
use std::collections::BTreeMap;

use crate::model::{
    CanonicalEntry, EntryId, MapError, MetricOrdinal, NodeId, StatePatchRecord, StructureEntityId,
    StructureNode,
};

pub(crate) fn metric_increments(
    state: &TraceState,
    increments: &[(MetricOrdinal, u64)],
) -> Result<Vec<StatePatchRecord>, MapError> {
    let mut ordered = increments.to_vec();
    ordered.sort_by_key(|(ordinal, _)| *ordinal);
    let mut records = Vec::with_capacity(ordered.len());
    for (ordinal, delta) in ordered {
        if delta == 0 {
            continue;
        }
        let before = state.metric_value(ordinal);
        records.push(StatePatchRecord::Metric {
            ordinal,
            before,
            after: before
                .checked_add(delta)
                .ok_or(MapError::ArithmeticOverflow)?,
        });
    }
    Ok(records)
}

pub(crate) fn node_change(
    state: &TraceState,
    after: StructureNode,
) -> Result<StatePatchRecord, MapError> {
    let before = state
        .node(after.id)
        .ok_or(MapError::TraceState(
            "changed binary node is missing from trace state",
        ))?
        .clone();
    Ok(StatePatchRecord::Node {
        id: after.id,
        before: Some(Box::new(before)),
        after: Some(Box::new(after)),
    })
}

pub(crate) fn metadata_change(
    state: &TraceState,
    after: StructureNode,
) -> Result<Vec<StatePatchRecord>, MapError> {
    Ok(vec![node_change(state, after)?])
}

pub(crate) fn projection_changes(
    state: &TraceState,
    root_after: Option<NodeId>,
    nodes_after: Vec<StructureNode>,
    metrics: &[(MetricOrdinal, u64)],
) -> Result<Vec<StatePatchRecord>, MapError> {
    let root_after = root_after.map(StructureEntityId::Node);
    let mut records = Vec::new();
    if state.root() != root_after {
        records.push(StatePatchRecord::Root {
            before: state.root(),
            after: root_after,
        });
    }
    let nodes: BTreeMap<_, _> = nodes_after
        .into_iter()
        .map(|node| (node.id, node))
        .collect();
    for node in nodes.into_values() {
        if state.node(node.id) != Some(&node) {
            records.push(node_change(state, node)?);
        }
    }
    records.extend(metric_increments(state, metrics)?);
    Ok(records)
}

pub(crate) fn node_removal(
    state: &TraceState,
    removed: NodeId,
    root_update: RootUpdate,
    nodes_after: Vec<StructureNode>,
) -> Result<Vec<StatePatchRecord>, MapError> {
    removal_records(state, removed, None, root_update, nodes_after, 1)
}

pub(crate) fn removal(
    state: &TraceState,
    removed: NodeId,
    entry: EntryId,
    root_update: RootUpdate,
    nodes_after: Vec<StructureNode>,
) -> Result<Vec<StatePatchRecord>, MapError> {
    removal_records(state, removed, Some(entry), root_update, nodes_after, 2)
}

fn removal_records(
    state: &TraceState,
    removed: NodeId,
    entry: Option<EntryId>,
    root_update: RootUpdate,
    nodes_after: Vec<StructureNode>,
    frees: u64,
) -> Result<Vec<StatePatchRecord>, MapError> {
    let root_after = match root_update {
        RootUpdate::Preserve => state.root(),
        RootUpdate::Set(root) => root.map(StructureEntityId::Node),
    };
    let mut records = Vec::new();
    if state.root() != root_after {
        records.push(StatePatchRecord::Root {
            before: state.root(),
            after: root_after,
        });
    }
    let removed = StructureEntityId::Node(removed);
    let mut nodes: BTreeMap<_, _> = nodes_after
        .into_iter()
        .map(|node| (node.id, node))
        .collect();
    nodes.remove(&removed);
    let mut node_ids = nodes.keys().copied().collect::<Vec<_>>();
    node_ids.push(removed);
    node_ids.sort_unstable();
    for id in node_ids {
        if id == removed {
            let before = state
                .node(id)
                .ok_or(MapError::TraceState(
                    "removed binary node is missing from trace state",
                ))?
                .clone();
            records.push(StatePatchRecord::Node {
                id,
                before: Some(Box::new(before)),
                after: None,
            });
        } else if let Some(node) = nodes.remove(&id)
            && state.node(id) != Some(&node)
        {
            records.push(node_change(state, node)?);
        }
    }
    if let Some(entry) = entry {
        let before = state
            .entry(entry)
            .ok_or(MapError::TraceState(
                "removed binary entry is missing from trace state",
            ))?
            .clone();
        records.push(StatePatchRecord::Entry {
            id: entry,
            before: Some(Box::new(before)),
            after: None,
        });
    }
    records.extend(metric_increments(state, &[(MetricOrdinal::Frees, frees)])?);
    Ok(records)
}

pub(crate) fn entry_change(
    state: &TraceState,
    after: CanonicalEntry,
) -> Result<Vec<StatePatchRecord>, MapError> {
    let before = state
        .entry(after.id)
        .ok_or(MapError::TraceState(
            "changed binary entry is missing from trace state",
        ))?
        .clone();
    Ok(vec![StatePatchRecord::Entry {
        id: after.id,
        before: Some(Box::new(before)),
        after: Some(Box::new(after)),
    }])
}

pub(crate) fn insertion(
    state: &TraceState,
    root_after: Option<NodeId>,
    parent_after: Option<StructureNode>,
    node_after: StructureNode,
    entry_after: CanonicalEntry,
) -> Result<Vec<StatePatchRecord>, MapError> {
    let root_after = root_after.map(StructureEntityId::Node);
    let mut records = Vec::with_capacity(5);
    if state.root() != root_after {
        records.push(StatePatchRecord::Root {
            before: state.root(),
            after: root_after,
        });
    }
    if let Some(parent_after) = parent_after
        && state.node(parent_after.id) != Some(&parent_after)
    {
        records.push(node_change(state, parent_after)?);
    }
    if state.node(node_after.id).is_some() || state.entry(entry_after.id).is_some() {
        return Err(MapError::TraceState(
            "inserted binary identity already exists",
        ));
    }
    records.push(StatePatchRecord::Node {
        id: node_after.id,
        before: None,
        after: Some(Box::new(node_after)),
    });
    records.sort_by_key(|record| match record {
        StatePatchRecord::Node { id, .. } => Some(*id),
        StatePatchRecord::Root { .. }
        | StatePatchRecord::Entry { .. }
        | StatePatchRecord::Metric { .. } => None,
    });
    records.push(StatePatchRecord::Entry {
        id: entry_after.id,
        before: None,
        after: Some(Box::new(entry_after)),
    });
    records.extend(metric_increments(
        state,
        &[(MetricOrdinal::Allocations, 2)],
    )?);
    Ok(records)
}

pub(crate) fn rotation(
    state: &TraceState,
    root: NodeId,
    pivot: NodeId,
    after_root: StructureNode,
    after_pivot: StructureNode,
) -> Result<Vec<StatePatchRecord>, MapError> {
    rotation_with_metrics(state, root, pivot, after_root, after_pivot, &[])
}

pub(crate) fn rotation_with_metrics(
    state: &TraceState,
    root: NodeId,
    pivot: NodeId,
    after_root: StructureNode,
    after_pivot: StructureNode,
    additional_metrics: &[(MetricOrdinal, u64)],
) -> Result<Vec<StatePatchRecord>, MapError> {
    let root_id = StructureEntityId::Node(root);
    let pivot_id = StructureEntityId::Node(pivot);
    let mut records = Vec::with_capacity(4);
    if state.root() == Some(root_id) {
        records.push(StatePatchRecord::Root {
            before: Some(root_id),
            after: Some(pivot_id),
        });
    } else {
        let parent = state.unique_incoming_node(root_id)?;
        let mut after_parent = parent.clone();
        let link = after_parent
            .links
            .iter_mut()
            .find(|link| link.target == root_id)
            .ok_or(MapError::TraceState("rotation incoming link disappeared"))?;
        link.target = pivot_id;
        records.push(StatePatchRecord::Node {
            id: parent.id,
            before: Some(Box::new(parent.clone())),
            after: Some(Box::new(after_parent)),
        });
    }
    records.push(node_change(state, after_root)?);
    records.push(node_change(state, after_pivot)?);
    records.sort_by_key(|record| match record {
        StatePatchRecord::Node { id, .. } => Some(*id),
        StatePatchRecord::Root { .. }
        | StatePatchRecord::Entry { .. }
        | StatePatchRecord::Metric { .. } => None,
    });
    let mut metrics = Vec::with_capacity(additional_metrics.len() + 1);
    metrics.push((MetricOrdinal::Rotations, 1));
    metrics.extend_from_slice(additional_metrics);
    records.extend(metric_increments(state, &metrics)?);
    Ok(records)
}
