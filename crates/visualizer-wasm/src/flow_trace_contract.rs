use flow::FlowTraceWorkUnitV1;

/// Closed registry of source-level actions that define one Detail primitive.
///
/// The publication normalizer rejects any `Micro` boundary absent from this
/// table. The representative audit also requires every entry to be observed,
/// so stale action IDs and silent producer renames fail the release gate.
pub(crate) const SOURCE_DETAIL_PRIMITIVE_CATALOG_IDS: &[&str] = &[
    "arc-fixing.advance-current-arc",
    "arc-fixing.inspect-residual-arc",
    "arc-fixing.push",
    "auction.bid",
    "auction.inspect-assignment-edge",
    "augment-relabel.advance-current-arc",
    "augment-relabel.advance-path",
    "augment-relabel.inspect-residual-arc",
    "augmenting-electrical-flow.elimination-pivot",
    "augmenting-electrical-flow.solve-direction",
    "bellman-ford-ssp.bottleneck",
    "bellman-ford-ssp.inspect-residual-arc",
    "bellman-ford-ssp.reconstruct-path",
    "bellman-ford-ssp.relax",
    "binary-blocking-flow.build-lift-adjacency",
    "binary-blocking-flow.build-reverse-zero-one-adjacency",
    "binary-blocking-flow.build-zero-scc-adjacency",
    "binary-blocking-flow.inspect-binary-length",
    "binary-blocking-flow.inspect-contracted-arc",
    "binary-blocking-flow.inspect-initial-cut-arc",
    "binary-blocking-flow.inspect-lift-arc",
    "binary-blocking-flow.inspect-residual-arc",
    "binary-blocking-flow.inspect-zero-scc-reverse-arc",
    "binary-blocking-flow.relax-binary-distance",
    "blocking-flow-primal-dual.inspect-equality-arc",
    "blocking-flow-primal-dual.inspect-level-arc",
    "blocking-flow-primal-dual.inspect-slack-arc",
    "blocking-flow-primal-dual.inspect-validation-arc",
    "borradaile-klein-planar.inspect-dual-arc",
    "borradaile-klein-planar.inspect-right-first-dart",
    "boykov-kolmogorov.grow-source-tree",
    "cancel-and-tighten.inspect-cycle-residual-arc",
    "cancel-and-tighten.inspect-ranking-residual-arc",
    "cancel-and-tighten.select-admissible-cycle",
    "capacity-scaling-augmenting-path.extend-path-prefix",
    "capacity-scaling-augmenting-path.inspect-residual-arc",
    "capacity-scaling-mcf.inspect-residual-arc",
    "convex-cost-scaling.inspect-marginal-residual-arc",
    "convex-cost-scaling.shortest-marginal-residual-path",
    "convex-network-simplex.price-forward-backward",
    "cost-scaling-push-relabel.advance-current-arc",
    "cost-scaling-push-relabel.inspect-residual-arc",
    "cost-scaling-push-relabel.push",
    "cost-scaling.advance-current-arc",
    "cost-scaling.inspect-residual-arc",
    "cost-scaling.push",
    "current-arc-heuristic.inspect-residual-arc",
    "current-arc-heuristic.push",
    "current-arc-heuristic.relabel",
    "deterministic-almost-linear-max-flow-oracle-demonstrator.inspect-fundamental-cycle",
    "deterministic-almost-linear-max-flow-oracle-demonstrator.install-branch-record",
    "deterministic-almost-linear-max-flow-oracle-demonstrator.query-cycle",
    "deterministic-almost-linear-mcf.query-minimum-ratio-cycle",
    "dfs-ford-fulkerson.extend-path-prefix",
    "dfs-ford-fulkerson.inspect-residual-arc",
    "dinic.extend-level-path-prefix",
    "dinic.inspect-residual-arc",
    "distance-directed-augmenting-path.inspect-residual-arc",
    "distance-directed-augmenting-path.relabel-node",
    "distance-directed-scaling-augmenting-path.inspect-residual-arc",
    "distance-directed-scaling-augmenting-path.relabel-node",
    "distance-directed-scaling-augmenting-path.tree-repaired",
    "double-scaling.inspect-transformed-residual-arc",
    "dual-network-simplex.inspect-entering-arc",
    "dual-network-simplex.inspect-initial-arc",
    "dynamic-eibfs.inspect-newly-residual-arc",
    "dynamic-eibfs.inspect-retained-parent",
    "dynamic-eibfs.repair-forest-boundary",
    "dynamic-eibfs.repair-invalidated-parent",
    "dynamic-eibfs.repair-new-bridge",
    "dynamic-tree-blocking-flow.inspect-residual-arc",
    "dynamic-tree-blocking-flow.link-candidate",
    "dynamic-tree-network-simplex.inspect-pricing-arc",
    "dynamic-tree-push-relabel.inspect-current-edge",
    "dynamic-tree-push-relabel.inspect-relabel-edge",
    "edmonds-karp.bottleneck",
    "edmonds-karp.inspect-residual-arc",
    "edmonds-karp.reconstruct-path",
    "eibfs.attach-sink-forest",
    "eibfs.attach-source-forest",
    "electrical-flow-interior-point-mcf.newton-centering-iteration",
    "electrical-flow.cg-iteration",
    "electrical-flow.matrix-scalar-product",
    "enhanced-capacity-scaling.inspect-residual-arc",
    "epsilon-relaxation.scan-price-breakpoint",
    "excess-scaling-mcf.inspect-residual-arc",
    "excess-scaling-push-relabel.inspect-residual-arc",
    "excess-scaling-push-relabel.push",
    "excess-scaling-push-relabel.relabel",
    "feasibility.advance-current-arc",
    "feasibility.extract-original-flow",
    "feasibility.inspect-cut-arc",
    "feasibility.inspect-discharge-arc",
    "feasibility.inspect-node-imbalance",
    "feasibility.inspect-relabel-arc",
    "feasibility.inspect-source-arc",
    "feasibility.push",
    "fifo-push-relabel.inspect-residual-arc",
    "fifo-push-relabel.push",
    "fifo-push-relabel.relabel",
    "ford-fulkerson.extend-path-prefix",
    "ford-fulkerson.inspect-residual-arc",
    "gap-relabel-heuristic.inspect-residual-arc",
    "gap-relabel-heuristic.push",
    "gap-relabel-heuristic.relabel",
    "generalized-cost-scaling.advance-current-arc",
    "generalized-cost-scaling.inspect-residual-arc",
    "generalized-cost-scaling.push",
    "generic-push-relabel.inspect-residual-arc",
    "generic-push-relabel.push",
    "generic-push-relabel.relabel",
    "global-relabel-heuristic.inspect-residual-arc",
    "global-relabel-heuristic.push",
    "global-relabel-heuristic.relabel",
    "goldberg-rao.build-lift-adjacency",
    "goldberg-rao.build-reverse-zero-one-adjacency",
    "goldberg-rao.build-zero-scc-adjacency",
    "goldberg-rao.inspect-binary-length",
    "goldberg-rao.inspect-canonical-cut-arc",
    "goldberg-rao.inspect-contracted-arc",
    "goldberg-rao.inspect-initial-cut-arc",
    "goldberg-rao.inspect-lift-arc",
    "goldberg-rao.inspect-residual-arc",
    "goldberg-rao.inspect-zero-scc-reverse-arc",
    "goldberg-rao.relax-binary-distance",
    "hassin-st-planar.inspect-dual-arc",
    "hassin-st-planar.settle-dual-face",
    "highest-label-push-relabel.inspect-residual-arc",
    "highest-label-push-relabel.push",
    "highest-label-push-relabel.relabel",
    "hochbaum-pseudoflow.inspect-residual-arc",
    "hochbaum-pseudoflow.normalize-push",
    "hochbaum-pseudoflow.split",
    "hopcroft-karp.extend-alternating-path",
    "hungarian.inspect-cell",
    "hungarian.select-minimum-slack",
    "ibfs.attach-sink-tree",
    "ibfs.attach-source-tree",
    "interior-point-max-flow.elimination-pivot",
    "interior-point-max-flow.solve-associated-electrical",
    "interior-point-max-flow.solve-centering-electrical",
    "isap.advance",
    "isap.inspect-residual-arc",
    "karzanov-preflow.balance",
    "karzanov-preflow.push",
    "minimum-mean-cycle-canceling.inspect-residual-arc",
    "minimum-ratio-cycle-max-flow.evaluate-cycle",
    "minimum-ratio-cycle-max-flow.inspect-vector-checkpoint",
    "minimum-ratio-cycle-mcf.evaluate-cycle",
    "minimum-ratio-cycle-mcf.evaluate-potential",
    "minimum-ratio-cycle-mcf.inspect-vector-checkpoint",
    "modi.compute-uv-opportunity-cost",
    "modi.form-closed-loop",
    "modi.inspect-pricing-route",
    "mpm.push-backward",
    "mpm.push-forward",
    "orlin-max-flow.inspect-classification-arc",
    "orlin-max-flow.inspect-compact-construction-arc",
    "orlin-max-flow.inspect-cut-residual-arc",
    "orlin-max-flow.inspect-decomposition-arc",
    "orlin-max-flow.inspect-expansion-residual-arc",
    "orlin-max-flow.inspect-lift-residual-arc",
    "orlin-max-flow.inspect-subproblem-arc",
    "orlin-max-flow.select-case",
    "orlin-mcf.inspect-compressed-arc",
    "orlin-mcf.inspect-compressed-residual-arc",
    "orlin-mcf.inspect-contractible-arc",
    "orlin-mcf.inspect-reachability-arc",
    "out-of-kilter.inspect-cut-arc",
    "out-of-kilter.modified-label-search",
    "out-of-kilter.select-out-of-kilter-arc",
    "parametric-breakpoint-rerun.inspect-static-residual-arc",
    "parametric-breakpoint-rerun.intersect-cut-functions",
    "parametric-pseudoflow.free-run-race",
    "parametric-pseudoflow.inspect-residual-arc",
    "partial-augment-relabel-max-flow.advance",
    "partial-augment-relabel-max-flow.inspect-residual-arc",
    "partial-augment-relabel-max-flow.relabel",
    "partial-augment-relabel-max-flow.retreat",
    "partial-augment-relabel-mcf.advance-current-arc",
    "partial-augment-relabel-mcf.advance-path",
    "partial-augment-relabel-mcf.inspect-residual-arc",
    "polynomial-primal-network-simplex.inspect-extended-arc",
    "polynomial-primal-network-simplex.select-admissible-arc",
    "potential-dijkstra-ssp.inspect-residual-arc",
    "prediction-assisted-epsilon-relaxation.inspect-admissible-arc",
    "prediction-assisted-epsilon-relaxation.inspect-price-breakpoint-arc",
    "prediction-assisted-epsilon-relaxation.select-positive-surplus",
    "price-refinement.advance-current-arc",
    "price-refinement.inspect-residual-arc",
    "price-refinement.push",
    "price-refinement.relax-price",
    "primal-dual-interior-point-mcf.inspect-forest-subset",
    "primal-dual-interior-point-mcf.sample-fundamental-cycle",
    "primal-dual-mcf.inspect-residual-arc",
    "primal-network-simplex.inspect-pricing-arc",
    "pseudoflow-simplex.inspect-residual-arc",
    "pseudoflow-simplex.select-entering",
    "randomized-almost-linear-max-flow-oracle-demonstrator.inspect-feasible-assignment",
    "randomized-almost-linear-max-flow-oracle-demonstrator.inspect-fundamental-cycle",
    "randomized-almost-linear-max-flow-oracle-demonstrator.query-cycle",
    "randomized-almost-linear-mcf-oracle-demonstrator.inspect-feasible-assignment",
    "randomized-almost-linear-mcf-oracle-demonstrator.inspect-oracle-vector",
    "randomized-almost-linear-mcf-oracle-demonstrator.query-minimum-ratio-cycle",
    "relabel-to-front.inspect-residual-arc",
    "relabel-to-front.push",
    "relabel-to-front.relabel",
    "relaxation.scan-balanced-arcs",
    "relaxation.scan-boundary-flow-arc",
    "relaxation.scan-price-cut-arc",
    "relaxed-most-negative-cycle.inspect-assignment-cell",
    "relaxed-most-negative-cycle.inspect-residual-arc",
    "relaxed-most-negative-cycle.select-family",
    "segment-expanded-convex-mcf.inspect-residual-arc",
    "shortest-augmenting-path.advance",
    "shortest-augmenting-path.inspect-residual-arc",
    "simple-cycle-canceling.inspect-residual-arc",
    "simple-cycle-canceling.relaxation-pass",
    "successive-shortest-augmenting-path.bottleneck",
    "successive-shortest-augmenting-path.inspect-residual-arc",
    "successive-shortest-augmenting-path.reconstruct-path",
    "successive-shortest-augmenting-path.settle-node",
    "successive-shortest-path.bottleneck",
    "successive-shortest-path.inspect-residual-arc",
    "successive-shortest-path.reconstruct-path",
    "successive-shortest-path.relax",
    "synchronous-parallel-push-relabel.inspect-residual-arc",
    "tardos-framework.inspect-fixed-variable",
    "tardos-framework.scan-residual-arc",
    "transportation-simplex.bland-price",
    "transportation-simplex.form-fundamental-cycle",
    "transportation-simplex.inspect-pricing-route",
    "unit-capacity-dinic.extend-level-path-prefix",
    "unit-capacity-dinic.inspect-residual-arc",
    "unit-network-dinic.extend-level-path-prefix",
    "unit-network-dinic.inspect-residual-arc",
    "warm-start-push-relabel.inspect-cut-saturation-arc",
    "warm-start-push-relabel.inspect-s-deficit-arc",
    "warm-start-push-relabel.inspect-t-excess-arc",
    "weighted-augmenting-paths.relabel-sweep",
    "weighted-push-relabel.completion-inspect-primitive-arc-checkpoint",
    "weighted-push-relabel.completion-relabel-checkpoint",
    "weighted-push-relabel.compute-distance-layers",
    "weighted-push-relabel.inspect-primitive-arc-checkpoint",
    "weighted-push-relabel.measure-short-flow",
    "weighted-push-relabel.relabel-checkpoint",
    "widest-augmenting-path.extend-path-prefix",
    "widest-augmenting-path.inspect-residual-arc",
];

