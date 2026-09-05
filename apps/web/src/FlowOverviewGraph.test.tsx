import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
	FlowOverviewGraph,
	flowOverviewAccessibleDescription,
} from "./FlowOverviewGraph";
import type { FlowOverviewRenderPlan } from "./flow-render-plan";

function overviewPlan(): FlowOverviewRenderPlan {
	const route = {
		edgeId: "aggregate",
		from: "cluster:0:0",
		to: "cluster:0:1",
		path: "M 100 100 L 300 100",
		reversePath: "M 300 100 L 100 100",
		label: { x: 200, y: 100 },
		labelWidth: 20,
		labelBoxWidth: 20,
		labelHeight: 24,
		labelYOffset: 0,
		labelCollisionFree: true,
		labelAnchor: { x: 200, y: 100 },
		laneToken: { x: 200, y: 100 },
		laneTokenAngle: 0,
		routeMidpoint: { x: 200, y: 100 },
		parallelIndex: 1,
		parallelCount: 1,
		residualForwardLabel: { x: 200, y: 90 },
		residualReverseLabel: { x: 200, y: 110 },
		selfLoop: false,
	};
	return {
		kind: "overview",
		level: "overview",
		grid: { columns: 2, rows: 1 },
		clusters: [
			{
				id: "cluster:0:0",
				x: 100,
				y: 100,
				memberCount: 3,
				sourceSide: "all",
				terminal: "source",
				terminalLabel: "s",
				balance: "none",
				supplyCount: 0,
				demandCount: 0,
				netBalance: 0n,
				containsSupernode: false,
				traceCount: 1,
				traceIdentities: ["node:s"],
				changeCount: 1,
				changedIdentities: ["node:s"],
			},
			{
				id: "cluster:0:1",
				x: 300,
				y: 100,
				memberCount: 2,
				sourceSide: "none",
				terminal: "sink",
				terminalLabel: "t",
				balance: "none",
				supplyCount: 0,
				demandCount: 0,
				netBalance: 0n,
				containsSupernode: false,
				traceCount: 0,
				traceIdentities: [],
				changeCount: 0,
				changedIdentities: [],
			},
		],
		originalEdges: [
			{
				id: "overview-original:source-to-sink",
				from: "cluster:0:0",
				to: "cluster:0:1",
				route,
				edgeCount: 4,
				capacity: 12n,
				flow: 6n,
				costKind: "positive",
				minimumCost: 2n,
				maximumCost: 5n,
				activeCount: 1,
				fixedCount: 0,
				cutCount: 0,
				traceCount: 1,
				traceIdentities: ["edge:e0"],
				changeCount: 1,
				changedIdentities: ["edge:e0"],
			},
		],
		residualArcs: [],
	};
}

function graph(problemKind: "max-flow" | "min-cost-flow") {
	return (
		<FlowOverviewGraph
			plan={overviewPlan()}
			problemKind={problemKind}
			viewMode="original"
			frameGroups={[]}
			onSelectionChange={() => undefined}
			selection={undefined}
			canvasBinding={
				{
					ref: () => undefined,
					viewBox: "0 0 960 540",
					"data-flow-zoom": "1",
					onPointerDown: () => undefined,
					onPointerMove: () => undefined,
					onPointerLeave: () => undefined,
					onPointerUp: () => undefined,
					onPointerCancel: () => undefined,
					onClickCapture: () => undefined,
				} as never
			}
		/>
	);
}

function render(problemKind: "max-flow" | "min-cost-flow"): string {
	return renderToStaticMarkup(graph(problemKind));
}

describe("FlowOverviewGraph", () => {
	it("uses the same capacity and flow channels without cost UI for Max Flow", () => {
		const markup = render("max-flow");
		expect(markup).toContain("flow-original-edge flow-overview-original-edge");
		expect(markup).toContain('class="flow-capacity-rail"');
		expect(markup).toContain('class="flow-flow-line"');
		expect(markup).not.toContain("flow-cost-rail");
		expect(markup).not.toContain(" · cost 2…5");
		expect(markup).not.toMatch(/[ぁ-んァ-ヶ一-龠]/);
	});

	it("uses a continuous outer cost rail only for Min-Cost Flow", () => {
		const markup = render("min-cost-flow");
		expect(markup).toContain('class="flow-cost-rail flow-cost-positive"');
		expect(markup).toContain("--flow-cost-intensity:1");
		expect(markup).toContain(" · cost 2…5");
		expect(markup).not.toContain("flow-cost-magnitude-");
		expect(markup).not.toContain("data-cost-magnitude");
	});

	it("renders exact event focus as a visible ring and route outline", () => {
		const markup = render("max-flow");
		expect(markup).toContain('data-event-identities="node:s"');
		expect(markup).toContain('class="flow-event-touch-node-ring"');
		expect(markup).toContain('data-event-identities="edge:e0"');
		expect(markup).toContain('class="flow-event-touch-edge-outline"');
		expect(markup).toContain('data-changed-identities="node:s"');
		expect(markup).toContain('class="flow-event-change-node-ring"');
		expect(markup).toContain('data-changed-identities="edge:e0"');
		expect(markup).toContain('class="flow-event-change-edge-outline"');
	});

	it("describes cost only for the problem family that defines it", () => {
		const context = {
			hasBalances: false,
			hasSupernode: false,
			hasFrames: false,
			hasFixedEdges: false,
		};
		expect(
			flowOverviewAccessibleDescription({
				...context,
				problemKind: "max-flow",
			}),
		).not.toContain("cost");
		expect(
			flowOverviewAccessibleDescription({
				...context,
				problemKind: "min-cost-flow",
			}),
		).toContain("continuous intensity");
	});

	it("scopes every DOM ID and SVG fragment reference per retained graph", () => {
		const markup = renderToStaticMarkup(
			<div>
				{graph("max-flow")}
				{graph("min-cost-flow")}
			</div>,
		);
		const ids = [...markup.matchAll(/\sid="([^"]+)"/g)].map(
			(match) => match[1] as string,
		);
		expect(ids.length).toBeGreaterThan(20);
		expect(new Set(ids).size).toBe(ids.length);

		const referencedIds = [
			...[...markup.matchAll(/url\(#([^)]+)\)/g)].map(
				(match) => match[1] as string,
			),
			...[
				...markup.matchAll(/aria-(?:labelledby|describedby)="([^"]+)"/g),
			].flatMap((match) => (match[1] as string).split(/\s+/)),
		];
		for (const referencedId of referencedIds) {
			expect(ids, referencedId).toContain(referencedId);
		}
	});
});
