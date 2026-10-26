import { render } from "solid-js/web";
import { CommentOverlay } from "../../components/MarkdownTab/CommentOverlay";
import { Button } from "../../components/ui/Button";
import { ColorSwatch } from "../../components/ui/ColorSwatch";
import { Input } from "../../components/ui/Input";
import { Select } from "../../components/ui/Select";
import { Slider } from "../../components/ui/Slider";
import { Switch } from "../../components/ui/Switch";
import { Textarea } from "../../components/ui/Textarea";
import "../../global.css";

render(() => {
	let content!: HTMLDivElement;
	return (
		<main style={{ padding: "48px", display: "grid", gap: "16px", width: "600px" }}>
			<h1>Shared field keyboard focus</h1>
			<Input aria-label="Title" />
			<Input compact aria-label="Compact title" />
			<Textarea aria-label="Description" />
			<Textarea aria-label="Invalid description" aria-invalid="true" />
			<Button>Action</Button>
			<Select aria-label="Choice">
				<option>One</option>
			</Select>
			<Switch aria-label="Enabled" />
			<Slider aria-label="Volume" />
			<ColorSwatch aria-label="Blue" swatchColor="#7aa2f7" />
			<div ref={content}>
				<span
					class="tweak-highlight"
					data-tweak-id="focus"
					data-tweak-comment="Keep the focus indicator next to the field border."
					data-tweak-at="2026-10-26T12:00:00Z"
				>
					Commented Markdown text
				</span>
			</div>
			<CommentOverlay contentRef={content} onSave={() => true} onDelete={() => true} />
		</main>
	);
}, document.body);
