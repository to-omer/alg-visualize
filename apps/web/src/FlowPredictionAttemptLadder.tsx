import type { FlowPredictionAssistedEpsilonOverlayV1 } from "./flow-scene";

export function FlowPredictionAttemptLadder({
	overlay,
}: Readonly<{ overlay: FlowPredictionAssistedEpsilonOverlayV1 }>) {
	const maximum = Number(BigInt(overlay.maximum_attempt));
	const attempt = Number(BigInt(overlay.attempt));
	const availableWidth = 836;
	const gap = maximum > 64 ? 1 : 2;
	const segmentWidth = Math.max(
		3,
		Math.min(18, (availableWidth - gap * Math.max(0, maximum - 1)) / maximum),
	);
	return (
		<g
			className="flow-prediction-attempt-ladder"
			transform="translate(62 17)"
			data-prediction-attempt-ladder={overlay.stage}
		>
			<title>{`Remark 1 exponent ladder · attempt ${overlay.attempt}/${overlay.maximum_attempt} · T ${overlay.exponent}${overlay.scale_exponent === undefined ? "" : ` · t ${overlay.scale_exponent}`}`}</title>
			<text className="flow-prediction-ladder-label" x="0" y="-5">
				{`T ${overlay.attempt}/${overlay.maximum_attempt}${overlay.scale_exponent === undefined ? "" : ` · t ${overlay.scale_exponent}`}`}
			</text>
			{Array.from({ length: maximum }, (_, index) => {
				const ordinal = index + 1;
				const state =
					ordinal < attempt ||
					(ordinal === attempt && overlay.stage === "abort-attempt")
						? "rejected"
						: ordinal === attempt
							? overlay.stage === "optimal"
								? "success"
								: "active"
							: "future";
				const x = index * (segmentWidth + gap);
				return (
					<g
						key={ordinal}
						className={`flow-prediction-attempt flow-prediction-attempt-${state}`}
						data-prediction-exponent-candidate={ordinal}
						data-prediction-attempt-state={state}
						transform={`translate(${x} 0)`}
					>
						<title>{`T=${ordinal} · ${state}`}</title>
						<rect width={segmentWidth} height="8" rx="2" />
						{state === "rejected" && segmentWidth >= 7 && (
							<path
								d={`M1,1 L${segmentWidth - 1},7 M${segmentWidth - 1},1 L1,7`}
							/>
						)}
					</g>
				);
			})}
		</g>
	);
}
