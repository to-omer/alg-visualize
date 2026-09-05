import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { FlowGraphFeasibilityLayer } from "./FlowGraphFeasibilityLayer";
import { FlowGraphIdScopeProvider } from "./flow-dom-id";

function state() {
	return {
		renderData: {
			overlayViews: {
				feasibility: {
					revision: "flow-feasibility-overlay/2",
					use_kind: "precheck-only",
					domain: {
						kind: "public-input",
						nodes: [
							{ node_id: "s", public_node_id: "s" },
							{ node_id: "t", public_node_id: "t" },
						],
						edges: [
							{
								edge_id: "e",
								from_node_id: "s",
								to_node_id: "t",
								lower: "0",
								capacity: "7",
								public_route_edge_id: "e",
							},
						],
						request: {
							kind: "balance",
							required_divergences: [
								{ node_id: "s", required_divergence: "3" },
								{ node_id: "t", required_divergence: "-3" },
							],
						},
					},
					stage: "add-original-arc",
					nodes: [
						{
							node: { kind: "original", original_node_id: "s" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
						{
							node: { kind: "original", original_node_id: "t" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
						{
							node: { kind: "super-source" },
							height: "4",
							excess: "3",
							current_arc: "0",
							active: false,
							reachable: false,
						},
						{
							node: { kind: "super-sink" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
					],
					arcs: [
						{
							arc: { kind: "original", original_edge_id: "e" },
							from: { kind: "original", original_node_id: "s" },
							to: { kind: "original", original_node_id: "t" },
							capacity: "7",
							flow: "2",
							forward_residual: "5",
							reverse_residual: "2",
							focused: true,
							focused_direction: "forward",
						},
						{
							arc: {
								kind: "from-super-source",
								imbalance_node_id: "s",
							},
							from: { kind: "super-source" },
							to: { kind: "original", original_node_id: "s" },
							capacity: "3",
							flow: "1",
							forward_residual: "2",
							reverse_residual: "1",
							focused: false,
						},
					],
					active_queue: [],
					focus_arc: {
						arc: { kind: "original", original_edge_id: "e" },
						direction: "forward",
					},
					total_required: "3",
					routed: "0",
					metrics: {},
				},
			},
		},
		positions: new Map([
			["s", { x: 120, y: 270 }],
			["t", { x: 840, y: 270 }],
		]),
		layout: {
			routes: new Map([
				[
					"e",
					{
						path: "M 148 270 L 812 270",
						reversePath: "M 812 270 L 148 270",
						label: { x: 480, y: 270 },
					},
				],
			]),
		},
		plan: {
			overlayPresentation: { activeFields: ["feasibility_overlay"] },
		},
	} as never;
}

function standaloneState() {
	const base = state() as unknown as Record<string, unknown>;
	return {
		...base,
		renderData: {
			overlayViews: {
				feasibility: {
					revision: "flow-feasibility-overlay/2",
					use_kind: "anchored-recovery",
					domain: {
						kind: "standalone-transformation",
						nodes: [{ node_id: "internal-a" }, { node_id: "internal-b" }],
						edges: [
							{
								edge_id: "internal-edge",
								from_node_id: "internal-a",
								to_node_id: "internal-b",
								lower: "0",
								capacity: "7",
							},
						],
						request: {
							kind: "balance",
							required_divergences: [
								{ node_id: "internal-a", required_divergence: "1" },
								{ node_id: "internal-b", required_divergence: "-1" },
							],
						},
					},
					stage: "add-original-arc",
					nodes: [
						{
							node: { kind: "original", original_node_id: "internal-a" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
						{
							node: { kind: "original", original_node_id: "internal-b" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
						{
							node: { kind: "super-source" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
						{
							node: { kind: "super-sink" },
							height: "0",
							excess: "0",
							current_arc: "0",
							active: false,
							reachable: false,
						},
					],
					arcs: [
						{
							arc: { kind: "original", original_edge_id: "internal-edge" },
							from: { kind: "original", original_node_id: "internal-a" },
							to: { kind: "original", original_node_id: "internal-b" },
							capacity: "7",
							flow: "0",
							forward_residual: "7",
							reverse_residual: "0",
							focused: true,
							focused_direction: "forward",
						},
					],
					active_queue: [],
					focus_arc: {
						arc: { kind: "original", original_edge_id: "internal-edge" },
						direction: "forward",
					},
					total_required: "0",
					routed: "0",
					metrics: {},
				},
			},
		},
	} as never;
}

describe("feasibility main-canvas layer", () => {
	it("renders only intrinsic, source-owned SVG leaves", () => {
		const svg = renderToStaticMarkup(
			<FlowGraphIdScopeProvider scope="feasibility-test">
				<svg>
					<title>Feasibility construction</title>
					<FlowGraphFeasibilityLayer state={state()} />
				</svg>
			</FlowGraphIdScopeProvider>,
		);

		expect(svg).toContain('data-feasibility-stage="add-original-arc"');
		expect(svg).toContain('data-feasibility-use="precheck-only"');
		expect(svg).toContain("PRECHECK · ADD ORIGINAL ARC");
		expect(svg).toContain('data-feasibility-node="artificial:super-source"');
		expect(svg).toContain('data-feasibility-node="artificial:super-sink"');
		expect(svg).toContain('data-feasibility-arc="original:e"');
		expect(svg).toContain("FLOW 2   CAPACITY 7");
		expect(svg).toContain('data-overlay-contribution="feasibility_overlay"');
		expect(svg).toContain('data-overlay-feature-bundle="feasibility"');
		expect(svg).toContain('data-overlay-residual-direction="forward"');
		expect(svg).toContain('data-overlay-entity-kind="residual-arc"');
		expect(svg).toContain('data-overlay-entity-id="e"');
		expect(svg).toContain('data-overlay-entity-kind="auxiliary-edge"');
		expect(svg).toContain('data-overlay-entity-kind="auxiliary-node"');
		expect(svg).toContain('class="flow-feasibility-status"');
		expect(svg).not.toContain('data-overlay-entity-id="feasibility:stage"');
	});

	it("renders a standalone transformed recovery graph instead of hiding its entities", () => {
		const svg = renderToStaticMarkup(
			<FlowGraphIdScopeProvider scope="feasibility-transformed-test">
				<svg aria-label="Transformed feasibility network">
					<FlowGraphFeasibilityLayer state={standaloneState()} />
				</svg>
			</FlowGraphIdScopeProvider>,
		);
		expect(svg).toContain(
			'data-feasibility-domain="standalone-transformation"',
		);
		expect(svg).toContain('data-feasibility-domain-node-count="2"');
		expect(svg).toContain('data-feasibility-rendered-original-arc-count="1"');
		expect(svg).toContain("ALGORITHM-OWNED RECOVERY NETWORK");
		expect(svg).toContain('data-feasibility-domain-node="internal-a"');
		expect(svg).toContain('data-feasibility-domain-node="internal-b"');
		expect(svg).toContain('data-feasibility-arc="original:internal-edge"');
		expect(svg).toContain('data-overlay-entity-kind="auxiliary-residual-arc"');
		expect(svg).not.toContain(
			'data-feasibility-node="artificial:super-source"',
		);
		expect(svg).not.toContain('data-feasibility-node="artificial:super-sink"');
	});
});
