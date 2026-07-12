use std::collections::BTreeMap;

use crate::StructureSnapshot;

pub(crate) type BinaryTopology = (u64, Vec<(u64, String, u64)>);

pub(crate) fn binary_topology(snapshot: &StructureSnapshot) -> BinaryTopology {
    let keys: BTreeMap<_, _> = snapshot
        .nodes
        .iter()
        .map(|node| (node.id, node.keys[0]))
        .collect();
    let root = snapshot
        .root
        .and_then(|id| keys.get(&id).copied())
        .expect("fixture tree has a root");
    let mut edges = snapshot
        .nodes
        .iter()
        .flat_map(|node| {
            node.links.iter().map(|link| {
                (
                    node.keys[0],
                    link.role.clone(),
                    *keys.get(&link.target).expect("fixture link is valid"),
                )
            })
        })
        .collect::<Vec<_>>();
    edges.sort();
    (root, edges)
}
