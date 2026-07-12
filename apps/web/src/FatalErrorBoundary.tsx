import { Component, type ErrorInfo, type ReactNode } from "react";

type FatalErrorBoundaryProps = {
	children: ReactNode;
};

type FatalErrorBoundaryState = {
	error?: Error;
};

export class FatalErrorBoundary extends Component<
	FatalErrorBoundaryProps,
	FatalErrorBoundaryState
> {
	state: FatalErrorBoundaryState = {};

	static getDerivedStateFromError(error: Error): FatalErrorBoundaryState {
		return { error };
	}

	componentDidCatch(_error: Error, _errorInfo: ErrorInfo) {
		// React reports the captured exception to the browser console. The UI keeps
		// a deterministic recovery surface instead of leaving an empty document.
	}

	render() {
		const { error } = this.state;
		if (error === undefined) {
			return this.props.children;
		}
		return (
			<main className="fatal-error" role="alert">
				<p className="eyebrow">VISUALIZER STOPPED</p>
				<h1>The interface could not continue safely.</h1>
				<p>{error.message}</p>
				<button
					type="button"
					className="primary-button"
					onClick={() => window.location.reload()}
				>
					Reload visualizer
				</button>
			</main>
		);
	}
}
