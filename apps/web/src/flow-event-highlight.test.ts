import { describe, expect, it } from "vitest";
import {
	flowAuxiliaryCellFocus,
	flowCapacityScalingPhaseBoundary,
	flowMinimumMeanResidualScan,
	flowPolynomialPrimalScan,
	flowPrimitiveArcInspection,
	flowRelaxationArcScan,
	ordinaryFlowEventEntityRefs,
	shouldRenderFlowEventEntityEmphasis,
} from "./flow-event-highlight";
import type { FlowEntityRenderContext } from "./flow-render-plan";

function context(): FlowEntityRenderContext {
	return {
		traceEvent: {
			entity_refs: [{ kind: "node", node_id: "v" }],
		},
		traceEventSemantics: {
			changed_entity_refs: [{ kind: "node", node_id: "changed" }],
		},
	} as FlowEntityRenderContext;
}

describe("ordinaryFlowEventEntityRefs", () => {
	it("keeps ordinary event focus", () => {
		expect(ordinaryFlowEventEntityRefs(context())).toEqual([
			{ kind: "node", node_id: "v" },
		]);
	});

	it("does not replace source focus with changed-entity metadata", () => {
		expect(ordinaryFlowEventEntityRefs(context())).toEqual([
			{ kind: "node", node_id: "v" },
		]);
	});
});

