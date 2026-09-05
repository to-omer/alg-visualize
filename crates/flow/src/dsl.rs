//! Bounded line-oriented DSL for the flow Scenario contract.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::Serialize;
use thiserror::Error;
use visualizer_core::scenario::{ReproducibilityMetadata, ReproducibilityRevisionSet};

use crate::assignment::AssignmentObjectiveV1;
use crate::scenario::{
    ALGORITHM_REVISION, FRAME_ENCODING_REVISION, FlowAlgorithmSelectionV1, FlowBipartiteAdapterV1,
    FlowEdgeV1, FlowGraphV1, FlowNodeV1, FlowParametricCapacitySlopeV1, FlowParametricRangeV1,
    FlowPositionV1, FlowProblemModelV1, FlowRationalV1, FlowScenarioPayloadV1, FlowScenarioV1,
    FlowUpdateV1, LAYOUT_REVISION, METRICS_CATALOG_REVISION, PLUGIN_RESULT_REVISION,
    PROJECTION_REVISION, RNG_VERSION, RunProfileV1, TRACE_REVISION, TraceGranularityV1,
    validate_flow_scenario,
};

/// Maximum accepted UTF-8 bytes for one DSL document.
pub const MAX_FLOW_DSL_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of lexical tokens in one DSL document.
pub const MAX_FLOW_DSL_TOKENS: usize = 1_000_000;

/// Stable source location using the browser/editor UTF-16 coordinate contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct FlowDslSpan {
    /// Zero-based UTF-16 offset from the start of the document.
    pub start_utf16: u32,
    /// Exclusive zero-based UTF-16 offset from the start of the document.
    pub end_utf16: u32,
    /// One-based line number.
    pub line: u32,
    /// One-based UTF-16 column.
    pub column_utf16: u32,
}

/// Stable, structured parser diagnostic safe to surface in the editor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FlowDslDiagnostic {
    /// Revision-stable machine-readable diagnostic code.
    pub code: &'static str,
    /// Concise human-readable diagnostic.
    pub message: String,
    /// Exact offending source span.
    pub span: FlowDslSpan,
}

/// Flow DSL lexical, syntactic, or semantic failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("{diagnostic}")]
pub struct FlowDslError {
    /// Structured first diagnostic. Parsing never publishes a partial Scenario.
    pub diagnostic: FlowDslDiagnostic,
}

impl fmt::Display for FlowDslDiagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at line {}, column {} ({})",
            self.message, self.span.line, self.span.column_utf16, self.code
        )
    }
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    span: FlowDslSpan,
}

#[derive(Default)]
struct DocumentBuilder {
    model: Option<(FlowProblemModelV1, FlowDslSpan)>,
    nodes: Vec<FlowNodeV1>,
    edges: Vec<FlowEdgeV1>,
    algorithm: Option<(String, FlowDslSpan)>,
    profile: Option<(RunProfileV1, FlowDslSpan)>,
    granularity: Option<(TraceGranularityV1, FlowDslSpan)>,
    seed: Option<(String, FlowDslSpan)>,
    updates: Vec<FlowUpdateV1>,
    parametric_capacities: Vec<(FlowParametricCapacitySlopeV1, FlowDslSpan)>,
}

/// Parses a complete DSL document and validates the resulting typed Scenario.
///
/// # Errors
///
/// Rejects oversize input, malformed tokens or statements, duplicate singleton
/// declarations, unknown attributes, noncanonical integers, and every semantic
/// error rejected by the JSON Scenario decoder.
pub fn decode_flow_dsl(source: &str) -> Result<FlowScenarioV1, FlowDslError> {
    if source.len() > MAX_FLOW_DSL_BYTES {
        return Err(error(
            "FDSL001",
            "Flow DSL exceeds the 64 MiB document limit",
            start_span(),
        ));
    }
    let lines = lex(source)?;
    let mut builder = DocumentBuilder::default();
    for tokens in lines {
        parse_statement(&mut builder, &tokens)?;
    }
    finish(builder)
}

fn lex(source: &str) -> Result<Vec<Vec<Token>>, FlowDslError> {
    let mut result = Vec::new();
    let mut document_utf16 = 0_u32;
    let mut token_count = 0_usize;
    for (line_index, raw_line) in source.split_inclusive('\n').enumerate() {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line_without_cr = line.strip_suffix('\r').unwrap_or(line);
        let tokens = lex_line(line_without_cr, line_index, document_utf16)?;
        token_count = token_count
            .checked_add(tokens.len())
            .ok_or_else(|| error("FDSL002", "Flow DSL token count overflow", start_span()))?;
        if token_count > MAX_FLOW_DSL_TOKENS {
            return Err(error(
                "FDSL002",
                "Flow DSL exceeds the token limit",
                start_span(),
            ));
        }
        if !tokens.is_empty() {
            result.push(tokens);
        }
        document_utf16 = document_utf16
            .checked_add(u32_utf16_len(raw_line)?)
            .ok_or_else(|| error("FDSL003", "Flow DSL UTF-16 offset overflow", start_span()))?;
    }
    if source.is_empty() {
        return Ok(result);
    }
    Ok(result)
}

