// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.
// Do not edit manually.

export const FLOW_SCENE_V9_SCHEMA: unknown = {
  "$defs": {
    "AssignmentObjectiveV1": {
      "oneOf": [
        {
          "const": "minimize",
          "type": "string"
        },
        {
          "const": "maximize",
          "type": "string"
        }
      ]
    },
    "FlowAlgorithmSelectionV1": {
      "additionalProperties": false,
      "properties": {
        "config": {
          "additionalProperties": true,
          "type": "object"
        },
        "id": {
          "type": "string"
        }
      },
      "required": [
        "id",
        "config"
      ],
      "type": "object"
    },
    "FlowAlgorithmStepContractV1": {
      "additionalProperties": false,
      "properties": {
        "detail": {
          "$ref": "#/$defs/FlowDetailStepCapabilityV1"
        },
        "operation_availability": {
          "$ref": "#/$defs/FlowStepAvailabilityV1"
        },
        "operation_unit": {
          "type": "string"
        },
        "phase_availability": {
          "$ref": "#/$defs/FlowStepAvailabilityV1"
        },
        "phase_unit": {
          "type": "string"
        },
        "primary_work": {
          "$ref": "#/$defs/FlowPrimaryWorkV1"
        }
      },
      "required": [
        "phase_unit",
        "phase_availability",
        "operation_unit",
        "operation_availability",
        "detail",
        "primary_work"
      ],
      "type": "object"
    },
    "FlowAssignmentLabelV1": {
      "additionalProperties": false,
      "properties": {
        "label": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "label"
      ],
      "type": "object"
    },
    "FlowAssignmentPairV1": {
      "additionalProperties": false,
      "properties": {
        "agent": {
          "type": "string"
        },
        "cost": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "task": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "agent",
        "task",
        "cost"
      ],
      "type": "object"
    },
    "FlowAugmentingElectricalEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "backward_residual": {
          "type": "string"
        },
        "boost_segments": {
          "type": "string"
        },
        "central_flow": {
          "type": "string"
        },
        "congestion": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "electrical_current": {
          "type": "string"
        },
        "extraction_central_scaled": {
          "type": [
            "string",
            "null"
          ]
        },
        "extraction_out_of_sink": {
          "type": [
            "string",
            "null"
          ]
        },
        "extraction_toward_source": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "forward_residual": {
          "type": "string"
        },
        "resistance": {
          "type": "string"
        },
        "rounded_central_flow": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "edge_id",
        "central_flow",
        "electrical_current",
        "forward_residual",
        "backward_residual",
        "congestion",
        "resistance",
        "boost_segments"
      ],
      "type": "object"
    },
    "FlowAugmentingElectricalExtractionArcKindV1": {
      "oneOf": [
        {
          "const": "central",
          "type": "string"
        },
        {
          "const": "toward-source",
          "type": "string"
        },
        {
          "const": "out-of-sink",
          "type": "string"
        }
      ]
    },
    "FlowAugmentingElectricalExtractionArcV1": {
      "additionalProperties": false,
      "properties": {
        "edge": {
          "type": "string"
        },
        "kind": {
          "$ref": "#/$defs/FlowAugmentingElectricalExtractionArcKindV1"
        }
      },
      "required": [
        "edge",
        "kind"
      ],
      "type": "object"
    },
    "FlowAugmentingElectricalNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "coupling_violation": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        },
        "target_source_side": {
          "type": "boolean"
        }
      },
      "required": [
        "node_id",
        "potential",
        "coupling_violation",
        "target_source_side"
      ],
      "type": "object"
    },
    "FlowAugmentingElectricalOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_discrete_amount": {
          "type": [
            "string",
            "null"
          ]
        },
        "active_extraction_cycle": {
          "items": {
            "$ref": "#/$defs/FlowAugmentingElectricalExtractionArcV1"
          },
          "type": "array"
        },
        "active_pivot_node": {
          "type": [
            "string",
            "null"
          ]
        },
        "active_working_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "active_working_path": {
          "items": {
            "$ref": "#/$defs/FlowAugmentingElectricalWorkingArcV1"
          },
          "type": "array"
        },
        "alpha": {
          "type": "string"
        },
        "congestion_l3": {
          "type": "string"
        },
        "congestion_l4": {
          "type": "string"
        },
        "coupling_l2": {
          "type": "string"
        },
        "current_value": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowAugmentingElectricalEdgeStateV1"
          },
          "type": "array"
        },
        "electrical_energy": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowAugmentingElectricalNodeStateV1"
          },
          "type": "array"
        },
        "original_target": {
          "type": "string"
        },
        "remaining": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowAugmentingElectricalStageV1"
        },
        "transformed_target": {
          "type": "string"
        },
        "working_edges": {
          "type": "string"
        },
        "working_nodes": {
          "type": "string"
        },
        "working_target": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "original_target",
        "transformed_target",
        "working_target",
        "current_value",
        "alpha",
        "remaining",
        "electrical_energy",
        "congestion_l3",
        "congestion_l4",
        "coupling_l2",
        "working_nodes",
        "working_edges",
        "active_working_path",
        "active_extraction_cycle",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowAugmentingElectricalStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "build-directed-reduction",
          "type": "string"
        },
        {
          "const": "add-preconditioning",
          "type": "string"
        },
        {
          "const": "install-target-cut",
          "type": "string"
        },
        {
          "const": "solve-electrical-direction",
          "type": "string"
        },
        {
          "const": "boost-high-energy-arc",
          "type": "string"
        },
        {
          "const": "augment-primal-dual",
          "type": "string"
        },
        {
          "const": "fix-coupling",
          "type": "string"
        },
        {
          "const": "collapse-boost-paths",
          "type": "string"
        },
        {
          "const": "round-central-flow",
          "type": "string"
        },
        {
          "const": "cleanup-augmenting-path",
          "type": "string"
        },
        {
          "const": "extract-directed-flow",
          "type": "string"
        },
        {
          "const": "cancel-extraction-cycle",
          "type": "string"
        },
        {
          "const": "round-directed-flow",
          "type": "string"
        },
        {
          "const": "check-certificate",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowAugmentingElectricalWorkingArcV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge": {
          "type": "string"
        },
        "flow_after": {
          "type": "string"
        },
        "from_node": {
          "type": "string"
        },
        "to_node": {
          "type": "string"
        }
      },
      "required": [
        "edge",
        "direction",
        "from_node",
        "to_node",
        "flow_after"
      ],
      "type": "object"
    },
    "FlowBinaryBlockingNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "component": {
          "type": "string"
        },
        "distance": {
          "type": [
            "string",
            "null"
          ]
        },
        "node_id": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "component"
      ],
      "type": "object"
    },
    "FlowBinaryBlockingOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "admissible_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "base_zero_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "delivered": {
          "type": "string"
        },
        "delta": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowBinaryBlockingNodeStateV1"
          },
          "type": "array"
        },
        "special_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "stage": {
          "$ref": "#/$defs/FlowBinaryBlockingStageV1"
        },
        "upper_bound": {
          "type": "string"
        },
        "zero_admissible_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        }
      },
      "required": [
        "stage",
        "upper_bound",
        "delta",
        "delivered",
        "nodes",
        "base_zero_arcs",
        "special_arcs",
        "admissible_arcs",
        "zero_admissible_arcs"
      ],
      "type": "object"
    },
    "FlowBinaryBlockingStageV1": {
      "oneOf": [
        {
          "const": "analyzing",
          "type": "string"
        },
        {
          "const": "analyzed",
          "type": "string"
        },
        {
          "const": "contracted",
          "type": "string"
        },
        {
          "const": "complete",
          "type": "string"
        }
      ]
    },
    "FlowBinaryBlockingTerminationV1": {
      "oneOf": [
        {
          "const": "blocking",
          "type": "string"
        },
        {
          "const": "delta-reached",
          "type": "string"
        }
      ]
    },
    "FlowBipartiteAdapterV1": {
      "additionalProperties": false,
      "properties": {
        "sink": {
          "type": "string"
        },
        "source": {
          "type": "string"
        }
      },
      "required": [
        "source",
        "sink"
      ],
      "type": "object"
    },
    "FlowBipartiteMatchingPairV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "left": {
          "type": "string"
        },
        "right": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "left",
        "right"
      ],
      "type": "object"
    },
    "FlowCancelTightenNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "potential": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "rank": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "node_id",
        "potential"
      ],
      "type": "object"
    },
    "FlowCancelTightenOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_cycle": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "admissible_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "delta": {
          "type": [
            "string",
            "null"
          ]
        },
        "epsilon": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "inspected_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowCancelTightenNodeStateV1"
          },
          "type": "array"
        },
        "phase": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowCancelTightenStageV1"
        }
      },
      "required": [
        "stage",
        "epsilon",
        "phase",
        "nodes",
        "admissible_arcs",
        "active_cycle",
        "inspected_arcs"
      ],
      "type": "object"
    },
    "FlowCancelTightenStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "initialize",
          "type": "string"
        },
        {
          "const": "begin-phase",
          "type": "string"
        },
        {
          "const": "inspect-cycle-arc",
          "type": "string"
        },
        {
          "const": "select-cycle",
          "type": "string"
        },
        {
          "const": "cancel-cycle",
          "type": "string"
        },
        {
          "const": "inspect-rank-arc",
          "type": "string"
        },
        {
          "const": "tighten",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowConvexCostArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "segment": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "segment",
        "direction"
      ],
      "type": "object"
    },
    "FlowConvexCostEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "base_cost_at_zero": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "forward_marginal_cost": {
          "type": [
            "string",
            "null"
          ]
        },
        "reverse_marginal_cost": {
          "type": [
            "string",
            "null"
          ]
        },
        "segments": {
          "items": {
            "$ref": "#/$defs/FlowConvexCostSegmentStateV1"
          },
          "type": "array"
        },
        "total_cost": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "base_cost_at_zero",
        "flow",
        "total_cost",
        "segments"
      ],
      "type": "object"
    },
    "FlowConvexCostOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_cycle": {
          "items": {
            "$ref": "#/$defs/FlowConvexCostArcRefV1"
          },
          "type": "array"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowConvexCostEdgeStateV1"
          },
          "type": "array"
        },
        "eligible_arcs": {
          "items": {
            "$ref": "#/$defs/FlowConvexCostArcRefV1"
          },
          "type": "array"
        },
        "scale": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowConvexCostStageV1"
        }
      },
      "required": [
        "stage",
        "edges",
        "active_cycle"
      ],
      "type": "object"
    },
    "FlowConvexCostSegmentStateV1": {
      "additionalProperties": false,
      "properties": {
        "end_flow": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "marginal_cost": {
          "type": "string"
        },
        "segment": {
          "type": "string"
        },
        "start_flow": {
          "type": "string"
        }
      },
      "required": [
        "segment",
        "start_flow",
        "end_flow",
        "flow",
        "marginal_cost"
      ],
      "type": "object"
    },
    "FlowConvexCostSegmentV1": {
      "additionalProperties": false,
      "properties": {
        "end_flow": {
          "type": "string"
        },
        "marginal_cost": {
          "type": "string"
        }
      },
      "required": [
        "end_flow",
        "marginal_cost"
      ],
      "type": "object"
    },
    "FlowConvexCostStageV1": {
      "oneOf": [
        {
          "const": "initialize",
          "type": "string"
        },
        {
          "const": "select-minimum-mean-cycle",
          "type": "string"
        },
        {
          "const": "cancel-cycle",
          "type": "string"
        },
        {
          "const": "start-scale",
          "type": "string"
        },
        {
          "const": "saturate-marginal",
          "type": "string"
        },
        {
          "const": "inspect-marginal-arc",
          "type": "string"
        },
        {
          "const": "shortest-path",
          "type": "string"
        },
        {
          "const": "update-potentials",
          "type": "string"
        },
        {
          "const": "augment",
          "type": "string"
        },
        {
          "const": "complete-scale",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowConvexCostV1": {
      "additionalProperties": false,
      "properties": {
        "base_cost_at_zero": {
          "type": "string"
        },
        "segments": {
          "items": {
            "$ref": "#/$defs/FlowConvexCostSegmentV1"
          },
          "type": "array"
        }
      },
      "required": [
        "base_cost_at_zero",
        "segments"
      ],
      "type": "object"
    },
    "FlowConvexNetworkSimplexArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "entity_id": {
          "type": "string"
        },
        "segment": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "entity_id",
        "direction"
      ],
      "type": "object"
    },
    "FlowConvexNetworkSimplexArtificialEdgeV1": {
      "additionalProperties": false,
      "properties": {
        "basis": {
          "$ref": "#/$defs/FlowConvexNetworkSimplexBasisV1"
        },
        "entering": {
          "type": "boolean"
        },
        "entity_id": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "in_cycle": {
          "type": "boolean"
        },
        "leaving": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "source": {
          "type": "string"
        },
        "target": {
          "type": "string"
        }
      },
      "required": [
        "entity_id",
        "node_id",
        "source",
        "target",
        "flow",
        "basis",
        "in_cycle",
        "entering",
        "leaving"
      ],
      "type": "object"
    },
    "FlowConvexNetworkSimplexBasisV1": {
      "oneOf": [
        {
          "const": "tree",
          "type": "string"
        },
        {
          "const": "breakpoint",
          "type": "string"
        }
      ]
    },
    "FlowConvexNetworkSimplexEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_segment": {
          "type": [
            "string",
            "null"
          ]
        },
        "basis": {
          "$ref": "#/$defs/FlowConvexNetworkSimplexBasisV1"
        },
        "edge_id": {
          "type": "string"
        },
        "entering": {
          "type": "boolean"
        },
        "in_cycle": {
          "type": "boolean"
        },
        "leaving": {
          "type": "boolean"
        }
      },
      "required": [
        "edge_id",
        "basis",
        "in_cycle",
        "entering",
        "leaving"
      ],
      "type": "object"
    },
    "FlowConvexNetworkSimplexNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "entity_id": {
          "type": "string"
        },
        "parent": {
          "type": [
            "string",
            "null"
          ]
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "entity_id",
        "potential"
      ],
      "type": "object"
    },
    "FlowConvexNetworkSimplexOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "artificial_cost": {
          "type": "string"
        },
        "artificial_edges": {
          "items": {
            "$ref": "#/$defs/FlowConvexNetworkSimplexArtificialEdgeV1"
          },
          "type": "array"
        },
        "cycle": {
          "items": {
            "$ref": "#/$defs/FlowConvexNetworkSimplexArcRefV1"
          },
          "type": "array"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowConvexNetworkSimplexEdgeStateV1"
          },
          "type": "array"
        },
        "entering": {
          "$ref": "#/$defs/FlowConvexNetworkSimplexArcRefV1"
        },
        "leaving": {
          "$ref": "#/$defs/FlowConvexNetworkSimplexArcRefV1"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowConvexNetworkSimplexNodeStateV1"
          },
          "type": "array"
        },
        "stage": {
          "$ref": "#/$defs/FlowConvexNetworkSimplexStageV1"
        }
      },
      "required": [
        "stage",
        "artificial_cost",
        "nodes",
        "edges",
        "artificial_edges",
        "cycle"
      ],
      "type": "object"
    },
    "FlowConvexNetworkSimplexStageV1": {
      "oneOf": [
        {
          "const": "initialize-basis",
          "type": "string"
        },
        {
          "const": "price",
          "type": "string"
        },
        {
          "const": "form-cycle",
          "type": "string"
        },
        {
          "const": "cross-breakpoint",
          "type": "string"
        },
        {
          "const": "exchange-basis",
          "type": "string"
        },
        {
          "const": "flip-bound",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowDetailStepCapabilityV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "availability": {
              "const": "available",
              "type": "string"
            },
            "unit": {
              "type": "string"
            }
          },
          "required": [
            "availability",
            "unit"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "availability": {
              "const": "unavailable",
              "type": "string"
            },
            "reason": {
              "type": "string"
            }
          },
          "required": [
            "availability",
            "reason"
          ],
          "type": "object"
        }
      ]
    },
    "FlowDeterministicAlmostLinearCycleKindV1": {
      "enum": [
        "tree",
        "spanner"
      ],
      "type": "string"
    },
    "FlowDeterministicAlmostLinearEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_core_edge": {
          "type": "boolean"
        },
        "active_cycle_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "active_spanner_edge": {
          "type": "boolean"
        },
        "active_tree_edge": {
          "type": "boolean"
        },
        "changed_coordinate": {
          "type": "boolean"
        },
        "edge_id": {
          "type": "string"
        },
        "embedding_hops": {
          "type": "string"
        },
        "embedding_stretch": {
          "type": "string"
        },
        "final_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "forest_level_mask": {
          "type": "string"
        },
        "gradient": {
          "type": "string"
        },
        "interior_flow": {
          "type": "string"
        },
        "length": {
          "type": "string"
        },
        "rounding_cycle_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "rounding_flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "rounding_forest_edge": {
          "type": "boolean"
        },
        "tree_level_mask": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "interior_flow",
        "gradient",
        "length",
        "tree_level_mask",
        "forest_level_mask",
        "active_tree_edge",
        "active_core_edge",
        "active_spanner_edge",
        "embedding_hops",
        "embedding_stretch",
        "active_cycle_sign",
        "changed_coordinate",
        "rounding_forest_edge",
        "rounding_cycle_sign"
      ],
      "type": "object"
    },
    "FlowDeterministicAlmostLinearNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_artificial_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "active_artificial_tree_edge": {
          "type": "boolean"
        },
        "artificial_capacity": {
          "type": "string"
        },
        "artificial_direction": {
          "type": "string"
        },
        "artificial_flow": {
          "type": "string"
        },
        "artificial_tree_level_mask": {
          "type": "string"
        },
        "forest_component": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "source_side": {
          "type": "boolean"
        },
        "tree_parent_node_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "node_id",
        "forest_component",
        "source_side",
        "artificial_direction",
        "artificial_flow",
        "artificial_capacity",
        "artificial_tree_level_mask",
        "active_artificial_tree_edge",
        "active_artificial_sign"
      ],
      "type": "object"
    },
    "FlowDeterministicAlmostLinearOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_branches": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "active_level": {
          "type": [
            "string",
            "null"
          ]
        },
        "active_return_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "active_return_tree_edge": {
          "type": "boolean"
        },
        "alpha": {
          "type": "string"
        },
        "artificial_edges": {
          "type": "string"
        },
        "artificial_flow": {
          "type": "string"
        },
        "branch_count": {
          "type": "string"
        },
        "built_branch_records": {
          "type": "string"
        },
        "core_edges": {
          "type": "string"
        },
        "core_vertices": {
          "type": "string"
        },
        "cost_gap": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowDeterministicAlmostLinearEdgeStateV1"
          },
          "type": "array"
        },
        "embedding_hops": {
          "type": "string"
        },
        "exact_pool_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_artificial_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_gap": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_point_mix": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_point_return_flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_point_threshold": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_return_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "forest_pool_size": {
          "type": "string"
        },
        "fundamental_cycles": {
          "type": "string"
        },
        "iteration": {
          "type": "string"
        },
        "level_count": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowDeterministicAlmostLinearNodeStateV1"
          },
          "type": "array"
        },
        "passes": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "potential": {
          "type": "string"
        },
        "rebuild_epoch": {
          "type": "string"
        },
        "return_capacity": {
          "type": "string"
        },
        "return_flow": {
          "type": "string"
        },
        "return_gradient": {
          "type": "string"
        },
        "return_length": {
          "type": "string"
        },
        "return_tree_level_mask": {
          "type": "string"
        },
        "rounding_processed_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "rounding_return_flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "rounding_return_forest_edge": {
          "type": "boolean"
        },
        "rounding_return_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "selected_cycle_kind": {
          "$ref": "#/$defs/FlowDeterministicAlmostLinearCycleKindV1"
        },
        "selected_off_tree_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "selected_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "spanner_edges": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowDeterministicAlmostLinearStageV1"
        },
        "target_value": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "alpha",
        "potential",
        "cost_gap",
        "forest_pool_size",
        "level_count",
        "branch_count",
        "built_branch_records",
        "active_branches",
        "passes",
        "fundamental_cycles",
        "core_vertices",
        "core_edges",
        "spanner_edges",
        "embedding_hops",
        "iteration",
        "rebuild_epoch",
        "return_flow",
        "return_capacity",
        "return_gradient",
        "return_length",
        "return_tree_level_mask",
        "active_return_tree_edge",
        "active_return_sign",
        "rounding_return_forest_edge",
        "rounding_return_sign",
        "artificial_edges",
        "artificial_flow",
        "final_point_threshold",
        "target_value",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowDeterministicAlmostLinearStageV1": {
      "enum": [
        "ready",
        "build-return-edge-reduction",
        "build-initial-point",
        "enumerate-forest-pool",
        "install-branch-record",
        "build-branch-collection",
        "build-core-graph",
        "build-spanner-embedding",
        "inspect-fundamental-cycle",
        "query-minimum-ratio-cycle",
        "query-failure",
        "shift-branch",
        "rebuild-deeper-levels",
        "potential-reduction-step",
        "detect-changed-coordinates",
        "scheduled-rebuild",
        "enumerate-feasible-set",
        "construct-final-point",
        "rounding-integral-edge",
        "rounding-link-fractional-edge",
        "rounding-cancel-fractional-cycle",
        "finish-flow-rounding",
        "check-certificate",
        "optimal"
      ],
      "type": "string"
    },
    "FlowDoubleScalingArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "branch": {
          "enum": [
            "flow",
            "slack"
          ],
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "branch",
        "direction"
      ],
      "type": "object"
    },
    "FlowDoubleScalingEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "flow_branch": {
          "type": "string"
        },
        "slack_branch": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "flow_branch",
        "slack_branch"
      ],
      "type": "object"
    },
    "FlowDoubleScalingNodeKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "edge",
          "type": "string"
        }
      ]
    },
    "FlowDoubleScalingNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "cursor": {
          "type": "string"
        },
        "entity_id": {
          "type": "string"
        },
        "imbalance": {
          "type": "string"
        },
        "kind": {
          "$ref": "#/$defs/FlowDoubleScalingNodeKindV1"
        },
        "price": {
          "type": "string"
        }
      },
      "required": [
        "entity_id",
        "kind",
        "price",
        "imbalance",
        "cursor"
      ],
      "type": "object"
    },
    "FlowDoubleScalingOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_path": {
          "items": {
            "$ref": "#/$defs/FlowDoubleScalingArcRefV1"
          },
          "type": "array"
        },
        "admissible_arcs": {
          "items": {
            "$ref": "#/$defs/FlowDoubleScalingArcRefV1"
          },
          "type": "array"
        },
        "capacity_phase": {
          "type": "string"
        },
        "cost_multiplier": {
          "type": "string"
        },
        "cost_phase": {
          "type": "string"
        },
        "delta": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowDoubleScalingEdgeStateV1"
          },
          "type": "array"
        },
        "epsilon": {
          "type": "string"
        },
        "inspected_arc": {
          "$ref": "#/$defs/FlowDoubleScalingArcRefV1"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowDoubleScalingNodeStateV1"
          },
          "type": "array"
        },
        "selected_deficit": {
          "type": [
            "string",
            "null"
          ]
        },
        "selected_root": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowDoubleScalingStageV1"
        }
      },
      "required": [
        "stage",
        "epsilon",
        "cost_multiplier",
        "delta",
        "cost_phase",
        "capacity_phase",
        "nodes",
        "edges",
        "admissible_arcs",
        "active_path"
      ],
      "type": "object"
    },
    "FlowDoubleScalingStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "initialize",
          "type": "string"
        },
        {
          "const": "start-cost-phase",
          "type": "string"
        },
        {
          "const": "start-capacity-phase",
          "type": "string"
        },
        {
          "const": "select-root",
          "type": "string"
        },
        {
          "const": "inspect-arc",
          "type": "string"
        },
        {
          "const": "advance",
          "type": "string"
        },
        {
          "const": "relabel",
          "type": "string"
        },
        {
          "const": "retreat",
          "type": "string"
        },
        {
          "const": "augment",
          "type": "string"
        },
        {
          "const": "complete-cost-phase",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowDualNetworkSimplexEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "basic_flow": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "in_tree": {
          "type": "boolean"
        },
        "reduced_cost": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "basic_flow",
        "reduced_cost",
        "in_tree"
      ],
      "type": "object"
    },
    "FlowDualNetworkSimplexNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "in_cut": {
          "type": "boolean"
        },
        "initialized": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "potential",
        "initialized",
        "in_cut"
      ],
      "type": "object"
    },
    "FlowDualNetworkSimplexOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "cut_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowDualNetworkSimplexEdgeStateV1"
          },
          "type": "array"
        },
        "entering_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "inspected_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "leaving_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowDualNetworkSimplexNodeStateV1"
          },
          "type": "array"
        },
        "pivot_price_delta": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowDualNetworkSimplexStageV1"
        }
      },
      "required": [
        "stage",
        "nodes",
        "edges",
        "cut_side"
      ],
      "type": "object"
    },
    "FlowDualNetworkSimplexStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "inspect-initial-arc",
          "type": "string"
        },
        {
          "const": "initialize-dual-tree",
          "type": "string"
        },
        {
          "const": "select-leaving",
          "type": "string"
        },
        {
          "const": "inspect-entering-arc",
          "type": "string"
        },
        {
          "const": "select-entering",
          "type": "string"
        },
        {
          "const": "pivot",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowDynamicEibfsOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "boundary_violations": {
          "type": "string"
        },
        "bridge_violations": {
          "type": "string"
        },
        "capacity_decreases": {
          "type": "string"
        },
        "capacity_increases": {
          "type": "string"
        },
        "certification_recoveries": {
          "type": "string"
        },
        "changed_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "current_arc_violations": {
          "type": "string"
        },
        "invalidated_parent_arcs": {
          "type": "string"
        },
        "label_violations": {
          "type": "string"
        },
        "new_capacity": {
          "type": [
            "string",
            "null"
          ]
        },
        "no_op_updates": {
          "type": "string"
        },
        "old_capacity": {
          "type": [
            "string",
            "null"
          ]
        },
        "over_capacity_repairs": {
          "type": "string"
        },
        "prefix_value": {
          "type": [
            "string",
            "null"
          ]
        },
        "promoted_roots": {
          "type": "string"
        },
        "repair_arc_scans": {
          "type": "string"
        },
        "repair_iterations": {
          "type": "string"
        },
        "reused_forest_nodes": {
          "type": "string"
        },
        "stage": {
          "enum": [
            "initial-solve",
            "apply-update",
            "repair-capacity",
            "repair-forest",
            "repair-violation",
            "continue-solve",
            "prefix-recovery",
            "prefix-certified",
            "resume-reusable-pseudoflow"
          ],
          "type": "string"
        },
        "state_transitions": {
          "type": "string"
        },
        "update_index": {
          "type": "string"
        },
        "update_total": {
          "type": "string"
        },
        "updates_applied": {
          "type": "string"
        },
        "violation": {
          "enum": [
            "over-capacity",
            "bridge",
            "label",
            "current-arc",
            "boundary"
          ],
          "type": "string"
        }
      },
      "required": [
        "stage",
        "update_index",
        "update_total",
        "reused_forest_nodes",
        "updates_applied",
        "capacity_increases",
        "capacity_decreases",
        "no_op_updates",
        "over_capacity_repairs",
        "invalidated_parent_arcs",
        "promoted_roots",
        "repair_arc_scans",
        "state_transitions",
        "bridge_violations",
        "label_violations",
        "current_arc_violations",
        "boundary_violations",
        "repair_iterations",
        "certification_recoveries"
      ],
      "type": "object"
    },
    "FlowEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "flow"
      ],
      "type": "object"
    },
    "FlowEdgeV1": {
      "additionalProperties": false,
      "properties": {
        "capacity": {
          "type": "string"
        },
        "convex_cost": {
          "$ref": "#/$defs/FlowConvexCostV1"
        },
        "cost": {
          "type": "string"
        },
        "from": {
          "type": "string"
        },
        "id": {
          "type": "string"
        },
        "initial_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "lower": {
          "type": "string"
        },
        "to": {
          "type": "string"
        }
      },
      "required": [
        "id",
        "from",
        "to",
        "capacity"
      ],
      "type": "object"
    },
    "FlowEibfsForestArcV1": {
      "additionalProperties": false,
      "properties": {
        "admissible_residual": {
          "$ref": "#/$defs/FlowResidualArcRefV1"
        },
        "child": {
          "type": "string"
        },
        "parent": {
          "type": "string"
        },
        "side": {
          "enum": [
            "source",
            "sink"
          ],
          "type": "string"
        }
      },
      "required": [
        "parent",
        "child",
        "side",
        "admissible_residual"
      ],
      "type": "object"
    },
    "FlowEibfsNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "imbalance": {
          "type": "string"
        },
        "membership": {
          "enum": [
            "free",
            "source",
            "sink"
          ],
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "orphan": {
          "type": "boolean"
        },
        "root_kind": {
          "enum": [
            "none",
            "source",
            "sink",
            "excess",
            "deficit"
          ],
          "type": "string"
        },
        "sink_label": {
          "type": "string"
        },
        "source_label": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "source_label",
        "sink_label",
        "membership",
        "root_kind",
        "orphan",
        "imbalance"
      ],
      "type": "object"
    },
    "FlowEibfsOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "forest_arcs": {
          "items": {
            "$ref": "#/$defs/FlowEibfsForestArcV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowEibfsNodeStateV1"
          },
          "type": "array"
        },
        "phase_direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "sink_depth": {
          "type": "string"
        },
        "source_depth": {
          "type": "string"
        }
      },
      "required": [
        "phase_direction",
        "source_depth",
        "sink_depth",
        "nodes",
        "forest_arcs"
      ],
      "type": "object"
    },
    "FlowElectricalEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "conductance": {
          "type": "string"
        },
        "congestion": {
          "type": "string"
        },
        "current": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "energy": {
          "type": "string"
        },
        "resistance": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "voltage_drop": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "resistance",
        "conductance",
        "voltage_drop",
        "current",
        "congestion",
        "energy"
      ],
      "type": "object"
    },
    "FlowElectricalFlowOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "converged": {
          "type": "boolean"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowElectricalEdgeStateV1"
          },
          "type": "array"
        },
        "effective_resistance": {
          "type": "string"
        },
        "exact_effective_resistance": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "iteration": {
          "type": "string"
        },
        "maximum_absolute_error": {
          "type": [
            "string",
            "null"
          ]
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowElectricalNodeStateV1"
          },
          "type": "array"
        },
        "relative_tolerance": {
          "type": "string"
        },
        "residual_l2": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowElectricalFlowStageV1"
        },
        "target_current": {
          "type": "string"
        },
        "total_energy": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "target_current",
        "relative_tolerance",
        "iteration",
        "residual_l2",
        "effective_resistance",
        "total_energy",
        "converged",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowElectricalFlowStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "assemble-laplacian",
          "type": "string"
        },
        {
          "const": "initialize-conjugate-gradient",
          "type": "string"
        },
        {
          "const": "conjugate-gradient-iteration",
          "type": "string"
        },
        {
          "const": "recover-currents",
          "type": "string"
        },
        {
          "const": "check-exact-reference",
          "type": "string"
        },
        {
          "const": "complete",
          "type": "string"
        }
      ]
    },
    "FlowElectricalIpmMcfEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "conductance": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "electrical_current": {
          "type": "string"
        },
        "face_lower": {
          "type": "string"
        },
        "face_upper": {
          "type": "string"
        },
        "final_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "fixed_on_face": {
          "type": "boolean"
        },
        "fractional_flow": {
          "type": "string"
        },
        "isolated_cost": {
          "type": "string"
        },
        "lower_slack": {
          "type": "string"
        },
        "lower_slack_direction": {
          "type": "string"
        },
        "perturbation": {
          "type": "string"
        },
        "resistance": {
          "type": "string"
        },
        "upper_complement": {
          "type": "string"
        },
        "upper_multiplier": {
          "type": "string"
        },
        "upper_multiplier_direction": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "perturbation",
        "isolated_cost",
        "fixed_on_face",
        "face_lower",
        "face_upper",
        "fractional_flow",
        "upper_complement",
        "lower_slack",
        "upper_multiplier",
        "resistance",
        "conductance",
        "electrical_current",
        "lower_slack_direction",
        "upper_multiplier_direction"
      ],
      "type": "object"
    },
    "FlowElectricalIpmMcfNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "anchored": {
          "type": "boolean"
        },
        "balance_residual": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        },
        "potential_direction": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "potential",
        "potential_direction",
        "balance_residual",
        "anchored"
      ],
      "type": "object"
    },
    "FlowElectricalIpmMcfOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "balance_residual": {
          "type": "string"
        },
        "barrier_objective": {
          "type": "string"
        },
        "centrality_residual": {
          "type": "string"
        },
        "duality_gap_bound": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowElectricalIpmMcfEdgeStateV1"
          },
          "type": "array"
        },
        "electrical_energy": {
          "type": "string"
        },
        "epsilon_3": {
          "type": "string"
        },
        "isolated_gap": {
          "type": "string"
        },
        "isolated_optimum_cost": {
          "type": "string"
        },
        "isolation_attempt": {
          "type": "string"
        },
        "isolation_scale": {
          "type": "string"
        },
        "linear_residual": {
          "type": "string"
        },
        "mu": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowElectricalIpmMcfNodeStateV1"
          },
          "type": "array"
        },
        "perturbation_bound": {
          "type": "string"
        },
        "recovery_epsilon": {
          "type": "string"
        },
        "seed": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowElectricalIpmMcfStageV1"
        },
        "step_size": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "seed",
        "mu",
        "epsilon_3",
        "recovery_epsilon",
        "duality_gap_bound",
        "centrality_residual",
        "balance_residual",
        "step_size",
        "electrical_energy",
        "linear_residual",
        "barrier_objective",
        "isolation_scale",
        "perturbation_bound",
        "isolation_attempt",
        "isolated_optimum_cost",
        "isolated_gap",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowElectricalIpmMcfStageV1": {
      "enum": [
        "ready",
        "normalize-lower-bounds",
        "isolation-attempt",
        "select-isolated-costs",
        "contract-fixed-face",
        "initialize-dual-interior",
        "assemble-electrical-laplacian",
        "solve-newton-direction",
        "damped-centering-step",
        "centered",
        "decrease-barrier",
        "approximate-flow",
        "round-nearest-integer",
        "check-certificate",
        "optimal"
      ],
      "type": "string"
    },
    "FlowElectricalNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "grounded": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        },
        "residual": {
          "type": "string"
        },
        "search_direction": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "potential",
        "residual",
        "search_direction",
        "grounded"
      ],
      "type": "object"
    },
    "FlowEnhancedCapacityScalingComponentV1": {
      "additionalProperties": false,
      "properties": {
        "component_id": {
          "type": "string"
        },
        "excess": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "members": {
          "items": {
            "type": "string"
          },
          "type": "array"
        }
      },
      "required": [
        "component_id",
        "members",
        "excess"
      ],
      "type": "object"
    },
    "FlowEnhancedCapacityScalingEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "internal": {
          "type": "boolean"
        },
        "reduced_cost": {
          "type": "string"
        },
        "strongly_feasible": {
          "type": "boolean"
        },
        "tight": {
          "type": "boolean"
        },
        "virtual_flow": {
          "$ref": "#/$defs/FlowRationalV1"
        }
      },
      "required": [
        "edge_id",
        "virtual_flow",
        "reduced_cost",
        "internal",
        "strongly_feasible",
        "tight"
      ],
      "type": "object"
    },
    "FlowEnhancedCapacityScalingNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "component_id": {
          "type": "string"
        },
        "distance": {
          "type": [
            "string",
            "null"
          ]
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "component_id",
        "potential"
      ],
      "type": "object"
    },
    "FlowEnhancedCapacityScalingOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "augmentation": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "components": {
          "items": {
            "$ref": "#/$defs/FlowEnhancedCapacityScalingComponentV1"
          },
          "type": "array"
        },
        "contraction_arc": {
          "type": [
            "string",
            "null"
          ]
        },
        "delta": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowEnhancedCapacityScalingEdgeStateV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowEnhancedCapacityScalingNodeStateV1"
          },
          "type": "array"
        },
        "path": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "phase": {
          "type": "string"
        },
        "sink_component": {
          "type": [
            "string",
            "null"
          ]
        },
        "source_component": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowEnhancedCapacityScalingStageV1"
        }
      },
      "required": [
        "stage",
        "delta",
        "phase",
        "components",
        "nodes",
        "edges",
        "path"
      ],
      "type": "object"
    },
    "FlowEnhancedCapacityScalingStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "initialize",
          "type": "string"
        },
        {
          "const": "complete-regeneration",
          "type": "string"
        },
        {
          "const": "begin-phase",
          "type": "string"
        },
        {
          "const": "contract",
          "type": "string"
        },
        {
          "const": "inspect-residual-arc",
          "type": "string"
        },
        {
          "const": "select-path",
          "type": "string"
        },
        {
          "const": "augment",
          "type": "string"
        },
        {
          "const": "complete-phase",
          "type": "string"
        },
        {
          "const": "halve-scale",
          "type": "string"
        },
        {
          "const": "recover-primal",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowFeasibilityArcKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "lower-bound-return",
          "type": "string"
        },
        {
          "const": "from-super-source",
          "type": "string"
        },
        {
          "const": "to-super-sink",
          "type": "string"
        }
      ]
    },
    "FlowFeasibilityArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "imbalance_node_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "kind": {
          "$ref": "#/$defs/FlowFeasibilityArcKindV1"
        },
        "original_edge_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "return_from": {
          "type": [
            "string",
            "null"
          ]
        },
        "return_to": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "kind"
      ],
      "type": "object"
    },
    "FlowFeasibilityArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "arc": {
          "$ref": "#/$defs/FlowFeasibilityArcRefV1"
        },
        "capacity": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "focused": {
          "type": "boolean"
        },
        "focused_direction": {
          "type": [
            "string",
            "null"
          ]
        },
        "forward_residual": {
          "type": "string"
        },
        "from": {
          "$ref": "#/$defs/FlowFeasibilityNodeRefV1"
        },
        "reverse_residual": {
          "type": "string"
        },
        "to": {
          "$ref": "#/$defs/FlowFeasibilityNodeRefV1"
        }
      },
      "required": [
        "arc",
        "from",
        "to",
        "capacity",
        "flow",
        "forward_residual",
        "reverse_residual",
        "focused"
      ],
      "type": "object"
    },
    "FlowFeasibilityDomainEdgeV1": {
      "additionalProperties": false,
      "properties": {
        "capacity": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "from_node_id": {
          "type": "string"
        },
        "lower": {
          "type": "string"
        },
        "public_route_edge_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "to_node_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "from_node_id",
        "to_node_id",
        "lower",
        "capacity"
      ],
      "type": "object"
    },
    "FlowFeasibilityDomainKindV1": {
      "oneOf": [
        {
          "const": "public-input",
          "type": "string"
        },
        {
          "const": "node-aligned-transformation",
          "type": "string"
        },
        {
          "const": "standalone-transformation",
          "type": "string"
        }
      ]
    },
    "FlowFeasibilityDomainNodeV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "public_node_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "node_id"
      ],
      "type": "object"
    },
    "FlowFeasibilityDomainV1": {
      "additionalProperties": false,
      "properties": {
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowFeasibilityDomainEdgeV1"
          },
          "type": "array"
        },
        "kind": {
          "$ref": "#/$defs/FlowFeasibilityDomainKindV1"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowFeasibilityDomainNodeV1"
          },
          "type": "array"
        },
        "request": {
          "$ref": "#/$defs/FlowFeasibilityRequestV1"
        }
      },
      "required": [
        "kind",
        "nodes",
        "edges",
        "request"
      ],
      "type": "object"
    },
    "FlowFeasibilityMetricsV1": {
      "additionalProperties": false,
      "properties": {
        "active_node_selections": {
          "type": "string"
        },
        "auxiliary_adjacency_inspections": {
          "type": "string"
        },
        "cut_adjacency_inspections": {
          "type": "string"
        },
        "discharges": {
          "type": "string"
        },
        "extracted_original_edges": {
          "type": "string"
        },
        "original_edge_inspections": {
          "type": "string"
        },
        "original_node_inspections": {
          "type": "string"
        },
        "pushes": {
          "type": "string"
        },
        "relabels": {
          "type": "string"
        }
      },
      "required": [
        "original_edge_inspections",
        "original_node_inspections",
        "auxiliary_adjacency_inspections",
        "pushes",
        "relabels",
        "active_node_selections",
        "discharges",
        "cut_adjacency_inspections",
        "extracted_original_edges"
      ],
      "type": "object"
    },
    "FlowFeasibilityNodeKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "super-source",
          "type": "string"
        },
        {
          "const": "super-sink",
          "type": "string"
        }
      ]
    },
    "FlowFeasibilityNodeRefV1": {
      "additionalProperties": false,
      "properties": {
        "kind": {
          "$ref": "#/$defs/FlowFeasibilityNodeKindV1"
        },
        "original_node_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "kind"
      ],
      "type": "object"
    },
    "FlowFeasibilityNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active": {
          "type": "boolean"
        },
        "current_arc": {
          "type": "string"
        },
        "excess": {
          "type": "string"
        },
        "height": {
          "type": "string"
        },
        "node": {
          "$ref": "#/$defs/FlowFeasibilityNodeRefV1"
        },
        "queue_position": {
          "type": [
            "string",
            "null"
          ]
        },
        "reachable": {
          "type": "boolean"
        }
      },
      "required": [
        "node",
        "height",
        "excess",
        "current_arc",
        "active",
        "reachable"
      ],
      "type": "object"
    },
    "FlowFeasibilityOverlayV2": {
      "additionalProperties": false,
      "properties": {
        "active_queue": {
          "items": {
            "$ref": "#/$defs/FlowFeasibilityNodeRefV1"
          },
          "type": "array"
        },
        "arcs": {
          "items": {
            "$ref": "#/$defs/FlowFeasibilityArcStateV1"
          },
          "type": "array"
        },
        "domain": {
          "$ref": "#/$defs/FlowFeasibilityDomainV1"
        },
        "focus_arc": {
          "$ref": "#/$defs/FlowFeasibilityResidualArcRefV1"
        },
        "focus_node": {
          "$ref": "#/$defs/FlowFeasibilityNodeRefV1"
        },
        "metrics": {
          "$ref": "#/$defs/FlowFeasibilityMetricsV1"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowFeasibilityNodeStateV1"
          },
          "type": "array"
        },
        "revision": {
          "const": "flow-feasibility-overlay/2",
          "type": "string"
        },
        "routed": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowFeasibilityStageV1"
        },
        "total_required": {
          "type": "string"
        },
        "use_kind": {
          "$ref": "#/$defs/FlowFeasibilityUseV1"
        }
      },
      "required": [
        "revision",
        "use_kind",
        "domain",
        "stage",
        "nodes",
        "arcs",
        "active_queue",
        "total_required",
        "routed",
        "metrics"
      ],
      "type": "object"
    },
    "FlowFeasibilityRequestV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "balance",
              "type": "string"
            },
            "required_divergences": {
              "items": {
                "$ref": "#/$defs/FlowFeasibilityRequiredDivergenceV1"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "required_divergences"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "max-flow-initial",
              "type": "string"
            },
            "sink_node_id": {
              "type": "string"
            },
            "source_node_id": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "source_node_id",
            "sink_node_id"
          ],
          "type": "object"
        }
      ]
    },
    "FlowFeasibilityRequiredDivergenceV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "required_divergence": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "required_divergence"
      ],
      "type": "object"
    },
    "FlowFeasibilityResidualArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "arc": {
          "$ref": "#/$defs/FlowFeasibilityArcRefV1"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        }
      },
      "required": [
        "arc",
        "direction"
      ],
      "type": "object"
    },
    "FlowFeasibilityStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "add-original-arc",
          "type": "string"
        },
        {
          "const": "add-return-arc",
          "type": "string"
        },
        {
          "const": "inspect-node-imbalance",
          "type": "string"
        },
        {
          "const": "add-imbalance-arc",
          "type": "string"
        },
        {
          "const": "initialize-source-height",
          "type": "string"
        },
        {
          "const": "inspect-source-arc",
          "type": "string"
        },
        {
          "const": "activate-node",
          "type": "string"
        },
        {
          "const": "select-active-node",
          "type": "string"
        },
        {
          "const": "inspect-discharge-arc",
          "type": "string"
        },
        {
          "const": "inspect-relabel-arc",
          "type": "string"
        },
        {
          "const": "push",
          "type": "string"
        },
        {
          "const": "advance-current-arc",
          "type": "string"
        },
        {
          "const": "relabel",
          "type": "string"
        },
        {
          "const": "complete-discharge",
          "type": "string"
        },
        {
          "const": "complete-routing",
          "type": "string"
        },
        {
          "const": "inspect-cut-arc",
          "type": "string"
        },
        {
          "const": "mark-reachable",
          "type": "string"
        },
        {
          "const": "extract-original-flow",
          "type": "string"
        },
        {
          "const": "feasible",
          "type": "string"
        },
        {
          "const": "infeasible",
          "type": "string"
        }
      ]
    },
    "FlowFeasibilityUseV1": {
      "oneOf": [
        {
          "const": "initial-flow",
          "type": "string"
        },
        {
          "const": "precheck-only",
          "type": "string"
        },
        {
          "const": "anchored-recovery",
          "type": "string"
        }
      ]
    },
    "FlowFeasibilityWorkSummaryV1": {
      "additionalProperties": false,
      "properties": {
        "invocations": {
          "type": "string"
        },
        "metrics": {
          "$ref": "#/$defs/FlowFeasibilityMetricsV1"
        }
      },
      "required": [
        "invocations",
        "metrics"
      ],
      "type": "object"
    },
    "FlowFrameworkMcfDynamicOperationV1": {
      "oneOf": [
        {
          "const": "topology-stage-applied",
          "type": "string"
        },
        {
          "const": "periodic-rebuilt",
          "type": "string"
        },
        {
          "const": "cycle-queried-accepted",
          "type": "string"
        },
        {
          "const": "cycle-queried-rejected",
          "type": "string"
        },
        {
          "const": "level-shifted",
          "type": "string"
        },
        {
          "const": "flow-applied",
          "type": "string"
        },
        {
          "const": "query-returned",
          "type": "string"
        },
        {
          "const": "detect-returned",
          "type": "string"
        },
        {
          "const": "completed",
          "type": "string"
        }
      ]
    },
    "FlowFrameworkMcfEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "cycle_coefficient": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "selected": {
          "type": "boolean"
        }
      },
      "required": [
        "edge_id",
        "flow",
        "cycle_coefficient",
        "selected"
      ],
      "type": "object"
    },
    "FlowFrameworkMcfFinalPointEdgeV1": {
      "additionalProperties": false,
      "properties": {
        "auxiliary": {
          "type": "boolean"
        },
        "capacity": {
          "type": "string"
        },
        "cost": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "from": {
          "type": "string"
        },
        "lower": {
          "type": "string"
        },
        "rounded_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "to": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "from",
        "to",
        "lower",
        "capacity",
        "cost",
        "flow",
        "auxiliary"
      ],
      "type": "object"
    },
    "FlowFrameworkMcfFinalPointNodeV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "required_divergence": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "required_divergence"
      ],
      "type": "object"
    },
    "FlowFrameworkMcfLevelStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_branch": {
          "type": "string"
        },
        "level": {
          "type": "string"
        },
        "passes": {
          "type": "string"
        }
      },
      "required": [
        "level",
        "active_branch",
        "passes"
      ],
      "type": "object"
    },
    "FlowFrameworkMcfOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "accepted_ratio": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "dynamic_operation": {
          "$ref": "#/$defs/FlowFrameworkMcfDynamicOperationV1"
        },
        "dynamic_operation_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowFrameworkMcfEdgeStateV1"
          },
          "type": "array"
        },
        "exact_gap_after": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "exact_gap_before": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_point_edges": {
          "items": {
            "$ref": "#/$defs/FlowFrameworkMcfFinalPointEdgeV1"
          },
          "type": [
            "array",
            "null"
          ]
        },
        "final_point_nodes": {
          "items": {
            "$ref": "#/$defs/FlowFrameworkMcfFinalPointNodeV1"
          },
          "type": [
            "array",
            "null"
          ]
        },
        "gap_after": {
          "type": "string"
        },
        "gap_before": {
          "type": "string"
        },
        "iteration": {
          "type": "string"
        },
        "levels": {
          "items": {
            "$ref": "#/$defs/FlowFrameworkMcfLevelStateV1"
          },
          "type": "array"
        },
        "optimum_cost": {
          "type": [
            "string",
            "null"
          ]
        },
        "potential_after": {
          "type": "string"
        },
        "potential_before": {
          "type": "string"
        },
        "reinitialized": {
          "type": "boolean"
        },
        "stage": {
          "$ref": "#/$defs/FlowFrameworkMcfStageV1"
        },
        "stopping_gap": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "target_progress": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "termination": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "stage",
        "iteration",
        "reinitialized",
        "potential_before",
        "potential_after",
        "gap_before",
        "gap_after",
        "exact_gap_before",
        "exact_gap_after",
        "stopping_gap",
        "accepted_ratio",
        "target_progress",
        "levels",
        "edges"
      ],
      "type": "object"
    },
    "FlowFrameworkMcfStageV1": {
      "oneOf": [
        {
          "const": "initialize-source-point",
          "type": "string"
        },
        {
          "const": "periodic-reinitialize",
          "type": "string"
        },
        {
          "const": "detect",
          "type": "string"
        },
        {
          "const": "query-minimum-ratio-cycle",
          "type": "string"
        },
        {
          "const": "source-progress",
          "type": "string"
        },
        {
          "const": "round-fractional-flow",
          "type": "string"
        },
        {
          "const": "check-certificate",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowGraphV1": {
      "additionalProperties": false,
      "properties": {
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowEdgeV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowNodeV1"
          },
          "type": "array"
        }
      },
      "required": [
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowInteriorPointEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "congestion": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "electrical_current": {
          "type": "string"
        },
        "final_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "fractional_flow": {
          "type": "string"
        },
        "measure": {
          "type": "string"
        },
        "normalized_away": {
          "type": "boolean"
        },
        "resistance": {
          "type": "string"
        },
        "slack": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "fractional_flow",
        "electrical_current",
        "slack",
        "measure",
        "resistance",
        "congestion",
        "normalized_away"
      ],
      "type": "object"
    },
    "FlowInteriorPointMaxFlowOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_working_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "b_matching_edges": {
          "type": "string"
        },
        "b_matching_nodes": {
          "type": "string"
        },
        "centrality": {
          "type": "string"
        },
        "congestion_l4": {
          "type": "string"
        },
        "duality_gap": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowInteriorPointEdgeStateV1"
          },
          "type": "array"
        },
        "electrical_energy": {
          "type": "string"
        },
        "mu": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowInteriorPointNodeStateV1"
          },
          "type": "array"
        },
        "stage": {
          "$ref": "#/$defs/FlowInteriorPointMaxFlowStageV1"
        },
        "step_size": {
          "type": "string"
        },
        "target_value": {
          "type": "string"
        },
        "working_edges": {
          "type": "string"
        },
        "working_nodes": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "target_value",
        "mu",
        "duality_gap",
        "centrality",
        "congestion_l4",
        "step_size",
        "electrical_energy",
        "b_matching_nodes",
        "b_matching_edges",
        "working_nodes",
        "working_edges",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowInteriorPointMaxFlowStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "enumerate-target-cut",
          "type": "string"
        },
        {
          "const": "build-b-matching-reduction",
          "type": "string"
        },
        {
          "const": "build-min-cost-reduction",
          "type": "string"
        },
        {
          "const": "initialize-central-path",
          "type": "string"
        },
        {
          "const": "solve-electrical-direction",
          "type": "string"
        },
        {
          "const": "descent-step",
          "type": "string"
        },
        {
          "const": "solve-centering-direction",
          "type": "string"
        },
        {
          "const": "centering-step",
          "type": "string"
        },
        {
          "const": "extract-fractional-flow",
          "type": "string"
        },
        {
          "const": "round-integral-flow",
          "type": "string"
        },
        {
          "const": "check-certificate",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowInteriorPointNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        },
        "target_source_side": {
          "type": "boolean"
        }
      },
      "required": [
        "node_id",
        "potential",
        "target_source_side"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleArcV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "sign": {
          "enum": [
            "-1",
            "1"
          ],
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "sign"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "candidate_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "denominator_contribution": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "gradient": {
          "type": "string"
        },
        "length": {
          "type": "string"
        },
        "numerator_contribution": {
          "type": "string"
        },
        "selected_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "tree_edge": {
          "type": "boolean"
        }
      },
      "required": [
        "edge_id",
        "gradient",
        "length",
        "tree_edge",
        "candidate_sign",
        "selected_sign",
        "numerator_contribution",
        "denominator_contribution"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleMcfEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "candidate_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "denominator_contribution": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "fixed_on_face": {
          "type": "boolean"
        },
        "gradient": {
          "type": "string"
        },
        "initial_flow": {
          "type": "string"
        },
        "length": {
          "type": "string"
        },
        "lower_slack": {
          "type": "string"
        },
        "numerator_contribution": {
          "type": "string"
        },
        "selected_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "tree_edge": {
          "type": "boolean"
        },
        "updated_flow": {
          "type": "string"
        },
        "upper_slack": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "fixed_on_face",
        "initial_flow",
        "updated_flow",
        "lower_slack",
        "upper_slack",
        "gradient",
        "length",
        "tree_edge",
        "candidate_sign",
        "selected_sign",
        "numerator_contribution",
        "denominator_contribution"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleMcfNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "candidate_balance": {
          "type": "string"
        },
        "component": {
          "type": "string"
        },
        "depth": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "on_candidate": {
          "type": "boolean"
        },
        "on_selected": {
          "type": "boolean"
        },
        "parent_node_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "node_id",
        "component",
        "depth",
        "candidate_balance",
        "on_candidate",
        "on_selected"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleMcfOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "alpha": {
          "type": "string"
        },
        "best_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "candidate_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "cost_gap": {
          "type": "string"
        },
        "current_cost": {
          "type": "string"
        },
        "current_potential": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowMinimumRatioCycleMcfEdgeStateV1"
          },
          "type": "array"
        },
        "enumerated_vectors": {
          "type": "string"
        },
        "eta": {
          "type": "string"
        },
        "feasible_flows": {
          "type": "string"
        },
        "fundamental_cycles": {
          "type": "string"
        },
        "guaranteed_decrease": {
          "type": "string"
        },
        "initial_cost": {
          "type": "string"
        },
        "kappa": {
          "type": "string"
        },
        "maximum_absolute_balance": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowMinimumRatioCycleMcfNodeStateV1"
          },
          "type": "array"
        },
        "optimum_cost": {
          "type": "string"
        },
        "potential_before": {
          "type": "string"
        },
        "potential_decrease": {
          "type": "string"
        },
        "selected_edge_count": {
          "type": "string"
        },
        "simple_cycles": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowMinimumRatioCycleMcfStageV1"
        },
        "stationary": {
          "type": "boolean"
        },
        "weighted_step_norm": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "alpha",
        "optimum_cost",
        "initial_cost",
        "current_cost",
        "cost_gap",
        "potential_before",
        "current_potential",
        "kappa",
        "eta",
        "weighted_step_norm",
        "potential_decrease",
        "guaranteed_decrease",
        "stationary",
        "selected_edge_count",
        "maximum_absolute_balance",
        "feasible_flows",
        "enumerated_vectors",
        "simple_cycles",
        "fundamental_cycles",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleMcfStageV1": {
      "enum": [
        "ready",
        "enumerate-feasible-set",
        "contract-fixed-face",
        "initialize-strict-interior",
        "evaluate-potential",
        "map-gradient-length",
        "build-spanning-forest",
        "inspect-vector",
        "evaluate-cycle",
        "update-best",
        "verify-cycle-space",
        "apply-source-step",
        "measure-potential-decrease",
        "check-dfs-oracle",
        "complete"
      ],
      "type": "string"
    },
    "FlowMinimumRatioCycleNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "candidate_balance": {
          "type": "string"
        },
        "component": {
          "type": "string"
        },
        "depth": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "on_candidate": {
          "type": "boolean"
        },
        "on_selected": {
          "type": "boolean"
        },
        "parent_node_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "node_id",
        "component",
        "depth",
        "candidate_balance",
        "on_candidate",
        "on_selected"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "best_ratio": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "candidate_ratio": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowMinimumRatioCycleEdgeStateV1"
          },
          "type": "array"
        },
        "enumerated_vectors": {
          "type": "string"
        },
        "fundamental_cycles": {
          "type": "string"
        },
        "maximum_absolute_balance": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowMinimumRatioCycleNodeStateV1"
          },
          "type": "array"
        },
        "selected_edge_count": {
          "type": "string"
        },
        "simple_cycles": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowMinimumRatioCycleStageV1"
        }
      },
      "required": [
        "stage",
        "selected_edge_count",
        "maximum_absolute_balance",
        "enumerated_vectors",
        "simple_cycles",
        "fundamental_cycles",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowMinimumRatioCycleStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "map-gradient-length",
          "type": "string"
        },
        {
          "const": "build-spanning-forest",
          "type": "string"
        },
        {
          "const": "inspect-vector",
          "type": "string"
        },
        {
          "const": "evaluate-cycle",
          "type": "string"
        },
        {
          "const": "update-best",
          "type": "string"
        },
        {
          "const": "verify-cycle-space",
          "type": "string"
        },
        {
          "const": "check-exhaustive-oracle",
          "type": "string"
        },
        {
          "const": "complete",
          "type": "string"
        }
      ]
    },
    "FlowNodePotentialV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "potential"
      ],
      "type": "object"
    },
    "FlowNodeTraceStateV1": {
      "additionalProperties": false,
      "properties": {
        "label": {
          "type": [
            "string",
            "null"
          ]
        },
        "node_id": {
          "type": "string"
        },
        "remaining_divergence": {
          "type": [
            "string",
            "null"
          ]
        },
        "search_ordinal": {
          "minimum": 0,
          "type": [
            "integer",
            "null"
          ]
        }
      },
      "required": [
        "node_id"
      ],
      "type": "object"
    },
    "FlowNodeV1": {
      "additionalProperties": false,
      "properties": {
        "id": {
          "type": "string"
        },
        "position": {
          "$ref": "#/$defs/FlowPositionV1"
        },
        "supply": {
          "type": "string"
        }
      },
      "required": [
        "id"
      ],
      "type": "object"
    },
    "FlowOrlinMaxFlowCompactArcKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "abundant-pseudo",
          "type": "string"
        },
        {
          "const": "transferred-pseudo",
          "type": "string"
        }
      ]
    },
    "FlowOrlinMaxFlowCompactArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "ordinal": {
          "type": "string"
        },
        "reverse": {
          "type": "boolean"
        }
      },
      "required": [
        "ordinal",
        "reverse"
      ],
      "type": "object"
    },
    "FlowOrlinMaxFlowCompactArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "capacity": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "from_component": {
          "type": "string"
        },
        "inspection_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "kind": {
          "$ref": "#/$defs/FlowOrlinMaxFlowCompactArcKindV1"
        },
        "ordinal": {
          "type": "string"
        },
        "to_component": {
          "type": "string"
        },
        "witness": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        }
      },
      "required": [
        "ordinal",
        "from_component",
        "to_component",
        "kind",
        "capacity",
        "flow",
        "witness"
      ],
      "type": "object"
    },
    "FlowOrlinMaxFlowNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "anti_potential": {
          "type": "string"
        },
        "component_id": {
          "type": "string"
        },
        "critical": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "source_side": {
          "type": "boolean"
        }
      },
      "required": [
        "node_id",
        "component_id",
        "critical",
        "anti_potential",
        "source_side"
      ],
      "type": "object"
    },
    "FlowOrlinMaxFlowOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_compact_path": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMaxFlowCompactArcRefV1"
          },
          "type": "array"
        },
        "active_original_path": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "compact_arcs": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMaxFlowCompactArcStateV1"
          },
          "type": "array"
        },
        "delta": {
          "type": "string"
        },
        "gamma": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMaxFlowNodeStateV1"
          },
          "type": "array"
        },
        "phase_case": {
          "$ref": "#/$defs/FlowOrlinMaxFlowPhaseCaseV1"
        },
        "residual_arcs": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMaxFlowResidualArcStateV1"
          },
          "type": "array"
        },
        "stage": {
          "$ref": "#/$defs/FlowOrlinMaxFlowStageV1"
        },
        "threshold": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "delta",
        "gamma",
        "nodes",
        "residual_arcs",
        "compact_arcs",
        "active_compact_path",
        "active_original_path",
        "threshold"
      ],
      "type": "object"
    },
    "FlowOrlinMaxFlowPhaseCaseV1": {
      "oneOf": [
        {
          "const": "original-approximation",
          "type": "string"
        },
        {
          "const": "compact-approximation",
          "type": "string"
        },
        {
          "const": "compact-exact",
          "type": "string"
        }
      ]
    },
    "FlowOrlinMaxFlowResidualArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "abundant": {
          "type": "boolean"
        },
        "anti_abundant": {
          "type": "boolean"
        },
        "capacity": {
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "inspection_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "medium": {
          "type": "boolean"
        },
        "small": {
          "type": "boolean"
        }
      },
      "required": [
        "edge_id",
        "direction",
        "capacity",
        "abundant",
        "anti_abundant",
        "small",
        "medium"
      ],
      "type": "object"
    },
    "FlowOrlinMaxFlowStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "begin-improvement",
          "type": "string"
        },
        {
          "const": "contract-abundant",
          "type": "string"
        },
        {
          "const": "inspect-classification-arc",
          "type": "string"
        },
        {
          "const": "classify",
          "type": "string"
        },
        {
          "const": "select-case",
          "type": "string"
        },
        {
          "const": "inspect-compact-construction-arc",
          "type": "string"
        },
        {
          "const": "transfer-capacity",
          "type": "string"
        },
        {
          "const": "build-subproblem",
          "type": "string"
        },
        {
          "const": "augment-subproblem",
          "type": "string"
        },
        {
          "const": "inspect-subproblem-arc",
          "type": "string"
        },
        {
          "const": "complete-subproblem",
          "type": "string"
        },
        {
          "const": "inspect-decomposition-arc",
          "type": "string"
        },
        {
          "const": "inspect-lift-residual-arc",
          "type": "string"
        },
        {
          "const": "lift-path",
          "type": "string"
        },
        {
          "const": "expand-contraction",
          "type": "string"
        },
        {
          "const": "inspect-expansion-residual-arc",
          "type": "string"
        },
        {
          "const": "inspect-cut-residual-arc",
          "type": "string"
        },
        {
          "const": "update-cut",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowOrlinMcfArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "branch": {
          "$ref": "#/$defs/FlowOrlinMcfBranchV1"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "branch",
        "direction"
      ],
      "type": "object"
    },
    "FlowOrlinMcfArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "branch": {
          "$ref": "#/$defs/FlowOrlinMcfBranchV1"
        },
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "internal": {
          "type": "boolean"
        },
        "reduced_cost": {
          "type": "string"
        },
        "strongly_feasible": {
          "type": "boolean"
        },
        "tight": {
          "type": "boolean"
        }
      },
      "required": [
        "edge_id",
        "branch",
        "flow",
        "reduced_cost",
        "internal",
        "strongly_feasible",
        "tight"
      ],
      "type": "object"
    },
    "FlowOrlinMcfBranchV1": {
      "oneOf": [
        {
          "const": "flow",
          "type": "string"
        },
        {
          "const": "slack",
          "type": "string"
        }
      ]
    },
    "FlowOrlinMcfComponentV1": {
      "additionalProperties": false,
      "properties": {
        "component_id": {
          "type": "string"
        },
        "excess": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "members": {
          "items": {
            "type": "string"
          },
          "type": "array"
        }
      },
      "required": [
        "component_id",
        "members",
        "excess"
      ],
      "type": "object"
    },
    "FlowOrlinMcfNodeKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "capacity",
          "type": "string"
        }
      ]
    },
    "FlowOrlinMcfNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "capacity_edge_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "component_id": {
          "type": "string"
        },
        "distance": {
          "type": [
            "string",
            "null"
          ]
        },
        "kind": {
          "$ref": "#/$defs/FlowOrlinMcfNodeKindV1"
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "kind",
        "component_id",
        "potential"
      ],
      "type": "object"
    },
    "FlowOrlinMcfOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "arcs": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMcfArcStateV1"
          },
          "type": "array"
        },
        "augmentation": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "components": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMcfComponentV1"
          },
          "type": "array"
        },
        "contraction_arc": {
          "$ref": "#/$defs/FlowOrlinMcfArcRefV1"
        },
        "delta": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "eliminated_capacity_nodes": {
          "type": "string"
        },
        "inspected_segment": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMcfArcRefV1"
          },
          "type": "array"
        },
        "inspection_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMcfNodeStateV1"
          },
          "type": "array"
        },
        "path": {
          "items": {
            "$ref": "#/$defs/FlowOrlinMcfArcRefV1"
          },
          "type": "array"
        },
        "phase": {
          "type": "string"
        },
        "shortcut_arcs": {
          "type": "string"
        },
        "sink_component": {
          "type": [
            "string",
            "null"
          ]
        },
        "source_component": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowOrlinMcfStageV1"
        }
      },
      "required": [
        "stage",
        "delta",
        "phase",
        "components",
        "nodes",
        "arcs",
        "path",
        "inspected_segment",
        "eliminated_capacity_nodes",
        "shortcut_arcs"
      ],
      "type": "object"
    },
    "FlowOrlinMcfStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "transform-capacities",
          "type": "string"
        },
        {
          "const": "initialize-dual",
          "type": "string"
        },
        {
          "const": "complete-regeneration",
          "type": "string"
        },
        {
          "const": "begin-phase",
          "type": "string"
        },
        {
          "const": "inspect-contractible-arc",
          "type": "string"
        },
        {
          "const": "inspect-reachability-arc",
          "type": "string"
        },
        {
          "const": "inspect-compressed-residual-arc",
          "type": "string"
        },
        {
          "const": "inspect-compressed-arc",
          "type": "string"
        },
        {
          "const": "contract",
          "type": "string"
        },
        {
          "const": "select-compressed-path",
          "type": "string"
        },
        {
          "const": "augment",
          "type": "string"
        },
        {
          "const": "complete-phase",
          "type": "string"
        },
        {
          "const": "halve-scale",
          "type": "string"
        },
        {
          "const": "expand-dual",
          "type": "string"
        },
        {
          "const": "recover-primal",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowOutcomeV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "cut_bound": {
              "type": "string"
            },
            "kind": {
              "const": "max-flow",
              "type": "string"
            },
            "source_side": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "value": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "value",
            "cut_bound",
            "source_side"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "augmentation_operations": {
              "type": "string"
            },
            "component_count": {
              "type": "string"
            },
            "delivered": {
              "type": "string"
            },
            "delta": {
              "type": "string"
            },
            "kind": {
              "const": "binary-blocking-flow",
              "type": "string"
            },
            "nontrivial_component_count": {
              "type": "string"
            },
            "termination": {
              "$ref": "#/$defs/FlowBinaryBlockingTerminationV1"
            },
            "upper_bound": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "upper_bound",
            "delta",
            "delivered",
            "termination",
            "component_count",
            "nontrivial_component_count",
            "augmentation_operations"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "determinant_bound": {
              "type": "string"
            },
            "epsilon": {
              "type": "string"
            },
            "fixed_variables": {
              "items": {
                "$ref": "#/$defs/FlowTardosFixedVariableV1"
              },
              "type": "array"
            },
            "kind": {
              "const": "tardos-framework",
              "type": "string"
            },
            "threshold": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "epsilon",
            "threshold",
            "determinant_bound",
            "fixed_variables"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "effective_resistance": {
              "type": "string"
            },
            "exact_effective_resistance": {
              "$ref": "#/$defs/FlowRationalV1"
            },
            "iterations": {
              "type": "string"
            },
            "kind": {
              "const": "electrical-flow",
              "type": "string"
            },
            "maximum_absolute_error": {
              "type": "string"
            },
            "residual_l2": {
              "type": "string"
            },
            "total_energy": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "effective_resistance",
            "exact_effective_resistance",
            "total_energy",
            "residual_l2",
            "maximum_absolute_error",
            "iterations"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "cycle": {
              "items": {
                "$ref": "#/$defs/FlowMinimumRatioCycleArcV1"
              },
              "type": "array"
            },
            "enumerated_vectors": {
              "type": "string"
            },
            "kind": {
              "const": "minimum-ratio-cycle",
              "type": "string"
            },
            "ratio": {
              "$ref": "#/$defs/FlowRationalV1"
            },
            "simple_cycles": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "cycle",
            "simple_cycles",
            "enumerated_vectors"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "alpha": {
              "type": "string"
            },
            "cycle": {
              "items": {
                "$ref": "#/$defs/FlowMinimumRatioCycleArcV1"
              },
              "type": "array"
            },
            "eta": {
              "type": "string"
            },
            "guaranteed_decrease": {
              "type": "string"
            },
            "kappa": {
              "type": "string"
            },
            "kind": {
              "const": "minimum-ratio-cycle-mcf",
              "type": "string"
            },
            "potential_decrease": {
              "type": "string"
            },
            "ratio": {
              "type": [
                "string",
                "null"
              ]
            },
            "stationary": {
              "type": "boolean"
            }
          },
          "required": [
            "kind",
            "cycle",
            "alpha",
            "kappa",
            "eta",
            "potential_decrease",
            "guaranteed_decrease",
            "stationary"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "breakpoints": {
              "items": {
                "$ref": "#/$defs/FlowParametricBreakpointV1"
              },
              "type": "array"
            },
            "kind": {
              "const": "parametric-max-flow",
              "type": "string"
            },
            "metrics": {
              "$ref": "#/$defs/FlowParametricMetricsV1"
            },
            "segments": {
              "items": {
                "$ref": "#/$defs/FlowParametricSegmentV1"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "segments",
            "breakpoints",
            "metrics"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "min-cost-flow",
              "type": "string"
            },
            "potentials": {
              "items": {
                "$ref": "#/$defs/FlowNodePotentialV1"
              },
              "type": "array"
            },
            "total_cost": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "total_cost",
            "potentials"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "cut_bound": {
              "type": "string"
            },
            "kind": {
              "const": "min-cost-max-flow",
              "type": "string"
            },
            "potentials": {
              "items": {
                "$ref": "#/$defs/FlowNodePotentialV1"
              },
              "type": "array"
            },
            "source_side": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "total_cost": {
              "type": "string"
            },
            "value": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "value",
            "cut_bound",
            "source_side",
            "total_cost",
            "potentials"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "cardinality": {
              "type": "string"
            },
            "cover_left": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "cover_right": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "kind": {
              "const": "bipartite-matching",
              "type": "string"
            },
            "pairs": {
              "items": {
                "$ref": "#/$defs/FlowBipartiteMatchingPairV1"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "cardinality",
            "pairs",
            "cover_left",
            "cover_right"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "agent_labels": {
              "items": {
                "$ref": "#/$defs/FlowAssignmentLabelV1"
              },
              "type": "array"
            },
            "kind": {
              "const": "assignment",
              "type": "string"
            },
            "objective": {
              "$ref": "#/$defs/AssignmentObjectiveV1"
            },
            "pairs": {
              "items": {
                "$ref": "#/$defs/FlowAssignmentPairV1"
              },
              "type": "array"
            },
            "task_labels": {
              "items": {
                "$ref": "#/$defs/FlowAssignmentLabelV1"
              },
              "type": "array"
            },
            "total_cost": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "objective",
            "total_cost",
            "pairs",
            "agent_labels",
            "task_labels"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "deficiency": {
              "type": "string"
            },
            "hall_agents": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "kind": {
              "const": "assignment-infeasible",
              "type": "string"
            },
            "neighbor_tasks": {
              "items": {
                "type": "string"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "deficiency",
            "hall_agents",
            "neighbor_tasks"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "infeasible",
              "type": "string"
            },
            "reachable_original_nodes": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "unsatisfied": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "unsatisfied",
            "reachable_original_nodes"
          ],
          "type": "object"
        }
      ]
    },
    "FlowParametricBreakpointV1": {
      "additionalProperties": false,
      "properties": {
        "after_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "before_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "entering_nodes": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "exact_maximal_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "exact_minimal_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "parameter": {
          "$ref": "#/$defs/FlowRationalV1"
        }
      },
      "required": [
        "parameter",
        "before_source_side",
        "after_source_side",
        "exact_minimal_source_side",
        "exact_maximal_source_side",
        "entering_nodes"
      ],
      "type": "object"
    },
    "FlowParametricCapacitySlopeV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "slope": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "slope"
      ],
      "type": "object"
    },
    "FlowParametricEdgeCapacityV1": {
      "additionalProperties": false,
      "properties": {
        "capacity": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "edge_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "capacity"
      ],
      "type": "object"
    },
    "FlowParametricMetricsV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "contraction_views": {
              "type": "string"
            },
            "cooperative_race_steps": {
              "type": "string"
            },
            "forest_initializations": {
              "type": "string"
            },
            "forest_reuses": {
              "type": "string"
            },
            "forward_race_wins": {
              "type": "string"
            },
            "free_run_races": {
              "type": "string"
            },
            "implementation": {
              "const": "parametric-pseudoflow",
              "type": "string"
            },
            "larger_child_continuations": {
              "type": "string"
            },
            "maximum_depth": {
              "type": "string"
            },
            "mergers": {
              "type": "string"
            },
            "parameter_advances": {
              "type": "string"
            },
            "relabels": {
              "type": "string"
            },
            "renormalization_pushes": {
              "type": "string"
            },
            "renormalization_splits": {
              "type": "string"
            },
            "residual_arc_scans": {
              "type": "string"
            },
            "reverse_race_wins": {
              "type": "string"
            },
            "smaller_child_restarts": {
              "type": "string"
            }
          },
          "required": [
            "implementation",
            "forest_initializations",
            "parameter_advances",
            "forest_reuses",
            "renormalization_pushes",
            "renormalization_splits",
            "mergers",
            "relabels",
            "free_run_races",
            "forward_race_wins",
            "reverse_race_wins",
            "cooperative_race_steps",
            "contraction_views",
            "smaller_child_restarts",
            "larger_child_continuations",
            "maximum_depth",
            "residual_arc_scans"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "breakpoints": {
              "type": "string"
            },
            "implementation": {
              "const": "breakpoint-rerun",
              "type": "string"
            },
            "intersections": {
              "type": "string"
            },
            "maximum_depth": {
              "type": "string"
            },
            "oracle_runs": {
              "type": "string"
            },
            "pseudoflow_runs": {
              "type": "string"
            },
            "segments": {
              "type": "string"
            },
            "simultaneous_breakpoints": {
              "type": "string"
            },
            "static_residual_arc_scans": {
              "type": "string"
            },
            "subproblems": {
              "type": "string"
            }
          },
          "required": [
            "implementation",
            "pseudoflow_runs",
            "oracle_runs",
            "static_residual_arc_scans",
            "intersections",
            "subproblems",
            "segments",
            "breakpoints",
            "simultaneous_breakpoints",
            "maximum_depth"
          ],
          "type": "object"
        }
      ]
    },
    "FlowParametricOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "edge_capacities": {
          "items": {
            "$ref": "#/$defs/FlowParametricEdgeCapacityV1"
          },
          "type": "array"
        },
        "parameter": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "recorded_breakpoints": {
          "items": {
            "$ref": "#/$defs/FlowParametricBreakpointV1"
          },
          "type": "array"
        },
        "recorded_segments": {
          "items": {
            "$ref": "#/$defs/FlowParametricSegmentV1"
          },
          "type": "array"
        },
        "stage": {
          "type": "string"
        },
        "traversal": {
          "$ref": "#/$defs/FlowParametricTraversalV1"
        },
        "visual_scale_max_capacity": {
          "$ref": "#/$defs/FlowRationalV1"
        }
      },
      "required": [
        "stage",
        "parameter",
        "edge_capacities",
        "visual_scale_max_capacity",
        "recorded_segments",
        "recorded_breakpoints"
      ],
      "type": "object"
    },
    "FlowParametricRangeV1": {
      "additionalProperties": false,
      "properties": {
        "maximum": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "minimum": {
          "$ref": "#/$defs/FlowRationalV1"
        }
      },
      "required": [
        "minimum",
        "maximum"
      ],
      "type": "object"
    },
    "FlowParametricSegmentV1": {
      "additionalProperties": false,
      "properties": {
        "intercept": {
          "type": "string"
        },
        "lower": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "maximal_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "minimal_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "slope": {
          "type": "string"
        },
        "upper": {
          "$ref": "#/$defs/FlowRationalV1"
        }
      },
      "required": [
        "lower",
        "upper",
        "intercept",
        "slope",
        "minimal_source_side",
        "maximal_source_side"
      ],
      "type": "object"
    },
    "FlowParametricTraversalV1": {
      "additionalProperties": false,
      "properties": {
        "active_nodes": {
          "type": [
            "string",
            "null"
          ]
        },
        "cold_static_rerun": {
          "type": "boolean"
        },
        "kind": {
          "type": "string"
        },
        "labels_retained": {
          "type": "boolean"
        },
        "left_active_nodes": {
          "type": [
            "string",
            "null"
          ]
        },
        "lower": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "lower_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "normalized_tree_reused": {
          "type": "boolean"
        },
        "orientation": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "probe": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "race_winner": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "renormalization_pushes": {
          "type": "string"
        },
        "renormalization_splits": {
          "type": "string"
        },
        "right_active_nodes": {
          "type": [
            "string",
            "null"
          ]
        },
        "scale_denominator": {
          "type": [
            "string",
            "null"
          ]
        },
        "static_run_ordinal": {
          "type": [
            "string",
            "null"
          ]
        },
        "upper": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "upper_source_side": {
          "items": {
            "type": "string"
          },
          "type": "array"
        }
      },
      "required": [
        "kind",
        "lower",
        "upper",
        "normalized_tree_reused",
        "labels_retained",
        "renormalization_pushes",
        "renormalization_splits"
      ],
      "type": "object"
    },
    "FlowPlanarDartDirectionV1": {
      "oneOf": [
        {
          "const": "forward",
          "type": "string"
        },
        {
          "const": "reverse",
          "type": "string"
        }
      ]
    },
    "FlowPlanarDartV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "$ref": "#/$defs/FlowPlanarDartDirectionV1"
        },
        "edge_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction"
      ],
      "type": "object"
    },
    "FlowPlanarEmbeddingV1": {
      "additionalProperties": false,
      "properties": {
        "outer_face": {
          "$ref": "#/$defs/FlowPlanarDartV1"
        },
        "rotations": {
          "items": {
            "$ref": "#/$defs/FlowPlanarRotationV1"
          },
          "type": "array"
        },
        "terminal_corners": {
          "$ref": "#/$defs/FlowPlanarTerminalCornersV1"
        }
      },
      "required": [
        "rotations",
        "outer_face"
      ],
      "type": "object"
    },
    "FlowPlanarRotationV1": {
      "additionalProperties": false,
      "properties": {
        "darts": {
          "items": {
            "$ref": "#/$defs/FlowPlanarDartV1"
          },
          "type": "array"
        },
        "node_id": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "darts"
      ],
      "type": "object"
    },
    "FlowPlanarTerminalCornersV1": {
      "additionalProperties": false,
      "properties": {
        "sink": {
          "$ref": "#/$defs/FlowPlanarDartV1"
        },
        "source": {
          "$ref": "#/$defs/FlowPlanarDartV1"
        }
      },
      "required": [
        "source",
        "sink"
      ],
      "type": "object"
    },
    "FlowPolynomialDualEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "augment_direction": {
          "type": [
            "string",
            "null"
          ]
        },
        "bad": {
          "type": "boolean"
        },
        "basic_flow": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "in_augment_path": {
          "type": "boolean"
        },
        "in_tree": {
          "type": "boolean"
        },
        "pseudoflow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "reduced_cost": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "pseudoflow",
        "basic_flow",
        "reduced_cost",
        "in_tree",
        "bad",
        "in_augment_path"
      ],
      "type": "object"
    },
    "FlowPolynomialDualNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active": {
          "type": "boolean"
        },
        "bad": {
          "type": "boolean"
        },
        "excess": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "in_pivot_cut": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        },
        "root": {
          "type": "boolean"
        }
      },
      "required": [
        "node_id",
        "potential",
        "excess",
        "root",
        "active",
        "bad",
        "in_pivot_cut"
      ],
      "type": "object"
    },
    "FlowPolynomialDualSimplexOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_node": {
          "type": [
            "string",
            "null"
          ]
        },
        "augment_path": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "bad_edges": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "bad_nodes": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "delta": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialDualEdgeStateV1"
          },
          "type": "array"
        },
        "entering_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "leaving_edge": {
          "type": [
            "string",
            "null"
          ]
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialDualNodeStateV1"
          },
          "type": "array"
        },
        "phase": {
          "type": "string"
        },
        "pivot_cut": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "pivot_price_delta": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowPolynomialDualSimplexStageV1"
        }
      },
      "required": [
        "stage",
        "phase",
        "delta",
        "nodes",
        "edges",
        "augment_path",
        "bad_edges",
        "bad_nodes",
        "pivot_cut"
      ],
      "type": "object"
    },
    "FlowPolynomialDualSimplexStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "inspect-initial-arc",
          "type": "string"
        },
        {
          "const": "initialize-tree",
          "type": "string"
        },
        {
          "const": "initialize-pseudoflow",
          "type": "string"
        },
        {
          "const": "begin-scale",
          "type": "string"
        },
        {
          "const": "inspect-augmentation-arc",
          "type": "string"
        },
        {
          "const": "select-active",
          "type": "string"
        },
        {
          "const": "augment-to-root",
          "type": "string"
        },
        {
          "const": "select-bad-arc",
          "type": "string"
        },
        {
          "const": "inspect-entering-arc",
          "type": "string"
        },
        {
          "const": "select-entering",
          "type": "string"
        },
        {
          "const": "pivot-make-good",
          "type": "string"
        },
        {
          "const": "finish-scale",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowPolynomialPrimalArtificialEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "basis": {
          "$ref": "#/$defs/FlowPolynomialPrimalBasisStateV1"
        },
        "entering": {
          "type": "boolean"
        },
        "entity_id": {
          "type": "string"
        },
        "in_cycle": {
          "type": "boolean"
        },
        "leaving": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "perturbed_flow": {
          "type": "string"
        },
        "unperturbed_basic_flow": {
          "type": "string"
        }
      },
      "required": [
        "entity_id",
        "node_id",
        "basis",
        "perturbed_flow",
        "unperturbed_basic_flow",
        "in_cycle",
        "entering",
        "leaving"
      ],
      "type": "object"
    },
    "FlowPolynomialPrimalBasisStateV1": {
      "oneOf": [
        {
          "const": "lower",
          "type": "string"
        },
        {
          "const": "tree",
          "type": "string"
        },
        {
          "const": "upper",
          "type": "string"
        }
      ]
    },
    "FlowPolynomialPrimalEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "basis": {
          "$ref": "#/$defs/FlowPolynomialPrimalBasisStateV1"
        },
        "edge_id": {
          "type": "string"
        },
        "entering": {
          "type": "boolean"
        },
        "in_cycle": {
          "type": "boolean"
        },
        "leaving": {
          "type": "boolean"
        },
        "perturbed_flow": {
          "type": "string"
        },
        "reduced_cost": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "unperturbed_basic_flow": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "basis",
        "perturbed_flow",
        "unperturbed_basic_flow",
        "reduced_cost",
        "in_cycle",
        "entering",
        "leaving"
      ],
      "type": "object"
    },
    "FlowPolynomialPrimalNodeFlagV1": {
      "oneOf": [
        {
          "const": "eligible",
          "type": "string"
        },
        {
          "const": "awake",
          "type": "string"
        },
        {
          "const": "in-n-star",
          "type": "string"
        },
        {
          "const": "root",
          "type": "string"
        }
      ]
    },
    "FlowPolynomialPrimalNodeKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "artificial-root",
          "type": "string"
        }
      ]
    },
    "FlowPolynomialPrimalNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "entity_id": {
          "type": "string"
        },
        "flags": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialPrimalNodeFlagV1"
          },
          "type": "array"
        },
        "kind": {
          "$ref": "#/$defs/FlowPolynomialPrimalNodeKindV1"
        },
        "premultiplier": {
          "$ref": "#/$defs/FlowRationalV1"
        }
      },
      "required": [
        "entity_id",
        "kind",
        "premultiplier",
        "flags"
      ],
      "type": "object"
    },
    "FlowPolynomialPrimalResidualRefV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "entity_id": {
          "type": "string"
        },
        "original_edge_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "entity_id",
        "direction"
      ],
      "type": "object"
    },
    "FlowPolynomialPrimalSimplexOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "artificial_edges": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialPrimalArtificialEdgeStateV1"
          },
          "type": "array"
        },
        "cycle": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialPrimalResidualRefV1"
          },
          "type": "array"
        },
        "delta": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialPrimalEdgeStateV1"
          },
          "type": "array"
        },
        "entering": {
          "$ref": "#/$defs/FlowPolynomialPrimalResidualRefV1"
        },
        "epsilon": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "leaving_entity": {
          "type": [
            "string",
            "null"
          ]
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowPolynomialPrimalNodeStateV1"
          },
          "type": "array"
        },
        "perturbation_scale": {
          "type": "string"
        },
        "phase": {
          "type": "string"
        },
        "potential_shift": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "stage": {
          "$ref": "#/$defs/FlowPolynomialPrimalSimplexStageV1"
        }
      },
      "required": [
        "stage",
        "phase",
        "perturbation_scale",
        "nodes",
        "edges",
        "artificial_edges",
        "cycle"
      ],
      "type": "object"
    },
    "FlowPolynomialPrimalSimplexStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "initialize-basis",
          "type": "string"
        },
        {
          "const": "begin-scale",
          "type": "string"
        },
        {
          "const": "inspect-residual",
          "type": "string"
        },
        {
          "const": "select-admissible",
          "type": "string"
        },
        {
          "const": "pivot",
          "type": "string"
        },
        {
          "const": "modify-premultipliers",
          "type": "string"
        },
        {
          "const": "finish-scale",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowPositionV1": {
      "additionalProperties": false,
      "properties": {
        "x": {
          "type": "string"
        },
        "y": {
          "type": "string"
        }
      },
      "required": [
        "x",
        "y"
      ],
      "type": "object"
    },
    "FlowPredictionAssistedEpsilonEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "scaled_cost": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "scaled_cost"
      ],
      "type": "object"
    },
    "FlowPredictionAssistedEpsilonNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active": {
          "type": "boolean"
        },
        "node_id": {
          "type": "string"
        },
        "predicted_price": {
          "type": "string"
        },
        "prediction_clipped": {
          "type": "boolean"
        },
        "price": {
          "type": "string"
        },
        "raw_predicted_price": {
          "type": "string"
        },
        "surplus": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "raw_predicted_price",
        "predicted_price",
        "prediction_clipped",
        "price",
        "surplus",
        "active"
      ],
      "type": "object"
    },
    "FlowPredictionAssistedEpsilonOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_arc": {
          "$ref": "#/$defs/FlowResidualArcRefV1"
        },
        "active_node": {
          "type": [
            "string",
            "null"
          ]
        },
        "attempt": {
          "type": "string"
        },
        "certificate_aligned_prediction_error": {
          "type": [
            "string",
            "null"
          ]
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowPredictionAssistedEpsilonEdgeStateV1"
          },
          "type": "array"
        },
        "exponent": {
          "type": "string"
        },
        "maximum_attempt": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowPredictionAssistedEpsilonNodeStateV1"
          },
          "type": "array"
        },
        "scale_exponent": {
          "type": [
            "string",
            "null"
          ]
        },
        "scaling_parameter": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowPredictionAssistedEpsilonStageV1"
        }
      },
      "required": [
        "stage",
        "scaling_parameter",
        "attempt",
        "maximum_attempt",
        "exponent",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowPredictionAssistedEpsilonStageV1": {
      "oneOf": [
        {
          "const": "preprocess-prediction",
          "type": "string"
        },
        {
          "const": "begin-attempt",
          "type": "string"
        },
        {
          "const": "initialize-scale",
          "type": "string"
        },
        {
          "const": "select-surplus",
          "type": "string"
        },
        {
          "const": "inspect-admissible-arc",
          "type": "string"
        },
        {
          "const": "inspect-price-breakpoint-arc",
          "type": "string"
        },
        {
          "const": "push",
          "type": "string"
        },
        {
          "const": "raise-price",
          "type": "string"
        },
        {
          "const": "complete-up-iteration",
          "type": "string"
        },
        {
          "const": "complete-scale",
          "type": "string"
        },
        {
          "const": "abort-attempt",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowPrimalDualIpmMcfArcKindV1": {
      "oneOf": [
        {
          "const": "upper",
          "type": "string"
        },
        {
          "const": "lower",
          "type": "string"
        },
        {
          "const": "artificial",
          "type": "string"
        }
      ]
    },
    "FlowPrimalDualIpmMcfArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_cycle_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "auxiliary_id": {
          "type": "string"
        },
        "contracted": {
          "type": "boolean"
        },
        "deleted": {
          "type": "boolean"
        },
        "flow": {
          "type": "string"
        },
        "forest_candidate": {
          "type": "boolean"
        },
        "from": {
          "type": "string"
        },
        "in_minor": {
          "type": "boolean"
        },
        "in_tree": {
          "type": "boolean"
        },
        "kind": {
          "$ref": "#/$defs/FlowPrimalDualIpmMcfArcKindV1"
        },
        "original_edge_id": {
          "type": "string"
        },
        "resistance": {
          "type": [
            "string",
            "null"
          ]
        },
        "slack": {
          "type": "string"
        },
        "to": {
          "type": "string"
        }
      },
      "required": [
        "auxiliary_id",
        "original_edge_id",
        "from",
        "to",
        "kind",
        "flow",
        "slack",
        "deleted",
        "contracted",
        "in_minor",
        "in_tree",
        "forest_candidate",
        "active_cycle_sign"
      ],
      "type": "object"
    },
    "FlowPrimalDualIpmMcfNodeKindV1": {
      "oneOf": [
        {
          "const": "original",
          "type": "string"
        },
        {
          "const": "capacity",
          "type": "string"
        }
      ]
    },
    "FlowPrimalDualIpmMcfNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "auxiliary_id": {
          "type": "string"
        },
        "component": {
          "type": "string"
        },
        "in_crossover_set": {
          "type": "boolean"
        },
        "kind": {
          "$ref": "#/$defs/FlowPrimalDualIpmMcfNodeKindV1"
        },
        "original_edge_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "original_node_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "auxiliary_id",
        "kind",
        "potential",
        "component",
        "in_crossover_set"
      ],
      "type": "object"
    },
    "FlowPrimalDualIpmMcfOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "arcs": {
          "items": {
            "$ref": "#/$defs/FlowPrimalDualIpmMcfArcStateV1"
          },
          "type": "array"
        },
        "beta": {
          "type": "string"
        },
        "centrality_numerator": {
          "type": "string"
        },
        "cycle_alpha": {
          "type": "string"
        },
        "forest_subset_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "gamma": {
          "type": "string"
        },
        "mu": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowPrimalDualIpmMcfNodeStateV1"
          },
          "type": "array"
        },
        "proxy_gap": {
          "type": "string"
        },
        "sampled_arc": {
          "type": [
            "string",
            "null"
          ]
        },
        "seed": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowPrimalDualIpmMcfStageV1"
        },
        "tree_condition_number": {
          "$ref": "#/$defs/FlowPrimalDualIpmMcfRatioV1"
        }
      },
      "required": [
        "stage",
        "seed",
        "mu",
        "beta",
        "gamma",
        "proxy_gap",
        "centrality_numerator",
        "cycle_alpha",
        "nodes",
        "arcs"
      ],
      "type": "object"
    },
    "FlowPrimalDualIpmMcfRatioV1": {
      "additionalProperties": false,
      "properties": {
        "denominator": {
          "type": "string"
        },
        "numerator": {
          "type": "string"
        }
      },
      "required": [
        "numerator",
        "denominator"
      ],
      "type": "object"
    },
    "FlowPrimalDualIpmMcfStageV1": {
      "enum": [
        "ready",
        "normalize-input",
        "build-capacity-reduction",
        "initialize-central-point",
        "build-minor",
        "decrease-mu",
        "inspect-forest-subset",
        "build-low-stretch-forest",
        "sample-fundamental-cycle",
        "centering-cycle-update",
        "centered",
        "proxy-reached",
        "crossover-grow-cut",
        "restore-original-dual",
        "recover-admissible-flow",
        "check-certificate",
        "optimal"
      ],
      "type": "string"
    },
    "FlowPrimaryWorkV1": {
      "additionalProperties": false,
      "properties": {
        "abstraction": {
          "$ref": "#/$defs/FlowWorkAbstractionV1"
        },
        "metric_ordinal": {
          "maximum": 255,
          "minimum": 0,
          "type": "integer"
        },
        "unit": {
          "type": "string"
        },
        "visualization": {
          "$ref": "#/$defs/FlowWorkVisualizationKindV1"
        }
      },
      "required": [
        "metric_ordinal",
        "unit",
        "abstraction",
        "visualization"
      ],
      "type": "object"
    },
    "FlowProblemModelV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "max-flow",
              "type": "string"
            },
            "sink": {
              "type": "string"
            },
            "source": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "source",
            "sink"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "capacity_slopes": {
              "items": {
                "$ref": "#/$defs/FlowParametricCapacitySlopeV1"
              },
              "type": "array"
            },
            "kind": {
              "const": "parametric-max-flow",
              "type": "string"
            },
            "parameter": {
              "$ref": "#/$defs/FlowParametricRangeV1"
            },
            "sink": {
              "type": "string"
            },
            "source": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "source",
            "sink",
            "parameter",
            "capacity_slopes"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "fixed-flow-min-cost",
              "type": "string"
            },
            "required_flow": {
              "type": "string"
            },
            "sink": {
              "type": "string"
            },
            "source": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "source",
            "sink",
            "required_flow"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "min-cost-max-flow",
              "type": "string"
            },
            "sink": {
              "type": "string"
            },
            "source": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "source",
            "sink"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "circulation",
              "type": "string"
            }
          },
          "required": [
            "kind"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "transshipment",
              "type": "string"
            }
          },
          "required": [
            "kind"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "flow_adapter": {
              "$ref": "#/$defs/FlowBipartiteAdapterV1"
            },
            "kind": {
              "const": "bipartite-matching",
              "type": "string"
            },
            "left": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "right": {
              "items": {
                "type": "string"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "left",
            "right"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "agents": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "kind": {
              "const": "assignment",
              "type": "string"
            },
            "objective": {
              "$ref": "#/$defs/AssignmentObjectiveV1"
            },
            "tasks": {
              "items": {
                "type": "string"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "agents",
            "tasks",
            "objective"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "destinations": {
              "items": {
                "type": "string"
              },
              "type": "array"
            },
            "kind": {
              "const": "transportation",
              "type": "string"
            },
            "origins": {
              "items": {
                "type": "string"
              },
              "type": "array"
            }
          },
          "required": [
            "kind",
            "origins",
            "destinations"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "embedding": {
              "$ref": "#/$defs/FlowPlanarEmbeddingV1"
            },
            "kind": {
              "const": "planar-max-flow",
              "type": "string"
            },
            "sink": {
              "type": "string"
            },
            "source": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "source",
            "sink",
            "embedding"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "convex-cost-flow",
              "type": "string"
            }
          },
          "required": [
            "kind"
          ],
          "type": "object"
        }
      ]
    },
    "FlowPseudoflowForestV1": {
      "additionalProperties": false,
      "properties": {
        "arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "strong_nodes": {
          "items": {
            "type": "string"
          },
          "type": "array"
        }
      },
      "required": [
        "arcs",
        "strong_nodes"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_cycle_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "active_tree_edge": {
          "type": "boolean"
        },
        "changed_coordinate": {
          "type": "boolean"
        },
        "edge_id": {
          "type": "string"
        },
        "final_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "gradient": {
          "type": "string"
        },
        "interior_flow": {
          "type": "string"
        },
        "isolation_draw": {
          "type": "string"
        },
        "length": {
          "type": "string"
        },
        "sampled_tree_memberships": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "interior_flow",
        "gradient",
        "length",
        "sampled_tree_memberships",
        "active_tree_edge",
        "active_cycle_sign",
        "changed_coordinate",
        "isolation_draw"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearMcfEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "candidate_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "current_flow": {
          "type": "string"
        },
        "detected": {
          "type": "boolean"
        },
        "edge_id": {
          "type": "string"
        },
        "final_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_flow": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "fixed_on_face": {
          "type": "boolean"
        },
        "gradient": {
          "type": "string"
        },
        "initial_flow": {
          "type": "string"
        },
        "isolated_cost": {
          "type": "string"
        },
        "isolated_optimum_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "isolation_draw": {
          "type": "string"
        },
        "length": {
          "type": "string"
        },
        "selected_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "stale_flow": {
          "type": "string"
        },
        "tree_edge": {
          "type": "boolean"
        }
      },
      "required": [
        "edge_id",
        "fixed_on_face",
        "initial_flow",
        "current_flow",
        "stale_flow",
        "isolation_draw",
        "isolated_cost",
        "tree_edge",
        "candidate_sign",
        "selected_sign",
        "gradient",
        "length",
        "detected"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearMcfNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "component": {
          "type": "string"
        },
        "depth": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "on_selected_cycle": {
          "type": "boolean"
        },
        "parent_node_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "required_divergence": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "required_divergence",
        "component",
        "depth",
        "on_selected_cycle"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearMcfOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "alpha": {
          "type": "string"
        },
        "assignment_cursor": {
          "type": [
            "string",
            "null"
          ]
        },
        "assignment_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "current_cost": {
          "type": "string"
        },
        "detected_coordinates": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowRandomizedAlmostLinearMcfEdgeStateV1"
          },
          "type": "array"
        },
        "epsilon": {
          "type": "string"
        },
        "eta": {
          "type": "string"
        },
        "exact_recovery": {
          "type": "boolean"
        },
        "failure_denominator": {
          "type": "string"
        },
        "failure_numerator": {
          "type": "string"
        },
        "feasible_flows": {
          "type": "string"
        },
        "final_point_gap": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_point_mix": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "final_point_threshold": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "forest_pool_size": {
          "type": "string"
        },
        "initial_cost": {
          "type": "string"
        },
        "isolated_optimum_cost": {
          "type": "string"
        },
        "isolation_attempt": {
          "type": "string"
        },
        "isolation_scale": {
          "type": "string"
        },
        "kappa": {
          "type": "string"
        },
        "minimum_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowRandomizedAlmostLinearMcfNodeStateV1"
          },
          "type": "array"
        },
        "optimum_cost": {
          "type": "string"
        },
        "oracle_vector_serial": {
          "type": [
            "string",
            "null"
          ]
        },
        "potential": {
          "type": "string"
        },
        "rebuilds": {
          "type": "string"
        },
        "sampled_forest_index": {
          "type": [
            "string",
            "null"
          ]
        },
        "seed": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowRandomizedAlmostLinearMcfStageV1"
        }
      },
      "required": [
        "stage",
        "seed",
        "alpha",
        "epsilon",
        "kappa",
        "eta",
        "initial_cost",
        "current_cost",
        "optimum_cost",
        "isolated_optimum_cost",
        "potential",
        "isolation_attempt",
        "isolation_scale",
        "failure_numerator",
        "failure_denominator",
        "forest_pool_size",
        "final_point_threshold",
        "exact_recovery",
        "feasible_flows",
        "detected_coordinates",
        "rebuilds",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearMcfStageV1": {
      "enum": [
        "ready",
        "inspect-feasible-assignment",
        "enumerate-feasible-set",
        "sample-isolation-costs",
        "select-isolated-optimum",
        "initialize-relative-interior",
        "inspect-oracle-vector",
        "build-forest-pool",
        "sample-tree-chain",
        "refresh-gradient-length",
        "query-minimum-ratio-cycle",
        "potential-reduction-step",
        "detect-changed-coordinates",
        "rebuild-tree-chain",
        "construct-final-point",
        "round-nearest-integer",
        "check-certificate",
        "optimal"
      ],
      "type": "string"
    },
    "FlowRandomizedAlmostLinearNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "active_artificial_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "active_artificial_tree_edge": {
          "type": "boolean"
        },
        "artificial_capacity": {
          "type": "string"
        },
        "artificial_direction": {
          "type": "string"
        },
        "artificial_flow": {
          "type": "string"
        },
        "artificial_tree_memberships": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "source_side": {
          "type": "boolean"
        },
        "tree_component": {
          "type": "string"
        },
        "tree_parent_node_id": {
          "type": [
            "string",
            "null"
          ]
        }
      },
      "required": [
        "node_id",
        "tree_component",
        "source_side",
        "artificial_direction",
        "artificial_flow",
        "artificial_capacity",
        "artificial_tree_memberships",
        "active_artificial_tree_edge",
        "active_artificial_sign"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_return_sign": {
          "enum": [
            "-1",
            "0",
            "1"
          ],
          "type": "string"
        },
        "active_return_tree_edge": {
          "type": "boolean"
        },
        "alpha": {
          "type": "string"
        },
        "artificial_edges": {
          "type": "string"
        },
        "artificial_flow": {
          "type": "string"
        },
        "cost_gap": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowRandomizedAlmostLinearEdgeStateV1"
          },
          "type": "array"
        },
        "exact_pool_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_artificial_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_gap": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_mix": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_return_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "final_point_threshold": {
          "type": "string"
        },
        "final_return_flow": {
          "type": [
            "string",
            "null"
          ]
        },
        "forest_pool_size": {
          "type": "string"
        },
        "isolated_objective": {
          "type": [
            "string",
            "null"
          ]
        },
        "isolation_attempt": {
          "type": "string"
        },
        "isolation_failure_probability": {
          "$ref": "#/$defs/FlowRandomizedAlmostLinearProbabilityV1"
        },
        "isolation_scale": {
          "type": "string"
        },
        "iteration": {
          "type": "string"
        },
        "miss_probability": {
          "$ref": "#/$defs/FlowRandomizedAlmostLinearProbabilityV1"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowRandomizedAlmostLinearNodeStateV1"
          },
          "type": "array"
        },
        "potential": {
          "type": "string"
        },
        "random_draws": {
          "type": "string"
        },
        "rebuild_epoch": {
          "type": "string"
        },
        "return_capacity": {
          "type": "string"
        },
        "return_flow": {
          "type": "string"
        },
        "return_gradient": {
          "type": "string"
        },
        "return_isolation_draw": {
          "type": "string"
        },
        "return_length": {
          "type": "string"
        },
        "return_tree_memberships": {
          "type": "string"
        },
        "sample_count": {
          "type": "string"
        },
        "seed": {
          "type": "string"
        },
        "selected_ratio": {
          "type": [
            "string",
            "null"
          ]
        },
        "stage": {
          "$ref": "#/$defs/FlowRandomizedAlmostLinearStageV1"
        },
        "target_value": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "seed",
        "random_draws",
        "alpha",
        "potential",
        "cost_gap",
        "miss_probability",
        "forest_pool_size",
        "sample_count",
        "iteration",
        "rebuild_epoch",
        "return_flow",
        "return_capacity",
        "return_gradient",
        "return_length",
        "return_tree_memberships",
        "active_return_tree_edge",
        "active_return_sign",
        "return_isolation_draw",
        "artificial_edges",
        "artificial_flow",
        "isolation_scale",
        "isolation_attempt",
        "isolation_failure_probability",
        "final_point_threshold",
        "target_value",
        "nodes",
        "edges"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearProbabilityV1": {
      "additionalProperties": false,
      "properties": {
        "denominator": {
          "type": "string"
        },
        "numerator": {
          "type": "string"
        }
      },
      "required": [
        "numerator",
        "denominator"
      ],
      "type": "object"
    },
    "FlowRandomizedAlmostLinearStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "build-return-edge-reduction",
          "type": "string"
        },
        {
          "const": "build-initial-point",
          "type": "string"
        },
        {
          "const": "enumerate-forest-pool",
          "type": "string"
        },
        {
          "const": "sample-tree-chain",
          "type": "string"
        },
        {
          "const": "inspect-fundamental-cycle",
          "type": "string"
        },
        {
          "const": "query-minimum-ratio-cycle",
          "type": "string"
        },
        {
          "const": "sampling-failure",
          "type": "string"
        },
        {
          "const": "potential-reduction-step",
          "type": "string"
        },
        {
          "const": "detect-changed-coordinates",
          "type": "string"
        },
        {
          "const": "rebuild-tree-chain",
          "type": "string"
        },
        {
          "const": "inspect-feasible-assignment",
          "type": "string"
        },
        {
          "const": "enumerate-feasible-set",
          "type": "string"
        },
        {
          "const": "sample-isolation-costs",
          "type": "string"
        },
        {
          "const": "select-isolated-optimum",
          "type": "string"
        },
        {
          "const": "construct-final-point",
          "type": "string"
        },
        {
          "const": "round-nearest-integer",
          "type": "string"
        },
        {
          "const": "check-certificate",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowRationalV1": {
      "additionalProperties": false,
      "properties": {
        "denominator": {
          "type": "string"
        },
        "numerator": {
          "type": "string"
        }
      },
      "required": [
        "numerator",
        "denominator"
      ],
      "type": "object"
    },
    "FlowRelaxedMndcAssignmentCellV1": {
      "additionalProperties": false,
      "properties": {
        "column_node_id": {
          "type": "string"
        },
        "row_node_id": {
          "type": "string"
        }
      },
      "required": [
        "row_node_id",
        "column_node_id"
      ],
      "type": "object"
    },
    "FlowRelaxedMndcCycleV1": {
      "additionalProperties": false,
      "properties": {
        "arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "delta": {
          "type": [
            "string",
            "null"
          ]
        },
        "transformed_cost": {
          "type": "string"
        }
      },
      "required": [
        "transformed_cost",
        "arcs"
      ],
      "type": "object"
    },
    "FlowRelaxedMndcNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "left_dual": {
          "type": "string"
        },
        "matched_node_id": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "right_dual": {
          "type": "string"
        },
        "selected_arc": {
          "$ref": "#/$defs/FlowResidualArcRefV1"
        }
      },
      "required": [
        "node_id",
        "matched_node_id",
        "left_dual",
        "right_dual"
      ],
      "type": "object"
    },
    "FlowRelaxedMndcOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_assignment_cell": {
          "$ref": "#/$defs/FlowRelaxedMndcAssignmentCellV1"
        },
        "assignment_value": {
          "type": [
            "string",
            "null"
          ]
        },
        "epsilon": {
          "$ref": "#/$defs/FlowRationalV1"
        },
        "family": {
          "items": {
            "$ref": "#/$defs/FlowRelaxedMndcCycleV1"
          },
          "type": "array"
        },
        "inspected_arcs": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowRelaxedMndcNodeStateV1"
          },
          "type": "array"
        },
        "phase": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowRelaxedMndcStageV1"
        }
      },
      "required": [
        "stage",
        "epsilon",
        "phase",
        "nodes",
        "family",
        "inspected_arcs"
      ],
      "type": "object"
    },
    "FlowRelaxedMndcStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "initialize",
          "type": "string"
        },
        {
          "const": "begin-phase",
          "type": "string"
        },
        {
          "const": "inspect-residual-arc",
          "type": "string"
        },
        {
          "const": "inspect-assignment-cell",
          "type": "string"
        },
        {
          "const": "select-family",
          "type": "string"
        },
        {
          "const": "cancel-family",
          "type": "string"
        },
        {
          "const": "phase-optimal",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        }
      ]
    },
    "FlowResidualArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction"
      ],
      "type": "object"
    },
    "FlowResidualArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "active": {
          "type": "boolean"
        },
        "capacity": {
          "type": "string"
        },
        "cost": {
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "fixed": {
          "type": "boolean"
        },
        "from": {
          "type": "string"
        },
        "to": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction",
        "from",
        "to",
        "capacity",
        "cost",
        "active"
      ],
      "type": "object"
    },
    "FlowResourceLimitReasonV1": {
      "oneOf": [
        {
          "const": "input-admission",
          "type": "string"
        },
        {
          "const": "runtime-work",
          "type": "string"
        },
        {
          "const": "transformed-graph",
          "type": "string"
        },
        {
          "const": "trace-publication",
          "type": "string"
        },
        {
          "const": "numerical-convergence",
          "type": "string"
        },
        {
          "const": "declared-ceiling",
          "type": "string"
        }
      ]
    },
    "FlowSolveStatusV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "running",
          "type": "string"
        },
        {
          "const": "primitive-complete",
          "type": "string"
        },
        {
          "const": "optimal",
          "type": "string"
        },
        {
          "const": "infeasible",
          "type": "string"
        },
        {
          "const": "resource-limit",
          "type": "string"
        },
        {
          "const": "cancelled",
          "type": "string"
        }
      ]
    },
    "FlowStepAvailabilityV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "availability": {
              "const": "available",
              "type": "string"
            }
          },
          "required": [
            "availability"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "availability": {
              "const": "unavailable",
              "type": "string"
            },
            "reason": {
              "type": "string"
            }
          },
          "required": [
            "availability",
            "reason"
          ],
          "type": "object"
        }
      ]
    },
    "FlowTardosFixedVariableV1": {
      "additionalProperties": false,
      "properties": {
        "bound": {
          "enum": [
            "lower",
            "upper"
          ],
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "reduced_cost": {
          "type": "string"
        },
        "value": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "bound",
        "value",
        "direction",
        "reduced_cost"
      ],
      "type": "object"
    },
    "FlowTardosFrameworkOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "determinant_bound": {
          "type": "string"
        },
        "epsilon": {
          "type": "string"
        },
        "fixed_variables": {
          "items": {
            "$ref": "#/$defs/FlowTardosFixedVariableV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowTardosNodeStateV1"
          },
          "type": "array"
        },
        "residual_arcs": {
          "items": {
            "$ref": "#/$defs/FlowTardosResidualStateV1"
          },
          "type": "array"
        },
        "stage": {
          "$ref": "#/$defs/FlowTardosFrameworkStageV1"
        },
        "threshold": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "epsilon",
        "threshold",
        "determinant_bound",
        "nodes",
        "residual_arcs",
        "fixed_variables"
      ],
      "type": "object"
    },
    "FlowTardosFrameworkStageV1": {
      "oneOf": [
        {
          "const": "ready",
          "type": "string"
        },
        {
          "const": "construct-feasible-flow",
          "type": "string"
        },
        {
          "const": "measure-epsilon",
          "type": "string"
        },
        {
          "const": "classify-fixed-variables",
          "type": "string"
        },
        {
          "const": "complete",
          "type": "string"
        }
      ]
    },
    "FlowTardosNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "node_id": {
          "type": "string"
        },
        "potential": {
          "type": "string"
        }
      },
      "required": [
        "node_id",
        "potential"
      ],
      "type": "object"
    },
    "FlowTardosResidualStateV1": {
      "additionalProperties": false,
      "properties": {
        "capacity": {
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "fixes_variable": {
          "type": "boolean"
        },
        "reduced_cost": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction",
        "capacity",
        "reduced_cost",
        "fixes_variable"
      ],
      "type": "object"
    },
    "FlowTraceEntityRefSceneV1": {
      "oneOf": [
        {
          "additionalProperties": false,
          "properties": {
            "kind": {
              "const": "node",
              "type": "string"
            },
            "node_id": {
              "type": "string"
            }
          },
          "required": [
            "kind",
            "node_id"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "edge_id": {
              "type": "string"
            },
            "kind": {
              "const": "edge",
              "type": "string"
            }
          },
          "required": [
            "kind",
            "edge_id"
          ],
          "type": "object"
        },
        {
          "additionalProperties": false,
          "properties": {
            "direction": {
              "enum": [
                "forward",
                "reverse"
              ],
              "type": "string"
            },
            "edge_id": {
              "type": "string"
            },
            "kind": {
              "const": "residual-arc",
              "type": "string"
            }
          },
          "required": [
            "kind",
            "edge_id",
            "direction"
          ],
          "type": "object"
        }
      ]
    },
    "FlowTraceEventDetailSceneV1": {
      "additionalProperties": false,
      "properties": {
        "label": {
          "type": "string"
        },
        "value": {
          "type": "string"
        }
      },
      "required": [
        "label",
        "value"
      ],
      "type": "object"
    },
    "FlowTraceEventRoleV1": {
      "oneOf": [
        {
          "const": "observe",
          "type": "string"
        },
        {
          "const": "select",
          "type": "string"
        },
        {
          "const": "mutate",
          "type": "string"
        },
        {
          "const": "commit",
          "type": "string"
        },
        {
          "const": "certify",
          "type": "string"
        }
      ]
    },
    "FlowTraceEventSceneV1": {
      "additionalProperties": false,
      "properties": {
        "catalog_id": {
          "type": "string"
        },
        "detail": {
          "$ref": "#/$defs/FlowTraceEventDetailSceneV1"
        },
        "entity_refs": {
          "items": {
            "$ref": "#/$defs/FlowTraceEntityRefSceneV1"
          },
          "type": "array"
        },
        "event_id": {
          "type": "string"
        },
        "minimum_granularity": {
          "$ref": "#/$defs/TraceGranularityV1"
        },
        "parent_phase_id": {
          "type": [
            "string",
            "null"
          ]
        },
        "patch_count": {
          "minimum": 0,
          "type": "integer"
        },
        "pseudocode_line": {
          "type": "string"
        }
      },
      "required": [
        "event_id",
        "catalog_id",
        "minimum_granularity",
        "pseudocode_line",
        "patch_count",
        "entity_refs"
      ],
      "type": "object"
    },
    "FlowTraceEventSemanticsV1": {
      "additionalProperties": false,
      "properties": {
        "aggregation_count": {
          "type": "string"
        },
        "changed_entity_refs": {
          "items": {
            "$ref": "#/$defs/FlowTraceEntityRefSceneV1"
          },
          "type": "array"
        },
        "primary_work_block": {
          "$ref": "#/$defs/FlowTracePrimaryWorkBlockV1"
        },
        "role": {
          "$ref": "#/$defs/FlowTraceEventRoleV1"
        },
        "work_deltas": {
          "items": {
            "$ref": "#/$defs/FlowTraceWorkDeltaV1"
          },
          "type": "array"
        },
        "work_progress": {
          "$ref": "#/$defs/FlowTraceWorkProgressV1"
        }
      },
      "required": [
        "role",
        "work_deltas",
        "aggregation_count",
        "work_progress",
        "changed_entity_refs"
      ],
      "type": "object"
    },
    "FlowTracePrimaryWorkBlockV1": {
      "additionalProperties": false,
      "properties": {
        "first": {
          "type": "string"
        },
        "last": {
          "type": "string"
        },
        "total": {
          "type": "string"
        }
      },
      "required": [
        "first",
        "last",
        "total"
      ],
      "type": "object"
    },
    "FlowTraceWorkDeltaV1": {
      "additionalProperties": false,
      "properties": {
        "count": {
          "type": "string"
        },
        "unit": {
          "$ref": "#/$defs/FlowTraceWorkUnitV1"
        }
      },
      "required": [
        "unit",
        "count"
      ],
      "type": "object"
    },
    "FlowTraceWorkProgressV1": {
      "additionalProperties": false,
      "properties": {
        "detail_completed": {
          "type": "string"
        },
        "detail_total": {
          "type": "string"
        },
        "primary_completed": {
          "type": "string"
        },
        "primary_total": {
          "type": "string"
        }
      },
      "required": [
        "detail_completed",
        "detail_total",
        "primary_completed",
        "primary_total"
      ],
      "type": "object"
    },
    "FlowTraceWorkUnitV1": {
      "oneOf": [
        {
          "const": "published-transition",
          "type": "string"
        },
        {
          "const": "detail-primitive",
          "type": "string"
        },
        {
          "const": "primary-work",
          "type": "string"
        },
        {
          "const": "bfs-run",
          "type": "string"
        },
        {
          "const": "relaxation-pass",
          "type": "string"
        },
        {
          "const": "residual-arc-scan",
          "type": "string"
        },
        {
          "const": "augmentation",
          "type": "string"
        },
        {
          "const": "path-search",
          "type": "string"
        },
        {
          "const": "scaling-phase",
          "type": "string"
        },
        {
          "const": "blocking-flow-phase",
          "type": "string"
        },
        {
          "const": "relabel",
          "type": "string"
        },
        {
          "const": "retreat",
          "type": "string"
        },
        {
          "const": "reverse-bfs-run",
          "type": "string"
        },
        {
          "const": "gap-termination",
          "type": "string"
        },
        {
          "const": "push",
          "type": "string"
        },
        {
          "const": "saturating-push",
          "type": "string"
        },
        {
          "const": "nonsaturating-push",
          "type": "string"
        },
        {
          "const": "discharge",
          "type": "string"
        },
        {
          "const": "active-vertex-selection",
          "type": "string"
        },
        {
          "const": "potential-update",
          "type": "string"
        },
        {
          "const": "simplex-pivot",
          "type": "string"
        },
        {
          "const": "negative-cycle-search",
          "type": "string"
        },
        {
          "const": "cycle-cancellation",
          "type": "string"
        },
        {
          "const": "arc-saturation",
          "type": "string"
        }
      ]
    },
    "FlowWeightedAugmentingEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "scaled_capacity": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "scaled_capacity",
        "flow"
      ],
      "type": "object"
    },
    "FlowWeightedAugmentingHierarchyKindV1": {
      "oneOf": [
        {
          "const": "dag",
          "type": "string"
        },
        {
          "const": "expanding",
          "type": "string"
        }
      ]
    },
    "FlowWeightedAugmentingNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "alive": {
          "type": "boolean"
        },
        "component": {
          "type": "string"
        },
        "expansion_witness_side": {
          "type": "boolean"
        },
        "label": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "order": {
          "type": "string"
        },
        "source_side": {
          "type": "boolean"
        }
      },
      "required": [
        "node_id",
        "component",
        "order",
        "label",
        "alive",
        "expansion_witness_side",
        "source_side"
      ],
      "type": "object"
    },
    "FlowWeightedAugmentingPathsOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_bottleneck": {
          "type": "string"
        },
        "active_path": {
          "items": {
            "$ref": "#/$defs/FlowResidualArcRefV1"
          },
          "type": "array"
        },
        "augmentations": {
          "type": "string"
        },
        "augmented_units": {
          "type": "string"
        },
        "capacity_bit": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowWeightedAugmentingEdgeStateV1"
          },
          "type": "array"
        },
        "height": {
          "type": "string"
        },
        "hierarchy_cuts": {
          "type": "string"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowWeightedAugmentingNodeStateV1"
          },
          "type": "array"
        },
        "phase": {
          "type": "string"
        },
        "phase_count": {
          "type": "string"
        },
        "phi_denominator": {
          "type": "string"
        },
        "phi_numerator": {
          "type": "string"
        },
        "relabel_jumps": {
          "type": "string"
        },
        "residual_arcs": {
          "items": {
            "$ref": "#/$defs/FlowWeightedAugmentingResidualArcStateV1"
          },
          "type": "array"
        },
        "round": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowWeightedAugmentingPathsStageV1"
        }
      },
      "required": [
        "stage",
        "phase",
        "phase_count",
        "capacity_bit",
        "round",
        "height",
        "phi_numerator",
        "phi_denominator",
        "active_bottleneck",
        "hierarchy_cuts",
        "relabel_jumps",
        "augmentations",
        "augmented_units",
        "nodes",
        "edges",
        "residual_arcs",
        "active_path"
      ],
      "type": "object"
    },
    "FlowWeightedAugmentingPathsStageV1": {
      "enum": [
        "ready",
        "begin-capacity-phase",
        "build-hierarchy",
        "certify-expansion",
        "assign-weights",
        "relabel-sweep",
        "augment-path",
        "finish-weighted-round",
        "finish-capacity-phase",
        "check-certificate",
        "optimal"
      ],
      "type": "string"
    },
    "FlowWeightedAugmentingResidualArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "active": {
          "type": "boolean"
        },
        "admissible": {
          "type": "boolean"
        },
        "capacity": {
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "from": {
          "type": "string"
        },
        "hierarchy_kind": {
          "$ref": "#/$defs/FlowWeightedAugmentingHierarchyKindV1"
        },
        "to": {
          "type": "string"
        },
        "weight": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction",
        "from",
        "to",
        "capacity",
        "weight",
        "admissible",
        "active"
      ],
      "type": "object"
    },
    "FlowWeightedPushRelabelShortcutArcRefV1": {
      "additionalProperties": false,
      "properties": {
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction"
      ],
      "type": "object"
    },
    "FlowWeightedPushRelabelShortcutEdgeStateV1": {
      "additionalProperties": false,
      "properties": {
        "capacity": {
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "flow": {
          "type": "string"
        },
        "from": {
          "type": "string"
        },
        "kind": {
          "enum": [
            "original",
            "shortcut"
          ],
          "type": "string"
        },
        "shortcut_component": {
          "type": [
            "string",
            "null"
          ]
        },
        "to": {
          "type": "string"
        },
        "weight": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "kind",
        "from",
        "to",
        "capacity",
        "flow",
        "weight"
      ],
      "type": "object"
    },
    "FlowWeightedPushRelabelShortcutNodeStateV1": {
      "additionalProperties": false,
      "properties": {
        "alive": {
          "type": "boolean"
        },
        "component": {
          "type": "string"
        },
        "label": {
          "type": "string"
        },
        "node_id": {
          "type": "string"
        },
        "order": {
          "type": "string"
        },
        "original": {
          "type": "boolean"
        },
        "source_side": {
          "type": "boolean"
        },
        "sparse_cut_side": {
          "type": "boolean"
        }
      },
      "required": [
        "node_id",
        "original",
        "component",
        "order",
        "label",
        "alive",
        "sparse_cut_side",
        "source_side"
      ],
      "type": "object"
    },
    "FlowWeightedPushRelabelShortcutOverlayV1": {
      "additionalProperties": false,
      "properties": {
        "active_bottleneck": {
          "type": "string"
        },
        "active_path": {
          "items": {
            "$ref": "#/$defs/FlowWeightedPushRelabelShortcutArcRefV1"
          },
          "type": "array"
        },
        "active_relabel_nodes": {
          "items": {
            "type": "string"
          },
          "type": "array"
        },
        "augmentations": {
          "type": "string"
        },
        "completion_augmentations": {
          "type": "string"
        },
        "completion_relabel_steps": {
          "type": "string"
        },
        "demand": {
          "type": "string"
        },
        "edges": {
          "items": {
            "$ref": "#/$defs/FlowWeightedPushRelabelShortcutEdgeStateV1"
          },
          "type": "array"
        },
        "height": {
          "type": "string"
        },
        "hierarchy_levels": {
          "type": "string"
        },
        "inspected_arcs": {
          "items": {
            "$ref": "#/$defs/FlowWeightedPushRelabelShortcutArcRefV1"
          },
          "type": "array"
        },
        "nodes": {
          "items": {
            "$ref": "#/$defs/FlowWeightedPushRelabelShortcutNodeStateV1"
          },
          "type": "array"
        },
        "psi_denominator": {
          "type": "string"
        },
        "psi_numerator": {
          "type": "string"
        },
        "relabel_steps": {
          "type": "string"
        },
        "residual_arcs": {
          "items": {
            "$ref": "#/$defs/FlowWeightedPushRelabelShortcutResidualArcStateV1"
          },
          "type": "array"
        },
        "residual_rounds": {
          "type": "string"
        },
        "routed": {
          "type": "string"
        },
        "shortcut_traversals": {
          "type": "string"
        },
        "sparse_cut_capacity": {
          "type": "string"
        },
        "sparse_cut_level": {
          "type": "string"
        },
        "stage": {
          "$ref": "#/$defs/FlowWeightedPushRelabelShortcutStageV1"
        },
        "weighted_length": {
          "type": "string"
        },
        "weighted_length_units": {
          "type": "string"
        }
      },
      "required": [
        "stage",
        "hierarchy_levels",
        "psi_numerator",
        "psi_denominator",
        "height",
        "demand",
        "routed",
        "weighted_length",
        "weighted_length_units",
        "sparse_cut_level",
        "sparse_cut_capacity",
        "active_bottleneck",
        "relabel_steps",
        "augmentations",
        "shortcut_traversals",
        "residual_rounds",
        "completion_relabel_steps",
        "completion_augmentations",
        "nodes",
        "edges",
        "residual_arcs",
        "active_path",
        "inspected_arcs",
        "active_relabel_nodes"
      ],
      "type": "object"
    },
    "FlowWeightedPushRelabelShortcutResidualArcStateV1": {
      "additionalProperties": false,
      "properties": {
        "active": {
          "type": "boolean"
        },
        "admissible": {
          "type": "boolean"
        },
        "capacity": {
          "type": "string"
        },
        "direction": {
          "enum": [
            "forward",
            "reverse"
          ],
          "type": "string"
        },
        "edge_id": {
          "type": "string"
        },
        "from": {
          "type": "string"
        },
        "to": {
          "type": "string"
        },
        "weight": {
          "type": "string"
        }
      },
      "required": [
        "edge_id",
        "direction",
        "from",
        "to",
        "capacity",
        "weight",
        "admissible",
        "active"
      ],
      "type": "object"
    },
    "FlowWeightedPushRelabelShortcutStageV1": {
      "enum": [
        "ready",
        "build-weak-hierarchy",
        "build-shortcut-graph",
        "assign-weights",
        "initialize-demand",
        "relabel-sweep",
        "relabel-checkpoint",
        "inspect-primitive-arc-checkpoint",
        "augment-path",
        "measure-short-flow",
        "compute-distance-layers",
        "select-sparse-cut",
        "completion-inspect-primitive-arc-checkpoint",
        "completion-relabel-checkpoint",
        "completion-augment-path",
        "completion-residual-round",
        "complete-residual-rounds",
        "check-certificate",
        "optimal"
      ],
      "type": "string"
    },
    "FlowWorkAbstractionV1": {
      "oneOf": [
        {
          "const": "primitive",
          "type": "string"
        },
        {
          "const": "iteration",
          "type": "string"
        },
        {
          "const": "oracle-call",
          "type": "string"
        }
      ]
    },
    "FlowWorkVisualizationKindV1": {
      "oneOf": [
        {
          "const": "edge-field",
          "type": "string"
        },
        {
          "const": "candidate-field",
          "type": "string"
        },
        {
          "const": "numeric-field",
          "type": "string"
        }
      ]
    },
    "RunProfileV1": {
      "oneOf": [
        {
          "const": "trace",
          "type": "string"
        },
        {
          "const": "fast",
          "type": "string"
        },
        {
          "const": "cpu-parallel",
          "type": "string"
        }
      ]
    },
    "TraceGranularityV1": {
      "oneOf": [
        {
          "const": "phase",
          "type": "string"
        },
        {
          "const": "operation",
          "type": "string"
        },
        {
          "const": "micro",
          "type": "string"
        }
      ]
    }
  },
  "additionalProperties": false,
  "properties": {
    "algorithm": {
      "$ref": "#/$defs/FlowAlgorithmSelectionV1"
    },
    "augmenting_electrical_overlay": {
      "$ref": "#/$defs/FlowAugmentingElectricalOverlayV1"
    },
    "binary_blocking_overlay": {
      "$ref": "#/$defs/FlowBinaryBlockingOverlayV1"
    },
    "cancel_tighten_overlay": {
      "$ref": "#/$defs/FlowCancelTightenOverlayV1"
    },
    "convex_cost_overlay": {
      "$ref": "#/$defs/FlowConvexCostOverlayV1"
    },
    "convex_network_simplex_overlay": {
      "$ref": "#/$defs/FlowConvexNetworkSimplexOverlayV1"
    },
    "deterministic_almost_linear_overlay": {
      "$ref": "#/$defs/FlowDeterministicAlmostLinearOverlayV1"
    },
    "double_scaling_overlay": {
      "$ref": "#/$defs/FlowDoubleScalingOverlayV1"
    },
    "dual_network_simplex_overlay": {
      "$ref": "#/$defs/FlowDualNetworkSimplexOverlayV1"
    },
    "dynamic_eibfs_overlay": {
      "$ref": "#/$defs/FlowDynamicEibfsOverlayV1"
    },
    "edge_states": {
      "items": {
        "$ref": "#/$defs/FlowEdgeStateV1"
      },
      "type": "array"
    },
    "eibfs_overlay": {
      "$ref": "#/$defs/FlowEibfsOverlayV1"
    },
    "electrical_flow_overlay": {
      "$ref": "#/$defs/FlowElectricalFlowOverlayV1"
    },
    "electrical_ipm_mcf_overlay": {
      "$ref": "#/$defs/FlowElectricalIpmMcfOverlayV1"
    },
    "enhanced_capacity_scaling_overlay": {
      "$ref": "#/$defs/FlowEnhancedCapacityScalingOverlayV1"
    },
    "event_count": {
      "type": "string"
    },
    "event_id": {
      "type": "string"
    },
    "feasibility_overlay": {
      "$ref": "#/$defs/FlowFeasibilityOverlayV2"
    },
    "feasibility_work": {
      "$ref": "#/$defs/FlowFeasibilityWorkSummaryV1"
    },
    "flow_framework_mcf_overlay": {
      "$ref": "#/$defs/FlowFrameworkMcfOverlayV1"
    },
    "frame_revision": {
      "const": "flow-scene/9",
      "type": "string"
    },
    "graph": {
      "$ref": "#/$defs/FlowGraphV1"
    },
    "interior_point_max_flow_overlay": {
      "$ref": "#/$defs/FlowInteriorPointMaxFlowOverlayV1"
    },
    "metrics": {
      "items": {
        "type": "string"
      },
      "maxItems": 16,
      "minItems": 16,
      "type": "array"
    },
    "minimum_ratio_cycle_mcf_overlay": {
      "$ref": "#/$defs/FlowMinimumRatioCycleMcfOverlayV1"
    },
    "minimum_ratio_cycle_overlay": {
      "$ref": "#/$defs/FlowMinimumRatioCycleOverlayV1"
    },
    "model": {
      "$ref": "#/$defs/FlowProblemModelV1"
    },
    "node_trace_states": {
      "items": {
        "$ref": "#/$defs/FlowNodeTraceStateV1"
      },
      "type": "array"
    },
    "orlin_max_flow_overlay": {
      "$ref": "#/$defs/FlowOrlinMaxFlowOverlayV1"
    },
    "orlin_mcf_overlay": {
      "$ref": "#/$defs/FlowOrlinMcfOverlayV1"
    },
    "outcome": {
      "$ref": "#/$defs/FlowOutcomeV1"
    },
    "parametric_overlay": {
      "$ref": "#/$defs/FlowParametricOverlayV1"
    },
    "polynomial_dual_simplex_overlay": {
      "$ref": "#/$defs/FlowPolynomialDualSimplexOverlayV1"
    },
    "polynomial_primal_simplex_overlay": {
      "$ref": "#/$defs/FlowPolynomialPrimalSimplexOverlayV1"
    },
    "prediction_assisted_epsilon_overlay": {
      "$ref": "#/$defs/FlowPredictionAssistedEpsilonOverlayV1"
    },
    "primal_dual_ipm_mcf_overlay": {
      "$ref": "#/$defs/FlowPrimalDualIpmMcfOverlayV1"
    },
    "pseudoflow_forest": {
      "$ref": "#/$defs/FlowPseudoflowForestV1"
    },
    "randomized_almost_linear_mcf_overlay": {
      "$ref": "#/$defs/FlowRandomizedAlmostLinearMcfOverlayV1"
    },
    "randomized_almost_linear_overlay": {
      "$ref": "#/$defs/FlowRandomizedAlmostLinearOverlayV1"
    },
    "relaxed_mndc_overlay": {
      "$ref": "#/$defs/FlowRelaxedMndcOverlayV1"
    },
    "residual_arcs": {
      "items": {
        "$ref": "#/$defs/FlowResidualArcStateV1"
      },
      "type": "array"
    },
    "resource_limit_reason": {
      "$ref": "#/$defs/FlowResourceLimitReasonV1"
    },
    "result_schema_version": {
      "const": 9,
      "type": "integer"
    },
    "run_profile": {
      "$ref": "#/$defs/RunProfileV1"
    },
    "solve_status": {
      "$ref": "#/$defs/FlowSolveStatusV1"
    },
    "tardos_framework_overlay": {
      "$ref": "#/$defs/FlowTardosFrameworkOverlayV1"
    },
    "trace_event": {
      "$ref": "#/$defs/FlowTraceEventSceneV1"
    },
    "trace_event_semantics": {
      "$ref": "#/$defs/FlowTraceEventSemanticsV1"
    },
    "trace_granularity": {
      "$ref": "#/$defs/TraceGranularityV1"
    },
    "trace_steps": {
      "$ref": "#/$defs/FlowAlgorithmStepContractV1"
    },
    "weighted_augmenting_paths_overlay": {
      "$ref": "#/$defs/FlowWeightedAugmentingPathsOverlayV1"
    },
    "weighted_push_relabel_shortcut_overlay": {
      "$ref": "#/$defs/FlowWeightedPushRelabelShortcutOverlayV1"
    }
  },
  "required": [
    "result_schema_version",
    "frame_revision",
    "event_id",
    "event_count",
    "solve_status",
    "model",
    "graph",
    "algorithm",
    "run_profile",
    "trace_granularity",
    "trace_steps",
    "edge_states",
    "residual_arcs",
    "node_trace_states",
    "metrics"
  ],
  "type": "object"
} as const;