describe("flowCapacityScalingPhaseBoundary", () => {
	it("keeps the exact variant, boundary, and published scale together", () => {
		const value = {
			...context(),
			traceEvent: {
				event_id: "phase-1",
				catalog_id: "capacity-scaling-mcf.start-scaling-phase",
				minimum_granularity: "phase",
				pseudocode_line: "capacity-scaling:start-delta-phase",
				patch_count: 0,
				entity_refs: [],
				detail: { label: "scale", value: "8" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowCapacityScalingPhaseBoundary(value)).toEqual({
			variant: "capacity",
			boundary: "start",
			scale: 8n,
			scaleLabel: "8",
		});

		expect(
			flowCapacityScalingPhaseBoundary({
				...value,
				traceEvent: {
					event_id: "phase-2",
					catalog_id: "excess-scaling-mcf.complete-excess-phase",
					minimum_granularity: "phase",
					pseudocode_line: "excess-scaling:complete-excess-phase",
					patch_count: 0,
					entity_refs: [],
					detail: { label: "scale", value: "2" },
				} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			} as FlowEntityRenderContext),
		).toEqual({
			variant: "excess",
			boundary: "complete",
			scale: 2n,
			scaleLabel: "2",
		});
	});

	it("surfaces a producer boundary that omits its exact scale", () => {
		expect(() =>
			flowCapacityScalingPhaseBoundary({
				...context(),
				traceEvent: {
					event_id: "phase-invalid",
					catalog_id: "capacity-scaling-mcf.start-scaling-phase",
					minimum_granularity: "phase",
					pseudocode_line: "capacity-scaling:start-delta-phase",
					patch_count: 0,
					entity_refs: [],
				} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			} as FlowEntityRenderContext),
		).toThrow(/did not publish its exact scale/u);
	});
});

describe("flowAuxiliaryCellFocus", () => {
	it("binds the matrix row and column to their exact graph vertices", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "electrical-flow.matrix-scalar-product",
				entity_refs: [
					{ kind: "node", node_id: "row" },
					{ kind: "node", node_id: "column" },
				],
				detail: { label: "matrix scalar products", value: "17" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				work_progress: { primary_total: "81" },
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowAuxiliaryCellFocus(value)).toEqual({
			kind: "laplacian",
			rowNodeId: "row",
			columnNodeId: "column",
			completed: "17",
			total: "81",
		});
	});

	it("collapses a diagonal cell and rejects broad or mixed focus", () => {
		const diagonal = {
			...context(),
			traceEvent: {
				catalog_id: "electrical-flow.matrix-scalar-product",
				entity_refs: [{ kind: "node", node_id: "v" }],
				detail: { label: "matrix scalar products", value: "4" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				work_progress: { primary_total: "9" },
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowAuxiliaryCellFocus(diagonal)).toMatchObject({
			kind: "laplacian",
			rowNodeId: "v",
			columnNodeId: "v",
		});
		expect(
			flowAuxiliaryCellFocus({
				...diagonal,
				traceEvent: {
					...diagonal.traceEvent,
					entity_refs: [
						{ kind: "node", node_id: "a" },
						{ kind: "node", node_id: "b" },
						{ kind: "node", node_id: "c" },
					],
				} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			} as FlowEntityRenderContext),
		).toBeUndefined();
	});

	it("keeps relaxed-MNDC assignment row and column roles", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "relaxed-most-negative-cycle.inspect-assignment-cell",
				entity_refs: [
					{ kind: "node", node_id: "left" },
					{ kind: "node", node_id: "right" },
				],
				detail: { label: "assignment cell scan", value: "23" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				work_progress: { primary_total: "144" },
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowAuxiliaryCellFocus(value)).toEqual({
			kind: "assignment",
			rowNodeId: "left",
			columnNodeId: "right",
			completed: "23",
			total: "144",
		});
	});

	it("derives Hungarian row and column roles from the assignment model", () => {
		const value = {
			...context(),
			model: {
				kind: "assignment",
				agents: ["z-agent"],
				tasks: ["a-task"],
				objective: "minimize",
			},
			traceEvent: {
				catalog_id: "hungarian.inspect-cell",
				entity_refs: [
					{ kind: "node", node_id: "a-task" },
					{ kind: "node", node_id: "z-agent" },
				],
				detail: { label: "cell-scans", value: "8" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				work_progress: { primary_total: "16" },
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowAuxiliaryCellFocus(value)).toEqual({
			kind: "assignment",
			rowNodeId: "z-agent",
			columnNodeId: "a-task",
			completed: "8",
			total: "16",
		});
	});
});

describe("flowPrimitiveArcInspection", () => {
	it("anchors the measured primary-work position to one residual direction", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "shortest-augmenting-path.inspect-residual-arc",
				pseudocode_line: "sap:inspect-residual-arc",
				entity_refs: [
					{ kind: "residual-arc", edge_id: "uv", direction: "reverse" },
				],
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				role: "select",
				primary_work_block: { first: "1", last: "1", total: "1" },
				work_progress: {
					detail_completed: "18",
					detail_total: "90",
					primary_completed: "17",
					primary_total: "61",
				},
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowPrimitiveArcInspection(value)).toEqual({
			caption: "SCAN 17/61 · REV",
			completed: "17",
			total: "61",
			target: { kind: "residual-arc", edge_id: "uv", direction: "reverse" },
		});
	});

	it("shows an exact range for aggregated work and rejects broad focus", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "kernel.scan-arc",
				pseudocode_line: "kernel:scan-arc",
				entity_refs: [{ kind: "edge", edge_id: "uv" }],
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				role: "select",
				primary_work_block: { first: "1", last: "3", total: "3" },
				work_progress: {
					detail_completed: "4",
					detail_total: "10",
					primary_completed: "9",
					primary_total: "30",
				},
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowPrimitiveArcInspection(value)?.caption).toBe("SCAN 7–9/30");
		expect(
			flowPrimitiveArcInspection({
				...value,
				traceEvent: {
					...value.traceEvent,
					entity_refs: [
						{ kind: "edge", edge_id: "uv" },
						{ kind: "edge", edge_id: "vw" },
					],
				} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			} as FlowEntityRenderContext),
		).toBeUndefined();
	});

	it("uses the action-local block width when its ordinal does not start at one", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "kernel.inspect-arc",
				pseudocode_line: "kernel:inspect-arc",
				entity_refs: [{ kind: "edge", edge_id: "uv" }],
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				role: "select",
				primary_work_block: { first: "5", last: "7", total: "10" },
				work_progress: {
					detail_completed: "8",
					detail_total: "12",
					primary_completed: "20",
					primary_total: "40",
				},
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowPrimitiveArcInspection(value)?.caption).toBe("SCAN 18–20/40");
	});

	it("keeps a counted inspection visible when its typed algorithm state mutates", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "warm-start-push-relabel.inspect-t-excess-arc",
				pseudocode_line: "warm-start:inspect-t-excess-arc",
				entity_refs: [{ kind: "edge", edge_id: "ut" }],
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			traceEventSemantics: {
				role: "mutate",
				primary_work_block: { first: "1", last: "1", total: "1" },
				work_progress: {
					detail_completed: "6",
					detail_total: "20",
					primary_completed: "5",
					primary_total: "14",
				},
			} as NonNullable<FlowEntityRenderContext["traceEventSemantics"]>,
		} as FlowEntityRenderContext;
		expect(flowPrimitiveArcInspection(value)?.caption).toBe("SCAN 5/14");
	});
});

