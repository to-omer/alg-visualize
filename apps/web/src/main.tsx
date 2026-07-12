import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { FatalErrorBoundary } from "./FatalErrorBoundary";
import "./styles.css";
import "./styles-compact.css";

const root = document.getElementById("root");
if (root === null) {
	throw new Error("root element is missing");
}

createRoot(root).render(
	<StrictMode>
		<FatalErrorBoundary>
			<App />
		</FatalErrorBoundary>
	</StrictMode>,
);
