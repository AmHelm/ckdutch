// @vitest-environment jsdom
import {
	act,
	cleanup,
	fireEvent,
	render,
	screen,
	waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
	wallet: undefined as ((account: string | null) => void) | undefined,
	getStatus: vi.fn(),
	connect: vi.fn(),
	disconnect: vi.fn(),
	setup: vi.fn(),
	call: vi.fn(),
	derive: vi.fn(),
	validate: vi.fn(),
}));
vi.mock("./chain", () => ({
	MAX_FILE_BYTES: 10 * 1024 * 1024,
	GLOBAL_CONTRACT: "flying-dman.testnet",
	accountId: (value: string) => value.trim(),
	subscribeWallet: (callback: (account: string | null) => void) => {
		mocks.wallet = callback;
		callback(null);
		return () => {
			mocks.wallet = undefined;
		};
	},
	startWallet: async () => {},
	getStatus: mocks.getStatus,
	connect: mocks.connect,
	disconnect: mocks.disconnect,
	setupSwitch: mocks.setup,
	callSwitch: mocks.call,
	deriveKey: mocks.derive,
}));
vi.mock("./crypto", () => ({
	loadCrypto: async () => ({ validateCapsule: mocks.validate }),
}));
import { App } from "./App";

const status = (owner = "owner.testnet", isPublic = false) => ({
	owner,
	version: "1",
	friends: ["friend.testnet"],
	challenge_deadline_ms: isPublic ? 10 : null,
	challenge_delay_ms: 60_000,
	now_ms: 100,
	key_is_public: isPublic,
});
const capsule = {
	filename: "hello.txt",
	contract: "owner.testnet",
	mpc_public_key: "mpc-key",
};
const wallet = async (account: string) => {
	await act(async () => mocks.wallet?.(account));
};
const navigate = (name: string) =>
	fireEvent.click(screen.getByRole("button", { name }));
const file = (value: string) => ({
	name: "capsule.ckdutch.json",
	size: value.length,
	arrayBuffer: async () => new TextEncoder().encode(value).buffer,
});

beforeEach(() => {
	vi.resetAllMocks();
	history.replaceState({}, "", "/");
	mocks.getStatus.mockImplementation(async (account: string) =>
		status(account),
	);
	mocks.validate.mockImplementation((json: string) => json);
});
afterEach(cleanup);

