import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";

import { canonicalize } from "./jcs.ts";

const fixtureUrl = new URL(
	"../../fixtures/contracts/jcs-cross-language.json",
	import.meta.url,
);
const fixture = JSON.parse(await readFile(fixtureUrl, "utf8"));
const input = JSON.parse(fixture.input);
const actual = canonicalize(input);

assert.equal(actual, fixture.canonical);
assert.equal(createHash("sha256").update(actual).digest("hex"), fixture.sha256);
assert.throws(() => canonicalize(9_007_199_254_740_992), /safe range/);

console.log("TypeScript JCS fixture verified");
