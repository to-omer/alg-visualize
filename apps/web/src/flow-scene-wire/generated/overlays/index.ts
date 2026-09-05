// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

import type { FlowCurrentSceneV9 } from "../FlowCurrentSceneV9.js";

export type FlowSceneV9OverlayField = Extract<
	keyof FlowCurrentSceneV9,
	`${string}_overlay`
>;
export type FlowSceneV9OverlayDecoder = (value: unknown) => unknown;

import { decodeStructure as decode0 } from "./augmenting_electrical_overlay.js";
import { decodeStructure as decode1 } from "./binary_blocking_overlay.js";
import { decodeStructure as decode2 } from "./cancel_tighten_overlay.js";
import { decodeStructure as decode3 } from "./convex_cost_overlay.js";
import { decodeStructure as decode4 } from "./convex_network_simplex_overlay.js";
import { decodeStructure as decode5 } from "./deterministic_almost_linear_overlay.js";
import { decodeStructure as decode6 } from "./double_scaling_overlay.js";
import { decodeStructure as decode7 } from "./dual_network_simplex_overlay.js";
import { decodeStructure as decode8 } from "./dynamic_eibfs_overlay.js";
import { decodeStructure as decode9 } from "./eibfs_overlay.js";
import { decodeStructure as decode10 } from "./electrical_flow_overlay.js";
import { decodeStructure as decode11 } from "./electrical_ipm_mcf_overlay.js";
import { decodeStructure as decode12 } from "./enhanced_capacity_scaling_overlay.js";
import { decodeStructure as decode13 } from "./feasibility_overlay.js";
import { decodeStructure as decode14 } from "./flow_framework_mcf_overlay.js";
import { decodeStructure as decode15 } from "./interior_point_max_flow_overlay.js";
import { decodeStructure as decode16 } from "./minimum_ratio_cycle_mcf_overlay.js";
import { decodeStructure as decode17 } from "./minimum_ratio_cycle_overlay.js";
import { decodeStructure as decode18 } from "./orlin_max_flow_overlay.js";
import { decodeStructure as decode19 } from "./orlin_mcf_overlay.js";
import { decodeStructure as decode20 } from "./parametric_overlay.js";
import { decodeStructure as decode21 } from "./polynomial_dual_simplex_overlay.js";
import { decodeStructure as decode22 } from "./polynomial_primal_simplex_overlay.js";
import { decodeStructure as decode23 } from "./prediction_assisted_epsilon_overlay.js";
import { decodeStructure as decode24 } from "./primal_dual_ipm_mcf_overlay.js";
import { decodeStructure as decode25 } from "./randomized_almost_linear_mcf_overlay.js";
import { decodeStructure as decode26 } from "./randomized_almost_linear_overlay.js";
import { decodeStructure as decode27 } from "./relaxed_mndc_overlay.js";
import { decodeStructure as decode28 } from "./tardos_framework_overlay.js";
import { decodeStructure as decode29 } from "./weighted_augmenting_paths_overlay.js";
import { decodeStructure as decode30 } from "./weighted_push_relabel_shortcut_overlay.js";

export const FLOW_SCENE_V9_OVERLAY_DECODERS: ReadonlyArray<
	readonly [field: FlowSceneV9OverlayField, decode: FlowSceneV9OverlayDecoder]
> = [
	["augmenting_electrical_overlay", decode0],
	["binary_blocking_overlay", decode1],
	["cancel_tighten_overlay", decode2],
	["convex_cost_overlay", decode3],
	["convex_network_simplex_overlay", decode4],
	["deterministic_almost_linear_overlay", decode5],
	["double_scaling_overlay", decode6],
	["dual_network_simplex_overlay", decode7],
	["dynamic_eibfs_overlay", decode8],
	["eibfs_overlay", decode9],
	["electrical_flow_overlay", decode10],
	["electrical_ipm_mcf_overlay", decode11],
	["enhanced_capacity_scaling_overlay", decode12],
	["feasibility_overlay", decode13],
	["flow_framework_mcf_overlay", decode14],
	["interior_point_max_flow_overlay", decode15],
	["minimum_ratio_cycle_mcf_overlay", decode16],
	["minimum_ratio_cycle_overlay", decode17],
	["orlin_max_flow_overlay", decode18],
	["orlin_mcf_overlay", decode19],
	["parametric_overlay", decode20],
	["polynomial_dual_simplex_overlay", decode21],
	["polynomial_primal_simplex_overlay", decode22],
	["prediction_assisted_epsilon_overlay", decode23],
	["primal_dual_ipm_mcf_overlay", decode24],
	["randomized_almost_linear_mcf_overlay", decode25],
	["randomized_almost_linear_overlay", decode26],
	["relaxed_mndc_overlay", decode27],
	["tardos_framework_overlay", decode28],
	["weighted_augmenting_paths_overlay", decode29],
	["weighted_push_relabel_shortcut_overlay", decode30],
] as const;