fn lex_line(
    line: &str,
    zero_based_line: usize,
    document_utf16: u32,
) -> Result<Vec<Token>, FlowDslError> {
    let mut tokens = Vec::new();
    let mut cursor = 0_usize;
    while cursor < line.len() {
        let character = line[cursor..]
            .chars()
            .next()
            .expect("cursor is a char boundary");
        if character.is_whitespace() {
            cursor += character.len_utf8();
            continue;
        }
        if character == '#' {
            break;
        }
        if line[cursor..].starts_with("->") {
            tokens.push(make_token(
                "->".to_owned(),
                line,
                cursor,
                cursor + 2,
                zero_based_line,
                document_utf16,
            )?);
            cursor += 2;
            continue;
        }
        let start = cursor;
        let mut quote_open = false;
        let mut escaped = false;
        while cursor < line.len() {
            let character = line[cursor..]
                .chars()
                .next()
                .expect("cursor is a char boundary");
            if quote_open {
                cursor += character.len_utf8();
                if escaped {
                    escaped = false;
                } else if character == '\\' {
                    escaped = true;
                } else if character == '"' {
                    quote_open = false;
                }
                continue;
            }
            if character == '"' {
                quote_open = true;
                cursor += 1;
                continue;
            }
            if character.is_whitespace() || character == '#' || line[cursor..].starts_with("->") {
                break;
            }
            cursor += character.len_utf8();
        }
        if quote_open {
            return Err(error(
                "FDSL004",
                "unterminated quoted string",
                span(line, start, line.len(), zero_based_line, document_utf16)?,
            ));
        }
        let raw = &line[start..cursor];
        let text = decode_token(raw).map_err(|message| {
            error(
                "FDSL005",
                message,
                span(line, start, cursor, zero_based_line, document_utf16)
                    .unwrap_or_else(|_| start_span()),
            )
        })?;
        tokens.push(make_token(
            text,
            line,
            start,
            cursor,
            zero_based_line,
            document_utf16,
        )?);
        if cursor < line.len() && line[cursor..].starts_with('#') {
            break;
        }
    }
    Ok(tokens)
}

fn decode_token(raw: &str) -> Result<String, String> {
    if let Some((key, value)) = raw.split_once('=') {
        if key.is_empty() || value.is_empty() {
            return Err("attribute must use key=value with both sides present".to_owned());
        }
        return Ok(format!("{key}={}", decode_atom(value)?));
    }
    decode_atom(raw)
}

fn decode_atom(raw: &str) -> Result<String, String> {
    if raw.starts_with('[') {
        let values = serde_json::from_str::<Vec<String>>(raw)
            .map_err(|_| "list attribute must be a compact JSON string array".to_owned())?;
        return serde_json::to_string(&values)
            .map_err(|_| "list attribute could not be canonicalized".to_owned());
    }
    if raw.starts_with('"') {
        if !raw.ends_with('"') || raw.len() < 2 {
            return Err("quoted string must end at the token boundary".to_owned());
        }
        return serde_json::from_str::<String>(raw)
            .map_err(|_| "quoted string is not valid JSON string syntax".to_owned());
    }
    if raw.contains('"') {
        return Err("quote may only begin an attribute value or token".to_owned());
    }
    Ok(raw.to_owned())
}

fn make_token(
    text: String,
    line: &str,
    start: usize,
    end: usize,
    zero_based_line: usize,
    document_utf16: u32,
) -> Result<Token, FlowDslError> {
    Ok(Token {
        text,
        span: span(line, start, end, zero_based_line, document_utf16)?,
    })
}

fn span(
    line: &str,
    start: usize,
    end: usize,
    zero_based_line: usize,
    document_utf16: u32,
) -> Result<FlowDslSpan, FlowDslError> {
    let before = u32_utf16_len(&line[..start])?;
    let width = u32_utf16_len(&line[start..end])?;
    let line_number = u32::try_from(zero_based_line)
        .ok()
        .and_then(|value| value.checked_add(1))
        .ok_or_else(|| error("FDSL003", "Flow DSL line number overflow", start_span()))?;
    Ok(FlowDslSpan {
        start_utf16: document_utf16
            .checked_add(before)
            .ok_or_else(|| error("FDSL003", "Flow DSL UTF-16 offset overflow", start_span()))?,
        end_utf16: document_utf16
            .checked_add(before)
            .and_then(|value| value.checked_add(width))
            .ok_or_else(|| error("FDSL003", "Flow DSL UTF-16 offset overflow", start_span()))?,
        line: line_number,
        column_utf16: before
            .checked_add(1)
            .ok_or_else(|| error("FDSL003", "Flow DSL column overflow", start_span()))?,
    })
}