/// Source actions that may carry the selected primary-work publication.
///
/// Measured work remains attached to these real source boundaries. Membership
/// declares a valid Detail fallback; aggregate counters never create synthetic
/// frames or demote an independently meaningful Phase or Operation.
pub(crate) const SOURCE_PRIMARY_WORK_BOUNDARY_CATALOG_IDS: &[&str] = &[
    "arc-fixing.complete-refine",
    "arc-fixing.fix-in",
    "arc-fixing.relabel",
    "arc-fixing.saturate-negative-arc",
    "arc-fixing.select-active-vertex",
    "arc-fixing.unfix-threshold-arcs",
    "arc-fixing.update-fixed-set",
    "auction.scale-complete",
    "auction.scale-start",
    "augment-relabel.complete-refine",
    "augment-relabel.relabel-tip",
    "augment-relabel.saturate-negative-arc",
    "augment-relabel.select-active-root",
    "augmenting-electrical-flow.augment-primal-dual",
    "bellman-ford-ssp.select-source",
    "binary-blocking-flow.analyze-binary-network",
    "blocking-flow-primal-dual.augment-admissible-path",
    "blocking-flow-primal-dual.complete-blocking-flow",
    "blocking-flow-primal-dual.tighten-dual",
    "borradaile-klein-planar.no-residual-path",
    "borradaile-klein-planar.preprocess-clockwise-cycles",
    "boykov-kolmogorov.adopt-sink-orphan",
    "boykov-kolmogorov.adopt-source-orphan",
    "boykov-kolmogorov.connect-trees",
    "boykov-kolmogorov.finish-active",
    "boykov-kolmogorov.free-sink-orphan",
    "boykov-kolmogorov.free-source-orphan",
    "boykov-kolmogorov.grow-sink-tree",
    "cancel-and-tighten.cancel-admissible-cycle",
    "capacity-scaling-augmenting-path.search",
    "cost-scaling-push-relabel.complete-refine",
    "cost-scaling-push-relabel.relabel",
    "cost-scaling-push-relabel.saturate-negative-arc",
    "cost-scaling-push-relabel.select-active-vertex",
    "cost-scaling.complete-refine",
    "cost-scaling.relabel",
    "cost-scaling.saturate-negative-arc",
    "cost-scaling.select-active-vertex",
    "deterministic-almost-linear-mcf.detect",
    "deterministic-almost-linear-mcf.periodic-reinitialize",
    "dfs-ford-fulkerson.search",
    "dinic.blocking-flow",
    "dinic.level-bfs",
    "dynamic-eibfs.repair-over-capacity",
    "dynamic-tree-blocking-flow.level-bfs",
    "dynamic-tree-network-simplex.price-block",
    "dynamic-tree-push-relabel.relabel-root",
    "eibfs.adopt-sink-orphan",
    "eibfs.adopt-source-orphan",
    "eibfs.cancel-same-cut-positive-flow",
    "eibfs.complete-phase",
    "eibfs.migrate-deficit-to-sink-root",
    "eibfs.migrate-excess-to-source-root",
    "eibfs.no-next-level",
    "eibfs.push-bridge-nonterminal-roots",
    "eibfs.push-bridge-sink-root",
    "eibfs.push-bridge-terminal-terminal",
    "eibfs.relabel-sink-orphan",
    "eibfs.relabel-source-orphan",
    "eibfs.remove-sink-orphan",
    "eibfs.remove-source-orphan",
    "epsilon-relaxation.push-admissible-arc",
    "feasibility.add-original-arc",
    "feasibility.complete-discharge",
    "feasibility.relabel",
    "feasibility.select-active-node",
    "ford-fulkerson.search-dfs",
    "generalized-cost-scaling.complete-refine",
    "generalized-cost-scaling.relabel",
    "generalized-cost-scaling.saturate-negative-arc",
    "generalized-cost-scaling.select-active-vertex",
    "global-relabel-heuristic.global-relabel",
    "goldberg-rao.start-gap-phase",
    "hochbaum-pseudoflow.blocking-cut",
    "hochbaum-pseudoflow.merge",
    "hochbaum-pseudoflow.recover-deficit",
    "hochbaum-pseudoflow.recover-excess",
    "hochbaum-pseudoflow.relabel-strong-set",
    "hopcroft-karp.level-bfs",
    "hopcroft-karp.phase-complete",
    "ibfs.adopt-sink-orphan",
    "ibfs.adopt-source-orphan",
    "ibfs.complete-pass",
    "ibfs.connect-trees",
    "ibfs.no-next-level",
    "ibfs.relabel-sink-orphan",
    "ibfs.relabel-source-orphan",
    "ibfs.remove-sink-orphan",
    "ibfs.remove-source-orphan",
    "isap.gap",
    "isap.relabel",
    "isap.reverse-bfs",
    "karzanov-preflow.initialize-preflow",
    "karzanov-preflow.level-bfs",
    "minimum-mean-cycle-canceling.optimal",
    "minimum-mean-cycle-canceling.select-minimum-mean-cycle",
    "mpm.level-bfs",
    "orlin-max-flow.begin-improvement",
    "out-of-kilter.raise-unlabeled-prices",
    "partial-augment-relabel-mcf.complete-refine",
    "partial-augment-relabel-mcf.relabel-tip",
    "partial-augment-relabel-mcf.saturate-negative-arc",
    "partial-augment-relabel-mcf.select-active-root",
    "polynomial-dual-network-simplex.inspect-augmentation-arc",
    "polynomial-dual-network-simplex.inspect-entering-arc",
    "polynomial-dual-network-simplex.inspect-initial-arc",
    "polynomial-primal-network-simplex.begin-epsilon-scale",
    "price-refinement.complete-relaxation-round",
    "price-refinement.relabel",
    "price-refinement.saturate-negative-arc",
    "price-refinement.select-active-vertex",
    "primal-network-simplex.price-block",
    "pseudoflow-simplex.blocking-cut",
    "pseudoflow-simplex.recover-deficit",
    "pseudoflow-simplex.recover-excess",
    "pseudoflow-simplex.relabel-strong-set",
    "relaxation.adjust-prices",
    "relaxed-most-negative-cycle.cancel-family",
    "segment-expanded-convex-mcf.optimal",
    "segment-expanded-convex-mcf.select-minimum-mean-cycle",
    "shortest-augmenting-path.relabel",
    "successive-shortest-path.select-source",
    "synchronous-parallel-push-relabel.global-relabel",
    "synchronous-parallel-push-relabel.recover-flow",
    "unit-capacity-dinic.blocking-flow",
    "unit-network-dinic.blocking-flow",
    "warm-start-push-relabel.recover-deficit",
    "warm-start-push-relabel.recover-excess",
    "weighted-augmenting-paths.augment-path",
    "weighted-augmenting-paths.finish-weighted-round",
    "widest-augmenting-path.search",
];

