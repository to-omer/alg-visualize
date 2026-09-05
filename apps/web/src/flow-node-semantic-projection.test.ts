import { describe, expect, it } from "vitest";
import { capacityScalingTraceLabelPrefix } from "./flow-node-semantic-projection";

describe("capacityScalingTraceLabelPrefix", () => {
	it.each([
		"capacity-scaling-mcf.inspect-residual-arc",
		"capacity-scaling-mcf.shortest-eligible-path",
		"capacity-scaling-mcf.no-eligible-deficit",
		"excess-scaling-mcf.inspect-residual-arc",
		"excess-scaling-mcf.shortest-large-excess-path",
		"excess-scaling-mcf.no-reachable-large-deficit",
	])("labels shortest-path state as reduced distance: %s", (catalogId) => {
		expect(capacityScalingTraceLabelPrefix(catalogId)).toBe("d̄");
	});

	it.each([
		"capacity-scaling-mcf.initialize-potentials",
		"capacity-scaling-mcf.update-potentials",
		"capacity-scaling-mcf.augment",
		"excess-scaling-mcf.initialize-potentials",
		"excess-scaling-mcf.update-potentials",
		"excess-scaling-mcf.augment-exact-delta",
	])("labels dual-state boundaries as node potentials: %s", (catalogId) => {
		expect(capacityScalingTraceLabelPrefix(catalogId)).toBe("π");
	});

	it("does not relabel nested feasibility events", () => {
		expect(
			capacityScalingTraceLabelPrefix("feasibility.inspect-residual-arc"),
		).toBeUndefined();
	});
});