describe("flowMinimumMeanResidualScan", () => {
	it("keeps the Karp substage, exact ordinal, and residual direction together", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "minimum-mean-cycle-canceling.inspect-residual-arc",
				entity_refs: [
					{ kind: "residual-arc", edge_id: "uv", direction: "reverse" },
				],
				detail: { label: "karp-dp scan ordinal", value: "17" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowMinimumMeanResidualScan(value)).toEqual({
			caption: "KARP DP · #17 · REV",
			ordinal: "17",
			target: { kind: "residual-arc", edge_id: "uv", direction: "reverse" },
		});
	});

	it("recognizes a retained source label and rejects a non-residual target", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "minimum-mean-cycle-canceling.inspect-residual-arc",
				entity_refs: [
					{ kind: "residual-arc", edge_id: "uv", direction: "forward" },
				],
				detail: {
					label:
						"residual-arc inspections · tight-arc scan ordinal 23 · units 23–23 of 80",
					value: "23",
				},
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowMinimumMeanResidualScan(value)?.caption).toBe(
			"TIGHT ARC · #23 · FWD",
		);

		const invalid = {
			...value,
			traceEvent: {
				...value.traceEvent,
				entity_refs: [{ kind: "edge", edge_id: "uv" }],
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowMinimumMeanResidualScan(invalid)).toBeUndefined();
	});
});

describe("shouldRenderFlowEventEntityEmphasis", () => {
	it("keeps local node work and suppresses near-global level sets", () => {
		expect(
			shouldRenderFlowEventEntityEmphasis({
				level: "structure",
				kind: "node",
				signal: "touch",
				memberCount: 3,
				totalCount: 40,
				structureLimit: 6,
			}),
		).toBe(true);
		expect(
			shouldRenderFlowEventEntityEmphasis({
				level: "structure",
				kind: "node",
				signal: "touch",
				memberCount: 16,
				totalCount: 18,
				structureLimit: 16,
			}),
		).toBe(false);
		expect(
			shouldRenderFlowEventEntityEmphasis({
				level: "structure",
				kind: "node",
				signal: "change",
				memberCount: 6,
				totalCount: 7,
				structureLimit: 16,
			}),
		).toBe(false);
	});

	it("keeps broad path edges visible while suppressing broad changed state", () => {
		expect(
			shouldRenderFlowEventEntityEmphasis({
				level: "detail",
				kind: "edge",
				signal: "touch",
				memberCount: 5,
				totalCount: 8,
				structureLimit: 6,
			}),
		).toBe(true);
		expect(
			shouldRenderFlowEventEntityEmphasis({
				level: "detail",
				kind: "edge",
				signal: "change",
				memberCount: 5,
				totalCount: 8,
				structureLimit: 6,
			}),
		).toBe(false);
	});
});

describe("flowPolynomialPrimalScan", () => {
	it("keeps the source scan kind, arc index, and residual target together", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "polynomial-primal-network-simplex.inspect-extended-arc",
				entity_refs: [
					{ kind: "residual-arc", edge_id: "uv", direction: "reverse" },
				],
				detail: { label: "scale scan ordinal", value: "3" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowPolynomialPrimalScan(value)).toEqual({
			caption: "SCALE · #3",
			ordinal: "3",
			target: { kind: "residual-arc", edge_id: "uv", direction: "reverse" },
		});
	});

	it("recognizes a source label retained inside an aggregated boundary label", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "polynomial-primal-network-simplex.inspect-extended-arc",
				entity_refs: [{ kind: "edge", edge_id: "uv" }],
				detail: {
					label:
						"extended-arc inspections · optimality scan ordinal 8 · units 1–1 of 1",
					value: "8",
				},
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowPolynomialPrimalScan(value)).toMatchObject({
			caption: "OPT · #8",
			ordinal: "8",
		});
	});
});

describe("flowRelaxationArcScan", () => {
	it("keeps the scan phase, global ordinal, and residual direction together", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "relaxation.scan-price-cut-arc",
				entity_refs: [
					{ kind: "residual-arc", edge_id: "uv", direction: "reverse" },
				],
				detail: {
					label: "original arc inspections · scan-ordinal 32 · units 1–1 of 1",
					value: "32",
				},
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowRelaxationArcScan(value)).toEqual({
			caption: "PRICE CUT · #32 · REV",
			ordinal: "32",
			target: { kind: "residual-arc", edge_id: "uv", direction: "reverse" },
		});
	});

	it("supports an original-edge boundary scan and rejects unrelated actions", () => {
		const value = {
			...context(),
			traceEvent: {
				catalog_id: "relaxation.scan-boundary-flow-arc",
				entity_refs: [{ kind: "edge", edge_id: "uv" }],
				detail: { label: "scan-ordinal", value: "9" },
			} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
		} as FlowEntityRenderContext;
		expect(flowRelaxationArcScan(value)?.caption).toBe("BOUND FLOW · #9");
		expect(
			flowRelaxationArcScan({
				...value,
				traceEvent: {
					...value.traceEvent,
					catalog_id: "relaxation.evaluate-ascent-slope",
				} as NonNullable<FlowEntityRenderContext["traceEvent"]>,
			} as FlowEntityRenderContext),
		).toBeUndefined();
	});
});
