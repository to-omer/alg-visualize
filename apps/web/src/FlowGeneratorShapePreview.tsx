import {
	type FlowGeneratorFixture,
	flowGeneratorFamilyDisplayName,
} from "./flow-generator-fixture";

type Point = readonly [x: number, y: number];
type PreviewGeometry = Readonly<{
	nodes: readonly Point[];
	edges: readonly (readonly [from: number, to: number])[];
}>;

const GEOMETRY = {
	"linear-layered": {
		nodes: [
			[12, 40],
			[48, 20],
			[48, 60],
			[88, 20],
			[88, 60],
			[148, 40],
		],
		edges: [
			[0, 1],
			[0, 2],
			[1, 3],
			[1, 4],
			[2, 3],
			[2, 4],
			[3, 5],
			[4, 5],
		],
	},
	"radial-cyclic": {
		nodes: [
			[80, 10],
			[125, 24],
			[142, 54],
			[80, 70],
			[18, 54],
			[35, 24],
		],
		edges: [
			[0, 1],
			[1, 2],
			[2, 3],
			[3, 4],
			[4, 5],
			[5, 0],
		],
	},
	"grid-local": {
		nodes: [
			[28, 18],
			[80, 18],
			[132, 18],
			[28, 62],
			[80, 62],
			[132, 62],
		],
		edges: [
			[0, 1],
			[1, 2],
			[3, 4],
			[4, 5],
			[0, 3],
			[1, 4],
			[2, 5],
		],
	},
	"grid-periodic": {
		nodes: [
			[28, 18],
			[80, 18],
			[132, 18],
			[28, 62],
			[80, 62],
			[132, 62],
		],
		edges: [
			[0, 1],
			[1, 2],
			[2, 0],
			[3, 4],
			[4, 5],
			[5, 3],
			[0, 3],
			[1, 4],
			[2, 5],
		],
	},
	partitioned: {
		nodes: [
			[22, 14],
			[22, 40],
			[22, 66],
			[138, 14],
			[138, 40],
			[138, 66],
		],
		edges: [
			[0, 3],
			[0, 4],
			[1, 3],
			[1, 5],
			[2, 4],
			[2, 5],
		],
	},
	hierarchical: {
		nodes: [
			[80, 10],
			[42, 38],
			[118, 38],
			[18, 68],
			[58, 68],
			[102, 68],
			[142, 68],
		],
		edges: [
			[0, 1],
			[0, 2],
			[1, 3],
			[1, 4],
			[2, 5],
			[2, 6],
		],
	},
	clustered: {
		nodes: [
			[28, 25],
			[48, 14],
			[50, 42],
			[108, 38],
			[130, 26],
			[132, 55],
		],
		edges: [
			[0, 1],
			[1, 2],
			[2, 0],
			[3, 4],
			[4, 5],
			[5, 3],
			[2, 3],
		],
	},
	"dense-spatial": {
		nodes: [
			[18, 50],
			[42, 18],
			[68, 62],
			[88, 30],
			[116, 58],
			[142, 20],
		],
		edges: [
			[0, 1],
			[0, 2],
			[0, 3],
			[1, 2],
			[1, 3],
			[1, 4],
			[2, 3],
			[2, 4],
			[3, 4],
			[3, 5],
			[4, 5],
		],
	},
	"benchmark-gadget": {
		nodes: [
			[10, 40],
			[44, 16],
			[44, 64],
			[80, 40],
			[116, 16],
			[116, 64],
			[150, 40],
		],
		edges: [
			[0, 1],
			[0, 2],
			[1, 3],
			[2, 3],
			[1, 2],
			[3, 4],
			[3, 5],
			[4, 6],
			[5, 6],
			[4, 5],
		],
	},
} as const satisfies Record<
	FlowGeneratorFixture["layout_class"],
	PreviewGeometry
>;

export const FLOW_GENERATOR_PREVIEW_LAYOUTS = Object.freeze(
	Object.keys(GEOMETRY) as FlowGeneratorFixture["layout_class"][],
);

export function FlowGeneratorShapePreview({
	fixture,
}: {
	fixture: FlowGeneratorFixture;
}) {
	const geometry = GEOMETRY[fixture.layout_class];
	return (
		<figure className="flow-generator-shape-preview">
			<svg
				viewBox="0 0 160 80"
				role="img"
				aria-label={`${flowGeneratorFamilyDisplayName(fixture.family_id)} capped shape preview; ${fixture.layout_class}`}
			>
				{geometry.edges.map(([from, to]) => {
					const source = geometry.nodes[from];
					const target = geometry.nodes[to];
					if (source === undefined || target === undefined) return null;
					return (
						<line
							key={`${from}:${to}`}
							x1={source[0]}
							y1={source[1]}
							x2={target[0]}
							y2={target[1]}
						/>
					);
				})}
				{geometry.nodes.map(([x, y]) => (
					<circle key={`${x}:${y}`} cx={x} cy={y} r="4" />
				))}
			</svg>
			<figcaption>
				<span>{fixture.layout_class}</span>
				<small>
					capped preview · at most 7 nodes / 11 edges · not the generated graph
				</small>
			</figcaption>
		</figure>
	);
}