fn u32_utf16_len(value: &str) -> Result<u32, FlowDslError> {
    u32::try_from(value.encode_utf16().count())
        .map_err(|_| error("FDSL003", "Flow DSL UTF-16 offset overflow", start_span()))
}

fn parse_statement(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    let head = &tokens[0];
    match head.text.as_str() {
        "model" => parse_model(builder, tokens),
        "node" => parse_node(builder, tokens),
        "edge" => parse_edge_statement(&mut builder.edges, tokens, 1),
        "parametric-capacity" => parse_parametric_capacity(builder, tokens),
        "algorithm" => parse_single_string(&mut builder.algorithm, tokens, "algorithm"),
        "profile" => parse_profile(builder, tokens),
        "granularity" => parse_granularity(builder, tokens),
        "seed" => parse_seed(builder, tokens),
        "update" => parse_update(builder, tokens),
        _ => Err(error(
            "FDSL100",
            format!("unknown statement '{}'", head.text),
            head.span,
        )),
    }
}

fn parse_model(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    if builder.model.is_some() {
        return Err(error(
            "FDSL101",
            "model may be declared only once",
            tokens[0].span,
        ));
    }
    let kind = required_token(tokens, 1, "model kind")?;
    let attributes = attributes(tokens, 2)?;
    let model = match kind.text.as_str() {
        "max-flow" => FlowProblemModelV1::MaxFlow {
            source: take_required(&attributes, "source", kind.span)?,
            sink: take_required(&attributes, "sink", kind.span)?,
        },
        "parametric-max-flow" => FlowProblemModelV1::ParametricMaxFlow {
            source: take_required(&attributes, "source", kind.span)?,
            sink: take_required(&attributes, "sink", kind.span)?,
            parameter: FlowParametricRangeV1 {
                minimum: take_required_rational(&attributes, "lambda-min", kind.span)?,
                maximum: take_required_rational(&attributes, "lambda-max", kind.span)?,
            },
            capacity_slopes: Vec::new(),
        },
        "fixed-flow-min-cost" => FlowProblemModelV1::FixedFlowMinCost {
            source: take_required(&attributes, "source", kind.span)?,
            sink: take_required(&attributes, "sink", kind.span)?,
            required_flow: take_required(&attributes, "required-flow", kind.span)?,
        },
        "min-cost-max-flow" => FlowProblemModelV1::MinCostMaxFlow {
            source: take_required(&attributes, "source", kind.span)?,
            sink: take_required(&attributes, "sink", kind.span)?,
        },
        "circulation" => {
            require_empty_attributes(&attributes)?;
            FlowProblemModelV1::Circulation {}
        }
        "transshipment" => {
            require_empty_attributes(&attributes)?;
            FlowProblemModelV1::Transshipment {}
        }
        "bipartite-matching" => {
            let left = take_string_list(&attributes, "left", kind.span)?;
            let right = take_string_list(&attributes, "right", kind.span)?;
            let adapter_source = attributes
                .get("adapter-source")
                .map(|value| value.0.clone());
            let adapter_sink = attributes.get("adapter-sink").map(|value| value.0.clone());
            let flow_adapter = match (adapter_source, adapter_sink) {
                (Some(source), Some(sink)) => Some(FlowBipartiteAdapterV1 { source, sink }),
                (None, None) => None,
                _ => {
                    return Err(error(
                        "FDSL103",
                        "matching adapter-source and adapter-sink must be declared together",
                        kind.span,
                    ));
                }
            };
            FlowProblemModelV1::BipartiteMatching {
                left,
                right,
                flow_adapter,
            }
        }
        "assignment" => {
            let agents = take_string_list(&attributes, "agents", kind.span)?;
            let tasks = take_string_list(&attributes, "tasks", kind.span)?;
            let objective = match take_required(&attributes, "objective", kind.span)?.as_str() {
                "minimize" => AssignmentObjectiveV1::Minimize,
                "maximize" => AssignmentObjectiveV1::Maximize,
                _ => {
                    return Err(error(
                        "FDSL103",
                        "assignment objective must be minimize or maximize",
                        kind.span,
                    ));
                }
            };
            FlowProblemModelV1::Assignment {
                agents,
                tasks,
                objective,
            }
        }
        "transportation" => FlowProblemModelV1::Transportation {
            origins: take_string_list(&attributes, "origins", kind.span)?,
            destinations: take_string_list(&attributes, "destinations", kind.span)?,
        },
        "convex-cost-flow" => FlowProblemModelV1::ConvexCostFlow {},
        _ => {
            return Err(error(
                "FDSL102",
                format!("unsupported model '{}'", kind.text),
                kind.span,
            ));
        }
    };
    reject_unused_attributes(&attributes, model_attribute_names(&model))?;
    builder.model = Some((model, kind.span));
    Ok(())
}

