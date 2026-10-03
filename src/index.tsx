/* @refresh reload */
const root = document.getElementById("app");
if (!root) throw new Error("Root element #app not found");

if (/^#\/secret-form(?:\?|$)/.test(window.location.hash)) {
	document.getElementById("splash")?.remove();
	root.id = "secret-form-root";
	void import("./secretForm").then(({ startSecretForm }) => startSecretForm(root));
} else {
	void import("./appEntry");
}
