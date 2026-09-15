import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { RepoPickerDialog } from "../../components/RepoPickerDialog/RepoPickerDialog";

describe("RepoPickerDialog", () => {
	const baseProps = {
		visible: true,
		path: "/Users/dev/some-folder",
		repos: [
			{ path: "/repo-a", displayName: "repo-a" },
			{ path: "/repo-b", displayName: "repo-b" },
		],
		onChooseRepo: () => {},
		onRegister: () => {},
		onUnattached: () => {},
		onClose: () => {},
	};

	it("renders nothing when not visible", () => {
		const { container } = render(() => <RepoPickerDialog {...baseProps} visible={false} />);
		expect(container.querySelector("button")).toBeNull();
	});

	it("shows the invoked path", () => {
		const { getByText } = render(() => <RepoPickerDialog {...baseProps} />);
		expect(getByText("/Users/dev/some-folder")).toBeTruthy();
	});

	it("lists every repo and calls onChooseRepo with its path when clicked", () => {
		const onChooseRepo = vi.fn();
		const { getByText } = render(() => <RepoPickerDialog {...baseProps} onChooseRepo={onChooseRepo} />);

		fireEvent.click(getByText("repo-a"));
		expect(onChooseRepo).toHaveBeenCalledWith("/repo-a");

		fireEvent.click(getByText("repo-b"));
		expect(onChooseRepo).toHaveBeenCalledWith("/repo-b");
	});

	it("shows an empty state instead of a list when there are no repos", () => {
		const { getByText, queryByText } = render(() => <RepoPickerDialog {...baseProps} repos={[]} />);
		expect(getByText(/no repositories are registered/i)).toBeTruthy();
		expect(queryByText("repo-a")).toBeNull();
	});

	it("still offers both escape hatches when there are no repos", () => {
		const { getByText } = render(() => <RepoPickerDialog {...baseProps} repos={[]} />);
		expect(getByText(/add this folder as a repository/i)).toBeTruthy();
		expect(getByText(/open unattached terminal/i)).toBeTruthy();
	});

	it("wires the register escape hatch", () => {
		const onRegister = vi.fn();
		const { getByText } = render(() => <RepoPickerDialog {...baseProps} onRegister={onRegister} />);
		fireEvent.click(getByText(/add this folder as a repository/i));
		expect(onRegister).toHaveBeenCalledTimes(1);
	});

	it("wires the unattached escape hatch", () => {
		const onUnattached = vi.fn();
		const { getByText } = render(() => <RepoPickerDialog {...baseProps} onUnattached={onUnattached} />);
		fireEvent.click(getByText(/open unattached terminal/i));
		expect(onUnattached).toHaveBeenCalledTimes(1);
	});

	it("Cancel button closes without choosing anything", () => {
		const onClose = vi.fn();
		const onChooseRepo = vi.fn();
		const { getByText } = render(() => (
			<RepoPickerDialog {...baseProps} onClose={onClose} onChooseRepo={onChooseRepo} />
		));
		fireEvent.click(getByText("Cancel"));
		expect(onClose).toHaveBeenCalledTimes(1);
		expect(onChooseRepo).not.toHaveBeenCalled();
	});

	it("Escape closes the dialog", () => {
		const onClose = vi.fn();
		render(() => <RepoPickerDialog {...baseProps} onClose={onClose} />);
		fireEvent.keyDown(document, { key: "Escape" });
		expect(onClose).toHaveBeenCalledTimes(1);
	});

	it("clicking the overlay closes the dialog, clicking inside the popover does not", () => {
		const onClose = vi.fn();
		const { container, getByText } = render(() => <RepoPickerDialog {...baseProps} onClose={onClose} />);

		fireEvent.click(getByText("repo-a").closest("div") as Element);
		expect(onClose).not.toHaveBeenCalled();

		const overlay = container.firstElementChild as Element;
		fireEvent.click(overlay);
		expect(onClose).toHaveBeenCalledTimes(1);
	});
});
