import { type JSX, splitProps } from "solid-js";
import { cx } from "../../utils";
import s from "./FormControls.module.css";

export type ButtonProps = JSX.ButtonHTMLAttributes<HTMLButtonElement> & {
	variant?: "primary" | "secondary" | "ghost" | "danger";
	size?: "default" | "compact";
	geometry?: "default" | "stretch";
};

/** Native action button; list rows and cards retain their layout semantics. */
export function Button(props: ButtonProps) {
	const [local, rest] = splitProps(props, ["class", "classList", "variant", "size", "geometry", "type"]);
	return (
		<button
			{...rest}
			type={local.type ?? "button"}
			data-form-control
			class={cx(
				s.control,
				s.button,
				s[local.variant ?? "secondary"],
				local.size === "compact" && s.compact,
				local.geometry === "stretch" && s.stretchButton,
				local.class,
			)}
			classList={local.classList}
		/>
	);
}