fn model_attribute_names(model: &FlowProblemModelV1) -> &'static [&'static str] {
    match model {
        FlowProblemModelV1::MaxFlow { .. } | FlowProblemModelV1::MinCostMaxFlow { .. } => {
            &["source", "sink"]
        }
        FlowProblemModelV1::ParametricMaxFlow { .. } => {
            &["source", "sink", "lambda-min", "lambda-max"]
        }
        FlowProblemModelV1::FixedFlowMinCost { .. } => &["source", "sink", "required-flow"],
        FlowProblemModelV1::Circulation {}
        | FlowProblemModelV1::Transshipment {}
        | FlowProblemModelV1::ConvexCostFlow {} => &[],
        FlowProblemModelV1::BipartiteMatching { .. } => {
            &["left", "right", "adapter-source", "adapter-sink"]
        }
        FlowProblemModelV1::Assignment { .. } => &["agents", "tasks", "objective"],
        FlowProblemModelV1::Transportation { .. } => &["origins", "destinations"],
        FlowProblemModelV1::PlanarMaxFlow { .. } => &["source", "sink"],
    }
}

fn parse_node(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    let id = required_token(tokens, 1, "node ID")?;
    let attributes = attributes(tokens, 2)?;
    reject_unused_attributes(&attributes, &["supply", "x", "y"])?;
    let position = match (attributes.get("x"), attributes.get("y")) {
        (Some(x), Some(y)) => Some(FlowPositionV1 {
            x: x.0.clone(),
            y: y.0.clone(),
        }),
        (None, None) => None,
        (Some(value), None) | (None, Some(value)) => {
            return Err(error(
                "FDSL103",
                "node position requires both x and y",
                value.1,
            ));
        }
    };
    builder.nodes.push(FlowNodeV1 {
        id: id.text.clone(),
        supply: attributes
            .get("supply")
            .map_or_else(|| "0".to_owned(), |value| value.0.clone()),
        position,
    });
    Ok(())
}

fn parse_edge_statement(
    output: &mut Vec<FlowEdgeV1>,
    tokens: &[Token],
    offset: usize,
) -> Result<(), FlowDslError> {
    let id = required_token(tokens, offset, "edge ID")?;
    let from = required_token(tokens, offset + 1, "edge tail")?;
    let arrow = required_token(tokens, offset + 2, "'->'")?;
    if arrow.text != "->" {
        return Err(error("FDSL104", "expected '->'", arrow.span));
    }
    let to = required_token(tokens, offset + 3, "edge head")?;
    let attributes = attributes(tokens, offset + 4)?;
    reject_unused_attributes(&attributes, &["lower", "capacity", "cost", "initial-flow"])?;
    output.push(FlowEdgeV1 {
        id: id.text.clone(),
        from: from.text.clone(),
        to: to.text.clone(),
        lower: attributes
            .get("lower")
            .map_or_else(|| "0".to_owned(), |value| value.0.clone()),
        capacity: take_required(&attributes, "capacity", id.span)?,
        cost: attributes
            .get("cost")
            .map_or_else(|| "0".to_owned(), |value| value.0.clone()),
        convex_cost: None,
        initial_flow: attributes.get("initial-flow").map(|value| value.0.clone()),
    });
    Ok(())
}

fn parse_parametric_capacity(
    builder: &mut DocumentBuilder,
    tokens: &[Token],
) -> Result<(), FlowDslError> {
    let values = attributes(tokens, 1)?;
    reject_unused_attributes(&values, &["edge", "slope"])?;
    let span = tokens[0].span;
    builder.parametric_capacities.push((
        FlowParametricCapacitySlopeV1 {
            edge_id: take_required(&values, "edge", span)?,
            slope: take_required(&values, "slope", span)?,
        },
        span,
    ));
    Ok(())
}

fn parse_single_string(
    slot: &mut Option<(String, FlowDslSpan)>,
    tokens: &[Token],
    name: &'static str,
) -> Result<(), FlowDslError> {
    if slot.is_some() {
        return Err(error(
            "FDSL105",
            format!("{name} may be declared only once"),
            tokens[0].span,
        ));
    }
    let value = required_token(tokens, 1, name)?;
    require_token_count(tokens, 2)?;
    *slot = Some((value.text.clone(), value.span));
    Ok(())
}

fn parse_profile(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    let mut slot = None;
    parse_single_string(&mut slot, tokens, "profile")?;
    if builder.profile.is_some() {
        return Err(error(
            "FDSL105",
            "profile may be declared only once",
            tokens[0].span,
        ));
    }
    let (value, span) = slot.expect("single string parser assigns a value");
    let profile = match value.as_str() {
        "trace" => RunProfileV1::Trace,
        "fast" => RunProfileV1::Fast,
        "cpu-parallel" => RunProfileV1::CpuParallel,
        _ => return Err(error("FDSL106", "unsupported run profile", span)),
    };
    builder.profile = Some((profile, span));
    Ok(())
}