describe("wallet frontend flows", () => {
	it("preserves the landing page and offers Meteor with no private-key fields", () => {
		render(<App />);
		expect(
			screen.getByRole("heading", { name: "Some things should live on." }),
		).toBeTruthy();
		fireEvent.click(screen.getByRole("button", { name: "Connect account" }));
		expect(screen.getByRole("button", { name: "Connect Meteor" })).toBeTruthy();
		expect(document.querySelector('input[type="password"]')).toBeNull();
		expect(screen.queryByText("Download private key backup")).toBeNull();
	});

	it("keeps a cancelled wallet connection in the dialog with its error", async () => {
		mocks.connect.mockRejectedValue(new Error("Connection cancelled"));
		render(<App />);
		fireEvent.click(screen.getByRole("button", { name: "Connect account" }));
		fireEvent.click(screen.getByRole("button", { name: "Connect Meteor" }));
		await waitFor(() =>
			expect(screen.getByRole("alert").textContent).toBe(
				"Connection cancelled",
			),
		);
		expect(screen.getByRole("dialog")).toBeTruthy();
	});

	it("ignores a stale switch lookup when a different account was selected", async () => {
		let resolveOld!: (value: ReturnType<typeof status>) => void;
		mocks.getStatus.mockImplementation((account: string) =>
			account === "old.testnet"
				? new Promise((resolve) => {
						resolveOld = resolve;
					})
				: Promise.resolve({ ...status(account), friends: [] }),
		);
		render(<App />);
		const input = screen.getByLabelText("Switch account");
		fireEvent.change(input, { target: { value: "old.testnet" } });
		fireEvent.click(screen.getByRole("button", { name: "Look up" }));
		await waitFor(() =>
			expect(mocks.getStatus).toHaveBeenCalledWith("old.testnet"),
		);
		fireEvent.change(input, { target: { value: "new.testnet" } });
		fireEvent.click(screen.getByRole("button", { name: "Look up" }));
		await screen.findByText("Standing by");
		await act(async () => resolveOld(status("old.testnet")));
		expect(screen.queryByText("Trusted friends: friend.testnet")).toBeNull();
		expect((input as HTMLInputElement).value).toBe("new.testnet");
	});

	it("binds setup to the displayed account and resets consent after account changes", async () => {
		render(<App />);
		await wallet("owner.testnet");
		navigate("Set up a switch");
		const consent = screen.getByRole("checkbox");
		fireEvent.click(consent);
		await wallet("other.testnet");
		expect((consent as HTMLInputElement).checked).toBe(false);
		expect(
			(
				screen.getByRole("button", {
					name: "Create switch",
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
		fireEvent.click(consent);
		fireEvent.change(screen.getByLabelText(/Trusted friends/), {
			target: { value: "alice.testnet,\nbob.testnet" },
		});
		fireEvent.click(screen.getByRole("button", { name: "Create switch" }));
		await waitFor(() =>
			expect(mocks.setup).toHaveBeenCalledWith(
				"flying-dman.testnet",
				604_800_000,
				["alice.testnet", "bob.testnet"],
				"other.testnet",
				expect.any(Function),
			),
		);
	});

	it("seals with WASM, frees the key, wipes temporary bytes and clears the note", async () => {
		const free = vi.fn();
		let plaintext!: Uint8Array;
		const encrypt = vi.fn(
			(_contract, _mpcKey, _filename, bytes: Uint8Array) => {
				plaintext = bytes;
				expect(new TextDecoder().decode(bytes)).toBe("For tomorrow");
				return JSON.stringify(capsule);
			},
		);
		mocks.derive.mockResolvedValue({
			key: { encrypt, free },
			mpcKey: "mpc-key",
		});
		render(<App />);
		await wallet("owner.testnet");
		navigate("Seal a capsule");
		fireEvent.change(screen.getByLabelText("Your message"), {
			target: { value: "For tomorrow" },
		});
		fireEvent.click(
			screen.getByRole("button", { name: "Encrypt & seal capsule" }),
		);
		await screen.findByRole("button", { name: "Download capsule" });
		expect(mocks.derive).toHaveBeenCalledWith(
			"owner.testnet",
			expect.any(Function),
		);
		expect(free).toHaveBeenCalledOnce();
		expect(plaintext.every((byte) => byte === 0)).toBe(true);
		expect(
			(screen.getByLabelText("Your message") as HTMLTextAreaElement).value,
		).toBe("");
	});

	it("blocks non-owner recovery until release and wipes recovered contents on clear", async () => {
		const plaintext = new TextEncoder().encode("Recovered");
		const free = vi.fn();
		const decrypt = vi.fn(() => plaintext);
		mocks.derive.mockResolvedValue({
			key: { decrypt, free },
			mpcKey: "mpc-key",
		});
		render(<App />);
		await wallet("friend.testnet");
		navigate("Open a capsule");
		fireEvent.change(document.getElementById("open-file")!, {
			target: { files: [file(JSON.stringify(capsule))] },
		});
		await screen.findByText("Protected — no active challenge");
		expect(
			(
				screen.getByRole("button", {
					name: "Recover key & open",
				}) as HTMLButtonElement
			).disabled,
		).toBe(true);
		await wallet("owner.testnet");
		fireEvent.click(screen.getByRole("button", { name: "Recover key & open" }));
		await screen.findByText("Your capsule is open.");
		expect(decrypt).toHaveBeenCalledWith(JSON.stringify(capsule));
		expect(free).toHaveBeenCalledOnce();
		fireEvent.click(
			screen.getByRole("button", { name: "Clear recovered contents" }),
		);
		expect(plaintext.every((byte) => byte === 0)).toBe(true);
		expect(
			screen.queryByRole("button", { name: "Download recovered file" }),
		).toBeNull();
	});

	it("frees the returned key when capsule authentication fails", async () => {
		const free = vi.fn();
		mocks.derive.mockResolvedValue({
			key: {
				decrypt: () => {
					throw new Error("Capsule authentication failed");
				},
				free,
			},
			mpcKey: "mpc-key",
		});
		render(<App />);
		await wallet("owner.testnet");
		navigate("Open a capsule");
		fireEvent.change(document.getElementById("open-file")!, {
			target: { files: [file(JSON.stringify(capsule))] },
		});
		await screen.findByText("Protected — no active challenge");
		fireEvent.click(screen.getByRole("button", { name: "Recover key & open" }));
		await screen.findByText("Capsule authentication failed");
		expect(free).toHaveBeenCalledOnce();
		expect(
			screen.queryByRole("button", { name: "Download recovered file" }),
		).toBeNull();
	});
});
