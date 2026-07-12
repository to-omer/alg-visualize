import {
	test as base,
	type ConsoleMessage,
	expect,
	type Page,
} from "@playwright/test";

type RuntimeFault = {
	message: string;
	source: "console.error" | "page.crash" | "page.error";
};

function observeRuntimeFaults(page: Page, faults: RuntimeFault[]): () => void {
	const onConsole = (message: ConsoleMessage) => {
		if (message.type() === "error") {
			faults.push({ source: "console.error", message: message.text() });
		}
	};
	const onCrash = () => {
		faults.push({ source: "page.crash", message: "browser page crashed" });
	};
	const onPageError = (error: Error) => {
		faults.push({ source: "page.error", message: error.message });
	};
	page.on("console", onConsole);
	page.on("crash", onCrash);
	page.on("pageerror", onPageError);
	return () => {
		page.off("console", onConsole);
		page.off("crash", onCrash);
		page.off("pageerror", onPageError);
	};
}

export const test = base.extend<{ browserRuntimeContract: undefined }>({
	browserRuntimeContract: [
		async ({ page }, use) => {
			const faults: RuntimeFault[] = [];
			const stopObserving = observeRuntimeFaults(page, faults);
			await use(undefined);
			stopObserving();
			expect(
				faults,
				"the browser runtime must not emit uncaught errors, console errors, or page crashes",
			).toEqual([]);
		},
		{ auto: true },
	],
});

export { expect };
