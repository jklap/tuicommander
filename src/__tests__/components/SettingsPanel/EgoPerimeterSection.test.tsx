import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import "../../mocks/tauri";
import { EgoPerimeterSection } from "../../../components/SettingsPanel/tabs/EgoPerimeterSection";
import type { EgoPerimeterClient } from "../../../services/egoCli";
import type { EgoPerimeterView } from "../../../types/ego";
import effective from "../../fixtures/ego-perimeter/effective-default.json";
import withReason from "../../fixtures/ego-perimeter/effective-with-reason.json";
import measured from "../../fixtures/ego-perimeter/measured-contract.json";

const initial: EgoPerimeterView = {
	profile: null,
	roots: { rootDir: "/Users/stefano.straus/Gits", rootAccess: "read-write", readAllowlist: "", writableDirs: "" },
	networkEnabled: true,
	execEnforcement: "notChecked",
	effective: { ...effective, roots: [{ path: "/Users/stefano.straus/Gits", access: "read-write", source: "default" }] },
	preview: "Recorded effective perimeter: seatbelt, capabilities not_checked, network online",
};
function client(over: Partial<EgoPerimeterClient> = {}): EgoPerimeterClient {
	return { perimeter: async () => initial, setRoots: async () => initial, setNetwork: async () => initial, ...over };
}
describe("EgoPerimeterSection", () => {
	afterEach(cleanup);

	// Catches: a reported Seatbelt backend is falsely presented as measured OS enforcement.
	it("does not claim OS enforcement when ego reports not_checked", async () => {
		render(() => <EgoPerimeterSection client={client()} />);
		expect(await screen.findByText("Enforcement not checked")).toBeTruthy();
		expect(screen.queryByText("Enforced by OS")).toBeNull();
		expect(screen.getByText(initial.preview)).toBeTruthy();
		expect((screen.getByLabelText("Root directory") as HTMLInputElement).value).toBe("/Users/stefano.straus/Gits");
	});

	// Catches: the backend's three enforcement states render the wrong badge.
	it.each([
		["complete ro offline", "enforcedByOs", "Enforced by OS", measured.capabilities, "ro", "offline"],
		["completed empty measurement", "promptOnly", "Prompt only", [], "ro", "offline"],
		["unknown evidence", "notChecked", "Enforcement not checked", "future", "ro", "online"],
	] as const)(
		"renders the backend's exec badge for %s",
		async (_case, execEnforcement, text, capabilities, sandbox, network) => {
			render(() => (
				<EgoPerimeterSection
					client={client({
						perimeter: async () => ({
							...initial,
							execEnforcement,
							effective: {
								...initial.effective,
								capabilities,
								sandbox,
								network,
								probe_evidence: measured.probe_evidence,
							},
						}),
					})}
				/>
			));
			expect(await screen.findByText(text)).toBeTruthy();
			expect(screen.getByText("Exec sandbox:")).toBeTruthy();
			if (execEnforcement === "promptOnly") expect(screen.getByText(/not fully OS-enforced/)).toBeTruthy();
		},
	);

	// Catches: the new inspector's unverified reason is hidden behind an OS backend name.
	it("shows the unchecked reason recorded from ego297", async () => {
		render(() => (
			<EgoPerimeterSection
				client={client({
					perimeter: async () => ({
						...initial,
						effective: { ...withReason, roots: initial.effective.roots },
					}),
				})}
			/>
		));
		expect(
			await screen.findByText("no completed backend measurement is available; inspection does not run probes"),
		).toBeTruthy();
		expect(screen.getByText("Enforcement not checked")).toBeTruthy();
	});

	// Catches: saving roots silently grants write access or shows the submitted value instead of ego's readback.
	it("sends access and both allowlists and adopts ego's successful readback", async () => {
		const setRoots = vi.fn(async () => ({
			...initial,
			roots: { ...initial.roots, rootDir: "/canonical/root", rootAccess: "read" as const },
		}));
		render(() => <EgoPerimeterSection client={client({ setRoots })} />);
		const root = await screen.findByLabelText("Root directory");
		fireEvent.input(root, { target: { value: "/requested/root" } });
		fireEvent.change(screen.getByLabelText("Root access"), { target: { value: "read" } });
		fireEvent.input(screen.getByLabelText("Read allowlist"), { target: { value: "/reference\n/notes" } });
		fireEvent.input(screen.getByLabelText("Extra writable directories"), { target: { value: "/scratch" } });
		fireEvent.click(screen.getByRole("button", { name: "Save roots in ego" }));
		await vi.waitFor(() =>
			expect(setRoots).toHaveBeenCalledWith({
				rootDir: "/requested/root",
				rootAccess: "read",
				readAllowlist: "/reference\n/notes",
				writableDirs: "/scratch",
			}),
		);
		await vi.waitFor(() => expect((root as HTMLInputElement).value).toBe("/canonical/root"));
	});

	// Catches: toggling network overwrites unrelated unsaved root edits.
	it("changes network without discarding an unsaved root draft", async () => {
		const setNetwork = vi.fn(async () => ({ ...initial, networkEnabled: false }));
		render(() => <EgoPerimeterSection client={client({ setNetwork })} />);
		const root = await screen.findByLabelText("Root directory");
		fireEvent.input(root, { target: { value: "/unsaved/root" } });
		fireEvent.click(screen.getByLabelText("Network enabled"));
		await vi.waitFor(() => expect(setNetwork).toHaveBeenCalledWith(false));
		expect((root as HTMLInputElement).value).toBe("/unsaved/root");
		expect((screen.getByLabelText("Network enabled") as HTMLInputElement).checked).toBe(false);
	});

	// Catches: a failed write or readback clears the draft and claims it succeeded.
	it("keeps a failed draft editable and reports ego's refusal", async () => {
		render(() => (
			<EgoPerimeterSection
				client={client({
					setRoots: async () => {
						throw {
							code: "commandFailed",
							message: "ego refused the command",
							stderr: "root exceeds system ceiling",
							stdout: "",
							command: "ego config set roots",
							exitCode: 1,
						};
					},
				})}
			/>
		));
		const root = await screen.findByLabelText("Root directory");
		fireEvent.input(root, { target: { value: "/rejected/root" } });
		fireEvent.click(screen.getByRole("button", { name: "Save roots in ego" }));
		expect(await screen.findByText(/root exceeds system ceiling/)).toBeTruthy();
		expect((root as HTMLInputElement).value).toBe("/rejected/root");
		expect((screen.getByRole("button", { name: "Save roots in ego" }) as HTMLButtonElement).disabled).toBe(false);
	});

	// Catches: a refused network write leaves the native checkbox showing an unsaved mode.
	it("keeps the stored network mode when ego refuses the change", async () => {
		render(() => (
			<EgoPerimeterSection
				client={client({
					setNetwork: async () => {
						throw {
							code: "commandFailed",
							message: "ego refused network",
							command: "ego config set network",
							stdout: "",
							stderr: "system requires offline",
							exitCode: 1,
						};
					},
				})}
			/>
		));
		const network = await screen.findByLabelText("Network enabled");
		fireEvent.click(network);
		expect(await screen.findByText(/system requires offline/)).toBeTruthy();
		expect((network as HTMLInputElement).checked).toBe(true);
	});
});
