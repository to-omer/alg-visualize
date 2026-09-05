const LAYERED_NODE_ID = /^l(\d{3})n(\d{4})$/;
const MAX_CANVAS_LABEL_CODE_POINTS = 8;

function compactNodeId(nodeId: string): string {
	const codePoints = [...nodeId];
	if (codePoints.length <= MAX_CANVAS_LABEL_CODE_POINTS) return nodeId;
	return `${codePoints.slice(0, 4).join("")}…${codePoints.slice(-3).join("")}`;
}

/**
 * Keeps generated stable IDs available to contracts and inspection while
 * giving every node a short label that fits inside its glyph. The exact ID
 * remains available through the SVG title, Inspector, and entity navigator.
 */
export function flowNodeCanvasLabel(nodeId: string): string {
	const layered = LAYERED_NODE_ID.exec(nodeId);
	if (layered !== null) {
		return `L${Number(layered[1])}·${Number(layered[2])}`;
	}
	return compactNodeId(nodeId);
}
