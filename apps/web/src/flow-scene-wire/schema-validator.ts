import { FLOW_SCENE_V9_SCHEMA } from "./generated/schema";

type JsonSchema = Readonly<{
	$ref?: string;
	type?: string | readonly string[];
	const?: unknown;
	enum?: readonly unknown[];
	anyOf?: readonly JsonSchema[];
	oneOf?: readonly JsonSchema[];
	properties?: Readonly<Record<string, JsonSchema>>;
	required?: readonly string[];
	additionalProperties?: boolean | JsonSchema;
	items?: JsonSchema;
	minItems?: number;
	maxItems?: number;
	minimum?: number;
	maximum?: number;
}>;

type RootSchema = JsonSchema & {
	$defs: Readonly<Record<string, JsonSchema>>;
};

const schema = FLOW_SCENE_V9_SCHEMA as unknown as RootSchema;
const maximumSchemaDepth = 128;

function isRecord(value: unknown): value is Record<string, unknown> {
	return typeof value === "object" && value !== null && !Array.isArray(value);
}

function sameJsonScalar(left: unknown, right: unknown): boolean {
	return (
		left === right ||
		(typeof left === "number" &&
			typeof right === "number" &&
			Number.isNaN(left) &&
			Number.isNaN(right))
	);
}

function referencedSchema(reference: string): JsonSchema {
	const prefix = "#/$defs/";
	if (!reference.startsWith(prefix)) {
		throw new Error(`Unsupported flow scene schema reference: ${reference}`);
	}
	const definition = reference.slice(prefix.length);
	const resolved = schema.$defs[definition];
	if (resolved === undefined) {
		throw new Error(`Unknown flow scene schema definition: ${definition}`);
	}
	return resolved;
}

function matches(
	value: unknown,
	candidate: JsonSchema,
	depth: number,
): boolean {
	try {
		assertSchema(value, candidate, "$candidate", depth);
		return true;
	} catch {
		return false;
	}
}

function assertTypedValue(value: unknown, type: string, path: string): void {
	const valid =
		type === "null"
			? value === null
			: type === "object"
				? isRecord(value)
				: type === "array"
					? Array.isArray(value)
					: type === "integer"
						? typeof value === "number" && Number.isSafeInteger(value)
						: type === "number"
							? typeof value === "number" && Number.isFinite(value)
							: typeof value === type;
	if (!valid) {
		throw new Error(
			`Flow scene structure mismatch at ${path}: expected ${type}`,
		);
	}
}

function assertSchema(
	value: unknown,
	candidate: JsonSchema,
	path: string,
	depth = 0,
): void {
	if (depth > maximumSchemaDepth) {
		throw new Error("Flow scene schema nesting limit exceeded");
	}
	if (candidate.$ref !== undefined) {
		assertSchema(value, referencedSchema(candidate.$ref), path, depth + 1);
		return;
	}
	if (
		candidate.const !== undefined &&
		!sameJsonScalar(value, candidate.const)
	) {
		throw new Error(`Flow scene structure mismatch at ${path}: wrong constant`);
	}
	if (
		candidate.enum !== undefined &&
		!candidate.enum.some((item) => sameJsonScalar(value, item))
	) {
		throw new Error(
			`Flow scene structure mismatch at ${path}: unknown enum value`,
		);
	}
	if (
		candidate.anyOf !== undefined &&
		!candidate.anyOf.some((item) => matches(value, item, depth + 1))
	) {
		throw new Error(
			`Flow scene structure mismatch at ${path}: no union member matched`,
		);
	}
	if (candidate.oneOf !== undefined) {
		const matchesCount = candidate.oneOf.filter((item) =>
			matches(value, item, depth + 1),
		).length;
		if (matchesCount !== 1) {
			throw new Error(
				`Flow scene structure mismatch at ${path}: expected one union member`,
			);
		}
	}
	if (candidate.type !== undefined) {
		const types = Array.isArray(candidate.type)
			? candidate.type
			: [candidate.type];
		if (
			!types.some((type) => {
				try {
					assertTypedValue(value, type, path);
					return true;
				} catch {
					return false;
				}
			})
		) {
			throw new Error(
				`Flow scene structure mismatch at ${path}: expected ${types.join(" or ")}`,
			);
		}
	}
	if (isRecord(value) && candidate.properties !== undefined) {
		for (const required of candidate.required ?? []) {
			if (!Object.hasOwn(value, required)) {
				throw new Error(
					`Flow scene structure mismatch at ${path}: missing ${required}`,
				);
			}
		}
		for (const [key, child] of Object.entries(value)) {
			const childSchema = candidate.properties[key];
			if (childSchema !== undefined) {
				assertSchema(child, childSchema, `${path}.${key}`, depth + 1);
			} else if (candidate.additionalProperties === false) {
				throw new Error(
					`Flow scene structure mismatch at ${path}: unknown field ${key}`,
				);
			} else if (isRecord(candidate.additionalProperties)) {
				assertSchema(
					child,
					candidate.additionalProperties,
					`${path}.${key}`,
					depth + 1,
				);
			}
		}
	}
	if (Array.isArray(value)) {
		if (candidate.minItems !== undefined && value.length < candidate.minItems) {
			throw new Error(
				`Flow scene structure mismatch at ${path}: array is too short`,
			);
		}
		if (candidate.maxItems !== undefined && value.length > candidate.maxItems) {
			throw new Error(
				`Flow scene structure mismatch at ${path}: array is too long`,
			);
		}
		if (candidate.items !== undefined) {
			for (const [index, item] of value.entries()) {
				assertSchema(item, candidate.items, `${path}[${index}]`, depth + 1);
			}
		}
	}
	if (typeof value === "number") {
		if (candidate.minimum !== undefined && value < candidate.minimum) {
			throw new Error(
				`Flow scene structure mismatch at ${path}: below minimum`,
			);
		}
		if (candidate.maximum !== undefined && value > candidate.maximum) {
			throw new Error(
				`Flow scene structure mismatch at ${path}: above maximum`,
			);
		}
	}
}

export function assertFlowSceneV9Root(value: unknown): void {
	assertSchema(value, schema, "$scene");
}

export function assertFlowSceneDefinition(
	value: unknown,
	definition: string,
	context: string,
): void {
	const candidate = schema.$defs[definition];
	if (candidate === undefined) {
		throw new Error(`Unknown flow scene definition: ${definition}`);
	}
	assertSchema(value, candidate, `$scene.${context}`);
}
