export type JsonValue =
	| null
	| boolean
	| number
	| string
	| JsonValue[]
	| { [key: string]: JsonValue };

const MAX_SAFE_INTEGER = 9_007_199_254_740_991;

/** Canonicalizes an already duplicate-checked I-JSON value using RFC 8785. */
export function canonicalize(value: JsonValue): string {
	if (value === null || typeof value === "boolean") {
		return JSON.stringify(value);
	}
	if (typeof value === "string") {
		return JSON.stringify(value);
	}
	if (typeof value === "number") {
		if (
			!Number.isFinite(value) ||
			(Number.isInteger(value) && Math.abs(value) > MAX_SAFE_INTEGER)
		) {
			throw new TypeError("number is outside the I-JSON safe range");
		}
		return JSON.stringify(value);
	}
	if (Array.isArray(value)) {
		return `[${value.map(canonicalize).join(",")}]`;
	}
	const properties = Object.keys(value)
		.sort()
		.map((key) => {
			const property = value[key];
			if (property === undefined) {
				throw new TypeError("undefined is not a JSON value");
			}
			return `${JSON.stringify(key)}:${canonicalize(property)}`;
		});
	return `{${properties.join(",")}}`;
}
