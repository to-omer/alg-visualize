import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { FlowGraphOverlayOwnedLeaves } from "./FlowGraphOverlayOwnedLeaves";

function state(activeFields: readonly string[]) {
	return {
		plan: { overlayPresentation: { activeFields } },
	} as never;
}

describe("FlowGraphOverlayOwnedLeaves", () => {
	it("annotates the existing painted leaves without creating a fallback glyph", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Owned electrical node feature</title>
				<FlowGraphOverlayOwnedLeaves
					state={state(["electrical_flow_overlay"])}
					bundle="node-continuous"
					entity={{ kind: "node", id: "v3" }}
					owners={[
						{ overlay: "electrical_flow_overlay", role: "nodes.potential" },
					]}
				>
					<g className="feature">
						<title>Potential</title>
						<circle className="potential-ring" r="8" />
						<text>φ 7</text>
					</g>
				</FlowGraphOverlayOwnedLeaves>
			</svg>,
		);
		expect(svg.match(/data-overlay-contribution=/gu)).toHaveLength(2);
		expect(svg).toContain('data-overlay-feature-bundle="node-continuous"');
		expect(svg).toContain('data-overlay-entity-id="v3"');
		expect(svg).toContain(
			'data-overlay-role="electrical_flow_overlay:nodes.potential"',
		);
		expect(svg).not.toMatch(/<g[^>]+data-overlay-contribution/gu);
		expect(svg).not.toMatch(/<title[^>]+data-overlay-contribution/gu);
	});

	it("publishes multiple exact owners on one shared native feature", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Shared EIBFS node feature</title>
				<FlowGraphOverlayOwnedLeaves
					state={state(["eibfs_overlay", "dynamic_eibfs_overlay"])}
					bundle="node-search"
					entity={{ kind: "node", id: "u" }}
					owners={[
						{ overlay: "eibfs_overlay", role: "nodes.membership" },
						{ overlay: "dynamic_eibfs_overlay", role: "repaired_forest" },
					]}
				>
					<circle r="7" />
				</FlowGraphOverlayOwnedLeaves>
			</svg>,
		);
		expect(svg).toContain(
			'data-overlay-contributions="eibfs_overlay dynamic_eibfs_overlay"',
		);
		expect(svg).not.toContain("data-overlay-contribution=");
	});

	it("renders no provenance leaf when the feature itself has no painted child", () => {
		const svg = renderToStaticMarkup(
			<svg>
				<title>Empty overlay feature</title>
				<FlowGraphOverlayOwnedLeaves
					state={state(["binary_blocking_overlay"])}
					bundle="node-optimization"
					entity={{ kind: "node", id: "v" }}
					owners={[
						{ overlay: "binary_blocking_overlay", role: "nodes.component" },
					]}
				>
					<g />
				</FlowGraphOverlayOwnedLeaves>
			</svg>,
		);
		expect(svg).not.toContain("data-overlay-contribution");
	});

	it("fails closed when an owner claims an undeclared bundle", () => {
		expect(() =>
			renderToStaticMarkup(
				<svg>
					<title>Invalid overlay ownership</title>
					<FlowGraphOverlayOwnedLeaves
						state={state(["electrical_flow_overlay"])}
						bundle="node-search"
						entity={{ kind: "node", id: "v" }}
						owners={[{ overlay: "electrical_flow_overlay", role: "nodes" }]}
					>
						<circle r="7" />
					</FlowGraphOverlayOwnedLeaves>
				</svg>,
			),
		).toThrow(/does not declare feature bundle node-search/u);
	});
});
