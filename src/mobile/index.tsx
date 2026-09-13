/* @refresh reload */
import { ErrorBoundary, render } from "solid-js/web";
import { CrashScreen } from "../components/CrashScreen/CrashScreen";
import { appLogger } from "../stores/appLogger";
import MobileApp from "./MobileApp";
import "./mobile.css";

// Global error handlers
window.addEventListener("error", (event) => {
	appLogger.error("app", `Uncaught: ${event.message}`, {
		filename: event.filename,
		lineno: event.lineno,
		colno: event.colno,
	});
});

window.addEventListener("unhandledrejection", (event) => {
	const reason = event.reason instanceof Error ? event.reason.message : String(event.reason);
	const stack = event.reason instanceof Error ? event.reason.stack : undefined;
	appLogger.error("app", `Unhandled rejection: ${reason}`, stack ? { stack } : undefined);
});

const root = document.getElementById("mobile-app");

if (!root) {
	throw new Error("Root element #mobile-app not found");
}

render(
	() => (
		<ErrorBoundary fallback={(err) => <CrashScreen title="TUICommander Mobile crashed" error={err} />}>
			<MobileApp />
		</ErrorBoundary>
	),
	root,
);