fn parse_granularity(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    let mut slot = None;
    parse_single_string(&mut slot, tokens, "granularity")?;
    if builder.granularity.is_some() {
        return Err(error(
            "FDSL105",
            "granularity may be declared only once",
            tokens[0].span,
        ));
    }
    let (value, span) = slot.expect("single string parser assigns a value");
    let granularity = match value.as_str() {
        "phase" => TraceGranularityV1::Phase,
        "operation" => TraceGranularityV1::Operation,
        "micro" => TraceGranularityV1::Micro,
        _ => return Err(error("FDSL107", "unsupported trace granularity", span)),
    };
    builder.granularity = Some((granularity, span));
    Ok(())
}

fn parse_seed(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    parse_single_string(&mut builder.seed, tokens, "seed")
}

fn parse_update(builder: &mut DocumentBuilder, tokens: &[Token]) -> Result<(), FlowDslError> {
    let kind = required_token(tokens, 1, "update kind")?;
    match kind.text.as_str() {
        "set-capacity" => {
            let values = attributes(tokens, 2)?;
            reject_unused_attributes(&values, &["edge", "capacity"])?;
            builder.updates.push(FlowUpdateV1::SetCapacity {
                edge: take_required(&values, "edge", kind.span)?,
                capacity: take_required(&values, "capacity", kind.span)?,
            });
        }
        "remove-edge" => {
            let values = attributes(tokens, 2)?;
            reject_unused_attributes(&values, &["edge"])?;
            builder.updates.push(FlowUpdateV1::RemoveEdge {
                edge: take_required(&values, "edge", kind.span)?,
            });
        }
        "set-terminals" => {
            let values = attributes(tokens, 2)?;
            reject_unused_attributes(&values, &["source", "sink"])?;
            builder.updates.push(FlowUpdateV1::SetTerminals {
                source: take_required(&values, "source", kind.span)?,
                sink: take_required(&values, "sink", kind.span)?,
            });
        }
        "add-edge" => {
            let mut edges = Vec::new();
            parse_edge_statement(&mut edges, tokens, 2)?;
            builder.updates.push(FlowUpdateV1::AddEdge {
                edge: edges.pop().expect("edge parser emits exactly one edge"),
            });
        }
        _ => {
            return Err(error(
                "FDSL108",
                format!("unsupported update '{}'", kind.text),
                kind.span,
            ));
        }
    }
    Ok(())
}

fn attributes(
    tokens: &[Token],
    start: usize,
) -> Result<BTreeMap<String, (String, FlowDslSpan)>, FlowDslError> {
    let mut result = BTreeMap::new();
    for token in tokens.iter().skip(start) {
        let Some((key, value)) = token.text.split_once('=') else {
            return Err(error("FDSL109", "expected key=value attribute", token.span));
        };
        if result
            .insert(key.to_owned(), (value.to_owned(), token.span))
            .is_some()
        {
            return Err(error(
                "FDSL110",
                format!("duplicate attribute '{key}'"),
                token.span,
            ));
        }
    }
    Ok(result)
}

fn reject_unused_attributes(
    attributes: &BTreeMap<String, (String, FlowDslSpan)>,
    allowed: &[&str],
) -> Result<(), FlowDslError> {
    let allowed = allowed.iter().copied().collect::<BTreeSet<_>>();
    if let Some((key, value)) = attributes
        .iter()
        .find(|(key, _)| !allowed.contains(key.as_str()))
    {
        return Err(error(
            "FDSL111",
            format!("unknown attribute '{key}'"),
            value.1,
        ));
    }
    Ok(())
}

fn require_empty_attributes(
    attributes: &BTreeMap<String, (String, FlowDslSpan)>,
) -> Result<(), FlowDslError> {
    reject_unused_attributes(attributes, &[])
}

fn take_required(
    attributes: &BTreeMap<String, (String, FlowDslSpan)>,
    name: &'static str,
    fallback_span: FlowDslSpan,
) -> Result<String, FlowDslError> {
    attributes
        .get(name)
        .map(|value| value.0.clone())
        .ok_or_else(|| {
            error(
                "FDSL112",
                format!("missing required attribute '{name}'"),
                fallback_span,
            )
        })
}

fn take_required_rational(
    attributes: &BTreeMap<String, (String, FlowDslSpan)>,
    name: &'static str,
    fallback_span: FlowDslSpan,
) -> Result<FlowRationalV1, FlowDslError> {
    let value = take_required(attributes, name, fallback_span)?;
    let Some((numerator, denominator)) = value.split_once('/') else {
        return Err(error(
            "FDSL103",
            format!("attribute '{name}' must use exact numerator/denominator syntax"),
            attributes.get(name).map_or(fallback_span, |item| item.1),
        ));
    };
    if denominator.contains('/') {
        return Err(error(
            "FDSL103",
            format!("attribute '{name}' must contain exactly one '/'"),
            attributes.get(name).map_or(fallback_span, |item| item.1),
        ));
    }
    Ok(FlowRationalV1 {
        numerator: numerator.to_owned(),
        denominator: denominator.to_owned(),
    })
}

