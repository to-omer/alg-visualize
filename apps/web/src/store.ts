import { create } from "zustand";

import type { EngineFrame } from "./engine-types";
import { type AlgorithmId, defaultScenario } from "./scenario";

export type PlaybackStatus =
	| "idle"
	| "loading"
	| "ready"
	| "playing"
	| "paused"
	| "error";

type VisualizerStore = {
	algorithm: AlgorithmId;
	cursor: number;
	error: string | undefined;
	frame: EngineFrame | undefined;
	itemCount: number;
	scenario: string;
	status: PlaybackStatus;
	clearFrame: () => void;
	setAlgorithm: (algorithm: AlgorithmId) => void;
	setCursor: (cursor: number) => void;
	setError: (error?: string) => void;
	setFrame: (frame: EngineFrame) => void;
	setItemCount: (itemCount: number) => void;
	setScenario: (scenario: string) => void;
	setStatus: (status: PlaybackStatus) => void;
};

export const useVisualizerStore = create<VisualizerStore>((set) => ({
	algorithm: "avl",
	cursor: 0,
	error: undefined,
	frame: undefined,
	itemCount: 0,
	scenario: defaultScenario(),
	status: "idle",
	clearFrame: () =>
		set({ cursor: 0, error: undefined, frame: undefined, itemCount: 0 }),
	setAlgorithm: (algorithm) => set({ algorithm }),
	setCursor: (cursor) => set({ cursor }),
	setError: (error) => set({ error }),
	setFrame: (frame) => set({ cursor: frame.itemIndex, frame }),
	setItemCount: (itemCount) => set({ itemCount }),
	setScenario: (scenario) => set({ scenario }),
	setStatus: (status) => set({ status }),
}));
