/**
 * The literal text `${NAME}` — a shell variable reference — as a plain string.
 *
 * Tests for Run Command assert on command lines like `imark "${TUIC_M0}"`. Written as an ordinary
 * string that trips `lint/suspicious/noTemplateCurlyInString` (rightly, in general: it usually means a
 * forgotten backtick), and escaping the brace inside a template literal makes every expectation hard to
 * read. Building the reference from parts keeps the expectations readable and the lint rule useful.
 */
export const shVar = (name: string): string => "$" + "{" + name + "}";
