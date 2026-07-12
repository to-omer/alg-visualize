export function fitsUtf8Budget(
	values: readonly string[],
	maxBytes: number,
): boolean {
	if (!Number.isSafeInteger(maxBytes) || maxBytes < 0) return false;
	let bytes = 0;
	for (const value of values) {
		for (let index = 0; index < value.length; index += 1) {
			const code = value.charCodeAt(index);
			if (code <= 0x7f) bytes += 1;
			else if (code <= 0x7ff) bytes += 2;
			else if (
				code >= 0xd800 &&
				code <= 0xdbff &&
				index + 1 < value.length &&
				value.charCodeAt(index + 1) >= 0xdc00 &&
				value.charCodeAt(index + 1) <= 0xdfff
			) {
				bytes += 4;
				index += 1;
			} else bytes += 3;
			if (bytes > maxBytes) return false;
		}
	}
	return true;
}