pub(crate) fn is_source_detail_primitive(catalog_id: &str) -> bool {
    SOURCE_DETAIL_PRIMITIVE_CATALOG_IDS
        .binary_search(&catalog_id)
        .is_ok()
}

pub(crate) fn is_source_primary_work_boundary(catalog_id: &str) -> bool {
    SOURCE_PRIMARY_WORK_BOUNDARY_CATALOG_IDS
        .binary_search(&catalog_id)
        .is_ok()
}

/// Detail primitives whose typed operand is a matrix cell rather than one
/// ordinary-network edge. The row and column nodes are both necessary to
/// identify that single cell; every other edge-free Detail primitive is
/// limited to one ordinary node.
pub(crate) fn source_detail_allows_two_node_auxiliary_focus(catalog_id: &str) -> bool {
    matches!(
        catalog_id,
        "electrical-flow.matrix-scalar-product"
            | "hungarian.inspect-cell"
            | "relaxed-most-negative-cycle.inspect-assignment-cell"
    )
}

/// Optional cross-algorithm counter for actions whose unit is exact.
///
/// Absence is deliberate: every event still carries publication, Detail, and
/// endpoint-primary work. This table only adds a second comparable unit where
/// the producer action has precisely that meaning.
pub(crate) fn exact_event_work_unit(catalog_id: &str) -> Option<FlowTraceWorkUnitV1> {
    match catalog_id {
        "edmonds-karp.bfs-complete" | "dinic.level-bfs" => Some(FlowTraceWorkUnitV1::BfsRun),
        "cost-scaling.advance-current-arc"
        | "cost-scaling-push-relabel.advance-current-arc"
        | "arc-fixing.advance-current-arc"
        | "cancel-and-tighten.inspect-cycle-residual-arc"
        | "cancel-and-tighten.inspect-ranking-residual-arc"
        | "double-scaling.inspect-transformed-residual-arc" => {
            Some(FlowTraceWorkUnitV1::ResidualArcScan)
        }
        "dfs-ford-fulkerson.search"
        | "capacity-scaling-augmenting-path.search"
        | "successive-shortest-path.shortest-path"
        | "bellman-ford-ssp.shortest-path"
        | "potential-dijkstra-ssp.shortest-path" => Some(FlowTraceWorkUnitV1::PathSearch),
        "potential-dijkstra-ssp.update-potentials" | "capacity-scaling-mcf.update-potentials" => {
            Some(FlowTraceWorkUnitV1::PotentialUpdate)
        }
        "dinic.blocking-flow" => Some(FlowTraceWorkUnitV1::BlockingFlowPhase),
        "primal-network-simplex.exchange-basis"
        | "primal-network-simplex.flip-entering-bound"
        | "dynamic-tree-network-simplex.cut-link-basis"
        | "dynamic-tree-network-simplex.flip-entering-bound" => {
            Some(FlowTraceWorkUnitV1::SimplexPivot)
        }
        "simple-cycle-canceling.find-negative-cycle" => {
            Some(FlowTraceWorkUnitV1::NegativeCycleSearch)
        }
        "simple-cycle-canceling.cancel-negative-cycle" => {
            Some(FlowTraceWorkUnitV1::CycleCancellation)
        }
        "edmonds-karp.augment"
        | "dfs-ford-fulkerson.augment"
        | "dinic.augment"
        | "successive-shortest-path.augment"
        | "bellman-ford-ssp.augment"
        | "potential-dijkstra-ssp.augment"
        | "capacity-scaling-augmenting-path.augment"
        | "capacity-scaling-mcf.augment" => Some(FlowTraceWorkUnitV1::Augmentation),
        "generic-push-relabel.push"
        | "feasibility.push"
        | "cost-scaling.push"
        | "cost-scaling-push-relabel.push"
        | "arc-fixing.push" => Some(FlowTraceWorkUnitV1::Push),
        "generic-push-relabel.relabel"
        | "feasibility.relabel"
        | "cost-scaling.relabel"
        | "cost-scaling-push-relabel.relabel"
        | "arc-fixing.relabel" => Some(FlowTraceWorkUnitV1::Relabel),
        "feasibility.advance-current-arc"
        | "feasibility.extract-original-flow"
        | "feasibility.inspect-cut-arc"
        | "feasibility.inspect-discharge-arc"
        | "feasibility.inspect-node-imbalance"
        | "feasibility.inspect-relabel-arc"
        | "feasibility.inspect-source-arc"
        | "feasibility.add-original-arc" => Some(FlowTraceWorkUnitV1::ResidualArcScan),
        "generic-push-relabel.discharge"
        | "feasibility.complete-discharge"
        | "cost-scaling.complete-discharge"
        | "cost-scaling-push-relabel.complete-discharge"
        | "arc-fixing.complete-discharge" => Some(FlowTraceWorkUnitV1::Discharge),
        "cost-scaling.start-refine"
        | "cost-scaling-push-relabel.start-refine"
        | "arc-fixing.start-refine"
        | "capacity-scaling-mcf.start-scaling-phase" => Some(FlowTraceWorkUnitV1::ScalingPhase),
        "cost-scaling.select-active-vertex"
        | "feasibility.select-active-node"
        | "cost-scaling-push-relabel.select-active-vertex"
        | "arc-fixing.select-active-vertex" => Some(FlowTraceWorkUnitV1::ActiveVertexSelection),
        "cost-scaling.saturate-negative-arc"
        | "cost-scaling-push-relabel.saturate-negative-arc"
        | "arc-fixing.saturate-negative-arc"
        | "capacity-scaling-mcf.saturate-negative-arc" => Some(FlowTraceWorkUnitV1::ArcSaturation),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{SOURCE_DETAIL_PRIMITIVE_CATALOG_IDS, SOURCE_PRIMARY_WORK_BOUNDARY_CATALOG_IDS};

    #[test]
    fn detail_primitive_registry_is_sorted_and_unique() {
        for registry in [
            SOURCE_DETAIL_PRIMITIVE_CATALOG_IDS,
            SOURCE_PRIMARY_WORK_BOUNDARY_CATALOG_IDS,
        ] {
            assert!(
                registry.windows(2).all(|pair| pair[0] < pair[1]),
                "Detail primitive registry must stay sorted and duplicate-free"
            );
        }
        assert!(
            SOURCE_DETAIL_PRIMITIVE_CATALOG_IDS
                .iter()
                .all(|catalog_id| SOURCE_PRIMARY_WORK_BOUNDARY_CATALOG_IDS
                    .binary_search(catalog_id)
                    .is_err()),
            "primitive and primary-work Detail registries must stay disjoint"
        );
    }
}
