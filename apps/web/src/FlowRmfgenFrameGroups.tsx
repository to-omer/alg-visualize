import type { RmfgenFrameGroup } from "./flow-entity-graph-state";

export function FlowRmfgenFrameGroups({
	groups,
}: Readonly<{ groups: readonly RmfgenFrameGroup[] }>) {
	return groups.map((group) => (
		<g
			key={`rmfgen-frame:${group.frame}`}
			className="flow-rmfgen-frame"
			data-rmfgen-frame={group.frame}
		>
			<rect
				x={group.x}
				y={group.y}
				width={group.width}
				height={group.height}
				rx="12"
			/>
			<text x={group.x + 12} y={group.y + 18}>
				frame {group.frame}
			</text>
		</g>
	));
}
