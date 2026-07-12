import {
	autocompletion,
	type CompletionContext,
} from "@codemirror/autocomplete";
import { defaultKeymap } from "@codemirror/commands";
import { json, jsonParseLinter } from "@codemirror/lang-json";
import { linter, lintGutter, setDiagnostics } from "@codemirror/lint";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { useEffect, useRef } from "react";

import type { InputDiagnostic } from "./engine-types";

type EditorPanelProps = {
	ariaLabel?: string;
	diagnostic?: InputDiagnostic | undefined;
	language?: "dsl" | "json";
	onChange: (value: string) => void;
	value: string;
};

const MAX_LINT_DOCUMENT_LENGTH = 256 * 1024;
const parseJson = jsonParseLinter();

function completeOperation(context: CompletionContext) {
	const word = context.matchBefore(/[a-z_-]*/);
	if (word === null || (word.from === word.to && !context.explicit)) {
		return null;
	}
	return {
		from: word.from,
		options: [
			"insert",
			"remove",
			"get",
			"lower_bound",
			"avl",
			"wbt",
			"aa",
			"llrb",
			"treap",
			"zip",
			"splay",
			"scapegoat",
			"skip-list",
			"b-tree",
			"veb",
			"x-fast",
			"y-fast",
		].map((label) => ({ label, type: "keyword" })),
	};
}

export function EditorPanel({
	ariaLabel = "Scenario editor",
	diagnostic,
	language = "json",
	onChange,
	value,
}: EditorPanelProps) {
	const editorHost = useRef<HTMLDivElement>(null);
	const initialValue = useRef(value);
	const onChangeRef = useRef(onChange);
	const viewRef = useRef<EditorView | undefined>(undefined);
	const applyingExternalValue = useRef(false);
	onChangeRef.current = onChange;

	useEffect(() => {
		if (editorHost.current === null) {
			return;
		}
		const state = EditorState.create({
			doc: initialValue.current,
			extensions: [
				...(language === "json"
					? [
							json(),
							linter((view) =>
								view.state.doc.length > MAX_LINT_DOCUMENT_LENGTH
									? []
									: parseJson(view),
							),
							lintGutter(),
						]
					: []),
				keymap.of(defaultKeymap),
				autocompletion({ override: [completeOperation] }),
				EditorView.lineWrapping,
				EditorView.updateListener.of((update) => {
					if (update.docChanged && !applyingExternalValue.current) {
						onChangeRef.current(update.state.doc.toString());
					}
				}),
				EditorView.theme({
					"&": { height: "100%", backgroundColor: "transparent" },
					".cm-content": { fontFamily: "var(--font-mono)", padding: "14px 0" },
					".cm-gutters": { backgroundColor: "transparent", border: "none" },
					".cm-activeLine, .cm-activeLineGutter": {
						backgroundColor: "#ffffff08",
					},
					"&.cm-focused": { outline: "none" },
					".cm-cursor": { borderLeftColor: "#e9a23b" },
				}),
			],
		});
		const view = new EditorView({ state, parent: editorHost.current });
		view.contentDOM.setAttribute("aria-label", ariaLabel);
		viewRef.current = view;
		return () => {
			viewRef.current = undefined;
			view.destroy();
		};
	}, [ariaLabel, language]);

	useEffect(() => {
		const view = viewRef.current;
		if (view === undefined || view.state.doc.toString() === value) {
			return;
		}
		applyingExternalValue.current = true;
		view.dispatch({
			changes: { from: 0, to: view.state.doc.length, insert: value },
		});
		applyingExternalValue.current = false;
	}, [value]);

	useEffect(() => {
		const view = viewRef.current;
		if (view === undefined) {
			return;
		}
		if (diagnostic === undefined) {
			view.dispatch(setDiagnostics(view.state, []));
			return;
		}
		const lineNumber = Math.min(diagnostic.line, view.state.doc.lines);
		const line = view.state.doc.line(lineNumber);
		const from = Math.min(line.to, line.from + diagnostic.column - 1);
		view.dispatch({
			...setDiagnostics(view.state, [
				{
					from,
					to: Math.min(line.to, from + 1),
					severity: "error",
					message: `${diagnostic.message} (${diagnostic.code})`,
				},
			]),
			selection: { anchor: from },
			scrollIntoView: true,
		});
	}, [diagnostic]);

	return (
		<div
			className="editor-host"
			data-diagnostic-code={diagnostic?.code}
			data-diagnostic-column={diagnostic?.column}
			data-diagnostic-line={diagnostic?.line}
			data-testid="scenario-editor"
			ref={editorHost}
		/>
	);
}