fn take_string_list(
    attributes: &BTreeMap<String, (String, FlowDslSpan)>,
    name: &'static str,
    fallback_span: FlowDslSpan,
) -> Result<Vec<String>, FlowDslError> {
    let value = take_required(attributes, name, fallback_span)?;
    serde_json::from_str::<Vec<String>>(&value).map_err(|_| {
        error(
            "FDSL103",
            format!("attribute '{name}' must be a compact JSON string array"),
            attributes.get(name).map_or(fallback_span, |item| item.1),
        )
    })
}

fn required_token<'a>(
    tokens: &'a [Token],
    index: usize,
    name: &'static str,
) -> Result<&'a Token, FlowDslError> {
    tokens.get(index).ok_or_else(|| {
        error(
            "FDSL113",
            format!("missing {name}"),
            tokens.last().map_or_else(start_span, |token| token.span),
        )
    })
}

fn require_token_count(tokens: &[Token], count: usize) -> Result<(), FlowDslError> {
    if let Some(extra) = tokens.get(count) {
        return Err(error("FDSL114", "unexpected trailing token", extra.span));
    }
    Ok(())
}

fn finish(builder: DocumentBuilder) -> Result<FlowScenarioV1, FlowDslError> {
    let (mut model, model_span) = builder.model.ok_or_else(|| {
        error(
            "FDSL115",
            "document requires exactly one model statement",
            start_span(),
        )
    })?;
    if let FlowProblemModelV1::ParametricMaxFlow {
        capacity_slopes, ..
    } = &mut model
    {
        *capacity_slopes = builder
            .parametric_capacities
            .iter()
            .map(|(coefficient, _)| coefficient.clone())
            .collect();
    } else if let Some((_, span)) = builder.parametric_capacities.first() {
        return Err(error(
            "FDSL116",
            "parametric-capacity requires a parametric-max-flow model",
            *span,
        ));
    }
    let algorithm = builder
        .algorithm
        .map_or_else(|| default_algorithm(&model).to_owned(), |value| value.0);
    let scenario = FlowScenarioV1 {
        schema_version: 1,
        scenario_encoding_revision: "rfc8785-jcs/1".to_owned(),
        plugin: "flow".to_owned(),
        reproducibility: ReproducibilityMetadata {
            declared: ReproducibilityRevisionSet {
                algorithm_revision: ALGORITHM_REVISION.to_owned(),
                rng_version: RNG_VERSION,
                plugin_result_revision: PLUGIN_RESULT_REVISION.to_owned(),
                metrics_catalog_revision: METRICS_CATALOG_REVISION.to_owned(),
                trace_revision: TRACE_REVISION.to_owned(),
                projection_revision: PROJECTION_REVISION.to_owned(),
                layout_revision: LAYOUT_REVISION.to_owned(),
                frame_encoding_revision: FRAME_ENCODING_REVISION.to_owned(),
            },
        },
        payload: FlowScenarioPayloadV1 {
            model,
            graph: FlowGraphV1 {
                nodes: builder.nodes,
                edges: builder.edges,
            },
            algorithm: FlowAlgorithmSelectionV1 {
                id: algorithm,
                config: BTreeMap::new(),
            },
            run_profile: builder.profile.map_or(RunProfileV1::Trace, |value| value.0),
            trace_granularity: builder
                .granularity
                .map_or(TraceGranularityV1::Operation, |value| value.0),
            algorithm_seed: builder.seed.map_or_else(|| "0".to_owned(), |value| value.0),
            updates: (!builder.updates.is_empty()).then_some(builder.updates),
            generator_provenance: None,
        },
    };
    validate_flow_scenario(&scenario).map_err(|source| {
        error(
            "FDSL200",
            format!("Scenario validation failed: {source}"),
            model_span,
        )
    })?;
    Ok(scenario)
}

fn default_algorithm(model: &FlowProblemModelV1) -> &'static str {
    match model {
        FlowProblemModelV1::MaxFlow { .. } => "edmonds-karp",
        FlowProblemModelV1::ParametricMaxFlow { .. } => "parametric-pseudoflow",
        FlowProblemModelV1::FixedFlowMinCost { .. }
        | FlowProblemModelV1::Circulation {}
        | FlowProblemModelV1::Transshipment {} => "bellman-ford-ssp",
        FlowProblemModelV1::MinCostMaxFlow { .. } => "successive-shortest-augmenting-path",
        FlowProblemModelV1::BipartiteMatching { .. } => "hopcroft-karp",
        FlowProblemModelV1::Assignment { .. } => "hungarian",
        FlowProblemModelV1::Transportation { .. } => "transportation-simplex",
        FlowProblemModelV1::PlanarMaxFlow { .. } => "hassin-st-planar",
        FlowProblemModelV1::ConvexCostFlow {} => "segment-expanded-convex-mcf",
    }
}

fn error(code: &'static str, message: impl Into<String>, span: FlowDslSpan) -> FlowDslError {
    FlowDslError {
        diagnostic: FlowDslDiagnostic {
            code,
            message: message.into(),
            span,
        },
    }
}

