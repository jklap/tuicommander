/** Whether `pattern` compiles as a JavaScript regular expression.
 *
 * Compiled with no flags — the smart-selection match engine's own compile step
 * (`compileRules` in `src/components/Terminal/smartSelection.ts`) adds the `"g"` flag, but no
 * pattern differs in validity between the two, so this stays a plain validity check. */
export function isValidRegex(pattern: string): boolean {
	try {
		new RegExp(pattern);
		return true;
	} catch {
		return false;
	}
}
