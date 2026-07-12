export type TraversalGradientSegment = {
	alpha: number;
	color: number;
	end: number;
	start: number;
};

const SOURCE_COLOR = 0x53c7c5;
const TARGET_COLOR = 0xffc36b;
const SEGMENT_COUNT = 16;
const TAIL_LENGTH = 0.42;

function mixColor(from: number, to: number, amount: number): number {
	const channel = (shift: number) =>
		Math.round(
			((from >> shift) & 0xff) * (1 - amount) + ((to >> shift) & 0xff) * amount,
		);
	return (channel(16) << 16) | (channel(8) << 8) | channel(0);
}

export function traversalGradientSegments(
	progress: number,
): readonly TraversalGradientSegment[] {
	if (progress <= 0 || progress >= 1) return [];
	const head = progress * (1 + TAIL_LENGTH);
	const output: TraversalGradientSegment[] = [];
	for (let index = 0; index < SEGMENT_COUNT; index += 1) {
		const start = index / SEGMENT_COUNT;
		const end = (index + 1) / SEGMENT_COUNT;
		const center = (start + end) / 2;
		const distance = head - center;
		if (distance < 0 || distance > TAIL_LENGTH) continue;
		const intensity = 1 - distance / TAIL_LENGTH;
		output.push({
			alpha: 0.35 + intensity * 0.65,
			color: mixColor(SOURCE_COLOR, TARGET_COLOR, intensity),
			end,
			start,
		});
	}
	return output;
}