const fn start_span() -> FlowDslSpan {
    FlowDslSpan {
        start_utf16: 0,
        end_utf16: 0,
        line: 1,
        column_utf16: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r"
# declarations are order-independent
model max-flow source=s sink=t
node s x=90 y=270
node a
node t
edge e1 s -> a capacity=10 cost=2
edge e2 a -> t lower=1 capacity=7 cost=-1
algorithm edmonds-karp
profile trace
granularity operation
seed 17
";

    #[test]
    fn parses_document_into_the_strict_flow_scenario() {
        let scenario = decode_flow_dsl(EXAMPLE).expect("example DSL is valid");
        let graph = scenario.canonical_network().expect("network validates");

        assert_eq!(scenario.payload.algorithm.id, "edmonds-karp");
        assert_eq!(scenario.payload.algorithm_seed, "17");
        assert_eq!(graph.nodes().len(), 3);
        assert_eq!(graph.edges().len(), 2);
        assert_eq!(graph.edges()[1].lower(), 1);
        assert_eq!(graph.edges()[1].cost(), -1);
    }

    #[test]
    fn parses_exact_parametric_model_and_ordered_capacity_slopes() {
        let source = r"
parametric-capacity edge=sa slope=2
model parametric-max-flow source=s sink=t lambda-min=-1/2 lambda-max=4/1
node a
node s
node t
edge at a -> t capacity=5
edge sa s -> a capacity=3
";
        let scenario = decode_flow_dsl(source).expect("parametric DSL is valid");
        assert_eq!(scenario.payload.algorithm.id, "parametric-pseudoflow");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::ParametricMaxFlow {
                source,
                sink,
                parameter: FlowParametricRangeV1 {
                    minimum: FlowRationalV1 {
                        numerator: minimum_numerator,
                        denominator: minimum_denominator,
                    },
                    maximum: FlowRationalV1 {
                        numerator: maximum_numerator,
                        denominator: maximum_denominator,
                    },
                },
                capacity_slopes,
            } if source == "s"
                && sink == "t"
                && minimum_numerator == "-1"
                && minimum_denominator == "2"
                && maximum_numerator == "4"
                && maximum_denominator == "1"
                && capacity_slopes == [FlowParametricCapacitySlopeV1 {
                    edge_id: "sa".to_owned(),
                    slope: "2".to_owned(),
                }]
        ));
    }

    #[test]
    fn rejects_parametric_capacity_on_other_models_and_noncanonical_rationals() {
        let wrong_model = r"
model max-flow source=s sink=t
node s
node t
edge st s -> t capacity=1
parametric-capacity edge=st slope=1
";
        assert_eq!(
            decode_flow_dsl(wrong_model)
                .expect_err("parametric coefficient requires the matching model")
                .diagnostic
                .code,
            "FDSL116"
        );

        let noncanonical = r"
model parametric-max-flow source=s sink=t lambda-min=0/2 lambda-max=1/1
node a
node s
node t
edge at a -> t capacity=2
edge sa s -> a capacity=1
parametric-capacity edge=sa slope=1
";
        assert_eq!(
            decode_flow_dsl(noncanonical)
                .expect_err("wire rationals must be normalized")
                .diagnostic
                .code,
            "FDSL200"
        );
    }

    #[test]
    fn supports_quoted_unicode_ids_comments_and_utf16_diagnostics() {
        let valid = r#"
model max-flow source="始点 😀" sink=終点
node "始点 😀"
node 終点
edge "辺 1" "始点 😀" -> 終点 capacity=3 # comment
"#;
        let scenario = decode_flow_dsl(valid).expect("quoted IDs are valid");
        assert_eq!(scenario.payload.graph.nodes[0].id, "始点 😀");

        let error = decode_flow_dsl("# 😀\n未知\n").expect_err("unknown statement");
        assert_eq!(error.diagnostic.code, "FDSL100");
        assert_eq!(error.diagnostic.span.line, 2);
        assert_eq!(error.diagnostic.span.start_utf16, 5);
        assert_eq!(error.diagnostic.span.column_utf16, 1);
    }

    #[test]
    fn parses_fixed_flow_balances_positions_initial_flow_and_updates() {
        let source = r"
model fixed-flow-min-cost source=s sink=t required-flow=2
node s supply=1 x=-10 y=20
node t supply=-1 x=10 y=20
edge e s -> t lower=1 capacity=3 cost=-2 initial-flow=1
update set-capacity edge=e capacity=4
update add-edge e2 t -> s capacity=1 cost=3
update remove-edge edge=e2
update set-terminals source=t sink=s
";
        let scenario = decode_flow_dsl(source).expect("extended document is valid");
        assert_eq!(scenario.payload.algorithm.id, "bellman-ford-ssp");
        assert_eq!(scenario.payload.updates.as_ref().map(Vec::len), Some(4));
        assert_eq!(
            scenario.payload.graph.edges[0].initial_flow.as_deref(),
            Some("1")
        );
    }

    #[test]
    fn parses_native_bipartite_matching_with_compact_partition_arrays() {
        let source = r#"
model bipartite-matching left=["l0","l1"] right=["r0","r1"] adapter-source=s adapter-sink=t
node s
node l0
node l1
node r0
node r1
node t
edge a0 s -> l0 capacity=1
edge a1 s -> l1 capacity=1
edge b0 l0 -> r0 capacity=1
edge b1 l0 -> r1 capacity=1
edge b2 l1 -> r0 capacity=1
edge c0 r0 -> t capacity=1
edge c1 r1 -> t capacity=1
"#;
        let scenario = decode_flow_dsl(source).expect("matching DSL is valid");
        assert_eq!(scenario.payload.algorithm.id, "hopcroft-karp");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::BipartiteMatching {
                ref left,
                ref right,
                flow_adapter: Some(ref adapter)
            } if left.iter().map(String::as_str).eq(["l0", "l1"])
                && right.iter().map(String::as_str).eq(["r0", "r1"])
                && adapter.source == "s"
                && adapter.sink == "t"
        ));
    }

    #[test]
    fn parses_rectangular_assignment_with_objective_and_forbidden_pairs() {
        let source = r#"
model assignment agents=["a0","a1"] tasks=["t0","t1","t2"] objective=maximize
node a0
node a1
node t0
node t1
node t2
edge e00 a0 -> t0 capacity=1 cost=-4
edge e01 a0 -> t1 capacity=1 cost=8
edge e10 a1 -> t0 capacity=1 cost=3
edge e12 a1 -> t2 capacity=1 cost=7
"#;
        let scenario = decode_flow_dsl(source).expect("assignment DSL is valid");
        assert_eq!(scenario.payload.algorithm.id, "hungarian");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::Assignment {
                ref agents,
                ref tasks,
                objective: AssignmentObjectiveV1::Maximize,
            } if agents.iter().map(String::as_str).eq(["a0", "a1"])
                && tasks.iter().map(String::as_str).eq(["t0", "t1", "t2"])
        ));
    }

    #[test]
    fn parses_balanced_transportation_with_forbidden_routes() {
        let source = r#"
model transportation origins=["o0","o1"] destinations=["d0","d1"]
node d0 supply=-2
node d1 supply=-3
node o0 supply=3
node o1 supply=2
edge e00 o0 -> d0 capacity=2 cost=4
edge e01 o0 -> d1 capacity=3 cost=1
edge e11 o1 -> d1 capacity=2 cost=3
"#;
        let scenario = decode_flow_dsl(source).expect("transportation DSL is valid");
        assert_eq!(scenario.payload.algorithm.id, "transportation-simplex");
        assert!(matches!(
            scenario.payload.model,
            FlowProblemModelV1::Transportation {
                ref origins,
                ref destinations,
            } if origins.iter().map(String::as_str).eq(["o0", "o1"])
                && destinations.iter().map(String::as_str).eq(["d0", "d1"])
        ));
        assert_eq!(scenario.payload.graph.edges.len(), 3);
    }

    #[test]
    fn rejects_unknown_duplicate_or_incomplete_attributes_without_partial_output() {
        let unknown = "model circulation nope=1\nnode s\n";
        assert_eq!(
            decode_flow_dsl(unknown)
                .expect_err("unknown attribute")
                .diagnostic
                .code,
            "FDSL111"
        );

        let duplicate = "model max-flow source=s source=x sink=t\nnode s\nnode t\n";
        assert_eq!(
            decode_flow_dsl(duplicate)
                .expect_err("duplicate attribute")
                .diagnostic
                .code,
            "FDSL110"
        );

        let incomplete = "model max-flow source=s\nnode s\nnode t\n";
        assert_eq!(
            decode_flow_dsl(incomplete)
                .expect_err("missing sink")
                .diagnostic
                .code,
            "FDSL112"
        );
    }

    #[test]
    fn strict_scenario_validation_rejects_noncanonical_numbers_and_graph_errors() {
        let number = "model max-flow source=s sink=t\nnode s\nnode t\nedge e s -> t capacity=03\n";
        assert_eq!(
            decode_flow_dsl(number)
                .expect_err("noncanonical capacity")
                .diagnostic
                .code,
            "FDSL200"
        );

        let dangling = "model max-flow source=s sink=t\nnode s\nnode t\nedge e s -> x capacity=3\n";
        assert_eq!(
            decode_flow_dsl(dangling)
                .expect_err("dangling endpoint")
                .diagnostic
                .code,
            "FDSL200"
        );
    }

    #[test]
    fn singleton_directives_and_trailing_tokens_are_rejected() {
        let duplicate = format!("{EXAMPLE}\nprofile fast\n");
        assert_eq!(
            decode_flow_dsl(&duplicate)
                .expect_err("duplicate profile")
                .diagnostic
                .code,
            "FDSL105"
        );

        let trailing = "model circulation\nnode a extra\n";
        assert_eq!(
            decode_flow_dsl(trailing)
                .expect_err("bare trailing token")
                .diagnostic
                .code,
            "FDSL109"
        );
    }
}
