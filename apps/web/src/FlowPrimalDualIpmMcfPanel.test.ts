import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import {
	buildPrimalDualIpmAuxiliaryPositions,
	FlowPrimalDualIpmMcfPanel,
} from "./FlowPrimalDualIpmMcfPanel";

const graph = {
	nodes: [{ id: "a" }, { id: "b" }, { id: "c" }, { id: "d" }],
	edges: [
		{ id: "ab-0", from: "a", to: "b" },
		{ id: "ab-1", from: "a", to: "b" },
		{ id: "bc", from: "b", to: "c" },
		{ id: "da", from: "d", to: "a" },
	],
} as const;

const capacityNodes = [
	{
		auxiliary_id: "capacity:bc",
		kind: "capacity",
		original_edge_id: "bc",
	},
	{
		auxiliary_id: "capacity:ab-0",
		kind: "capacity",
		original_edge_id: "ab-0",
	},
	{
		auxiliary_id: "capacity:da",
		kind: "capacity",
		original_edge_id: "da",
	},
	{
		auxiliary_id: "capacity:ab-1",
		kind: "capacity",
		original_edge_id: "ab-1",
	},
] as const;

describe("primal-dual IPM auxiliary layout", () => {
	it("is independent of overlay publication order and separates parallel edges", () => {
		const forward = buildPrimalDualIpmAuxiliaryPositions(graph, {
			nodes: capacityNodes,
		});
		const reversed = buildPrimalDualIpmAuxiliaryPositions(graph, {
			nodes: [...capacityNodes].reverse(),
		});

		for (const node of capacityNodes) {
			expect(forward.get(node.auxiliary_id)).toEqual(
				reversed.get(node.auxiliary_id),
			);
			const position = forward.get(node.auxiliary_id);
			expect(position).toBeDefined();
			expect(position?.x).toBeGreaterThanOrEqual(54);
			expect(position?.x).toBeLessThanOrEqual(920 - 54);
			expect(position?.y).toBeGreaterThanOrEqual(54);
			expect(position?.y).toBeLessThanOrEqual(410 - 54);
		}

		const first = forward.get("capacity:ab-0");
		const second = forward.get("capacity:ab-1");
		expect(first).toBeDefined();
		expect(second).toBeDefined();
		expect(
			Math.hypot(
				(first?.x ?? 0) - (second?.x ?? 0),
				(first?.y ?? 0) - (second?.y ?? 0),
			),
		).toBeGreaterThanOrEqual(40);
	});

	it("renders the exact forest-subset ordinal even for the empty subset", () => {
		const props = {
			graph: {
				nodes: [{ id: "s" }, { id: "t" }],
				edges: [{ id: "st", from: "s", to: "t" }],
			},
			overlay: {
				stage: "inspect-forest-subset",
				seed: "7",
				mu: "16",
				beta: "256",
				gamma: "32768",
				proxy_gap: "1",
				centrality_numerator: "2",
				cycle_alpha: "0",
				forest_subset_serial: "1",
				nodes: [
					{
						auxiliary_id: "node:s",
						kind: "original",
						original_node_id: "s",
						potential: "0",
						component: "0",
						in_crossover_set: false,
					},
					{
						auxiliary_id: "node:t",
						kind: "original",
						original_node_id: "t",
						potential: "0",
						component: "1",
						in_crossover_set: false,
					},
				],
				arcs: [
					{
						auxiliary_id: "aux:st",
						original_edge_id: "st",
						from: "node:s",
						to: "node:t",
						kind: "artificial",
						flow: "1",
						slack: "2",
						resistance: "2",
						deleted: false,
						contracted: false,
						in_minor: true,
						in_tree: false,
						forest_candidate: false,
						active_cycle_sign: "0",
					},
				],
			},
		} as unknown as Parameters<typeof FlowPrimalDualIpmMcfPanel>[0];
		const emptyMarkup = renderToStaticMarkup(
			createElement(FlowPrimalDualIpmMcfPanel, props),
		);
		expect(emptyMarkup).toContain('data-ipm-forest-subset="1"');
		expect(emptyMarkup).toContain("CANDIDATE SUBSET #1 · ∅");

		const candidateProps = {
			...props,
			overlay: {
				...props.overlay,
				forest_subset_serial: "2",
				arcs: props.overlay.arcs.map((arc) => ({
					...arc,
					forest_candidate: true,
				})),
			},
		};
		const candidateMarkup = renderToStaticMarkup(
			createElement(FlowPrimalDualIpmMcfPanel, candidateProps),
		);
		expect(candidateMarkup).toContain("CANDIDATE SUBSET #2 · 1 AUX ARC");
		expect(candidateMarkup).toContain("flow-ipm-forest-candidate-rail");
		expect(candidateMarkup).toContain('data-ipm-forest-candidate="true"');
	});
});
