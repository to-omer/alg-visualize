import { Container, Graphics, Text } from "pixi.js";

import type { Position } from "./graph-layout";
import {
	type CanvasController,
	displayResolution,
	type NodeEmphasis,
} from "./pixi-controller";

export type ScreenMarkers = {
	active: Container;
	activeEmphasis: NodeEmphasis | undefined;
	activeLabel: Text;
	activeShape: Graphics;
	selected: Graphics;
};

export function createScreenMarkers(): ScreenMarkers {
	const active = new Container();
	const activeShape = new Graphics();
	const activeLabel = new Text({
		resolution: displayResolution(),
		text: "",
		style: {
			fill: 0xffc36a,
			fontFamily: "ui-monospace, monospace",
			fontSize: 12,
			fontWeight: "600",
		},
	});
	activeLabel.anchor.set(0.5);
	activeLabel.y = -18;
	active.addChild(activeShape, activeLabel);
	active.eventMode = "none";
	active.visible = false;
	const selected = new Graphics()
		.circle(0, 0, 13)
		.stroke({ color: 0xf4fbff, width: 2.5 });
	selected.visible = false;
	return {
		active,
		activeEmphasis: undefined,
		activeLabel,
		activeShape,
		selected,
	};
}

function screenPosition(controller: CanvasController, position: Position) {
	return {
		x: controller.world.x + position.x * controller.world.scale.x,
		y: controller.world.y + position.y * controller.world.scale.y,
	};
}

export function updateScreenMarkers(
	markers: ScreenMarkers,
	controller: CanvasController,
	activePosition: Position | undefined,
	selectedPosition: Position | undefined,
	activeEmphasis: NodeEmphasis,
) {
	if (activePosition === undefined || controller.activeLabel === undefined) {
		markers.active.visible = false;
		controller.host.dataset.activeMarkerVisible = "false";
	} else {
		if (markers.activeEmphasis !== activeEmphasis) {
			const color =
				activeEmphasis === "mutation"
					? 0xff8a65
					: activeEmphasis === "visited"
						? 0x53c7c5
						: 0xffc15c;
			markers.activeShape.clear().circle(0, 0, 10).stroke({ color, width: 3 });
			markers.activeLabel.style.fill = color;
			markers.activeEmphasis = activeEmphasis;
			controller.host.dataset.activeMarkerEmphasis = activeEmphasis;
		}
		const position = screenPosition(controller, activePosition);
		markers.active.position.set(position.x, position.y);
		markers.activeLabel.text = controller.activeLabel;
		markers.active.visible = true;
		controller.host.dataset.activeMarkerVisible = "true";
	}
	if (selectedPosition === undefined) {
		markers.selected.visible = false;
		controller.host.dataset.selectionMarkerVisible = "false";
	} else {
		const position = screenPosition(controller, selectedPosition);
		markers.selected.position.set(position.x, position.y);
		markers.selected.visible = true;
		controller.host.dataset.selectionMarkerVisible = "true";
	}
}
