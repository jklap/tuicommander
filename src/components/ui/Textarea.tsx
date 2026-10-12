import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type TextareaProps = JSX.TextareaHTMLAttributes<HTMLTextAreaElement>;

export function Textarea(props: TextareaProps) {
	const [local, rest] = splitProps(props, ["class", "classList"]);
	return (
		<textarea {...rest} data-form-control class={cx(s.control, s.textarea, local.class)} classList={local.classList} />
	);
}
