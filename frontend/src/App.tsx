import {
	createContext,
	useCallback,
	useContext,
	useEffect,
	useId,
	useRef,
	useState,
	type ReactNode,
} from "react";
import * as chain from "./chain";
import { Icon } from "./Icon";
import { download, duration, readFile } from "./files";
import { loadCrypto } from "./crypto";

type Page =
	| "Overview"
	| "Seal a capsule"
	| "Open a capsule"
	| "Set up a switch";
type State = {
	page: Page;
	setPage: (page: Page) => void;
	wallet: string | null;
	account: string;
	selectAccount: (account: string) => void;
	status: chain.SwitchStatus | null;
	statusError: string;
	busy: boolean;
	now: number;
	showConnect: () => void;
	notify: (message: string, error?: boolean) => void;
	run: (work: () => Promise<string>) => Promise<void>;
	onHash: (hash: string) => void;
	call: (method: string, args: object, success: string) => void;
};
const Context = createContext<State | null>(null);
const useApp = () => useContext(Context)!;
const errorMessage = (error: unknown) =>
	error instanceof Error ? error.message : String(error);

export function App() {
	const [page, setPage] = useState<Page>("Overview");
	const [wallet, setWallet] = useState<string | null>(null);
	const [account, setAccount] = useState(
		() => new URLSearchParams(location.search).get("account") ?? "",
	);
	const [status, setStatus] = useState<chain.SwitchStatus | null>(null);
	const [statusError, setStatusError] = useState("");
	const [busy, setBusy] = useState(false);
	const [message, setMessage] = useState("");
	const [error, setError] = useState(false);
	const [transaction, setTransaction] = useState("");
	const [connectOpen, setConnectOpen] = useState(false);
	const [now, setNow] = useState(Date.now);
	const currentAccount = useRef(account);
	const request = useRef(0);
	const locked = useRef(false);
	const notify = useCallback((value: string, failed = false) => {
		setMessage(value);
		setError(failed);
	}, []);
	const refresh = useCallback(async () => {
		const selected = currentAccount.current;
		const sequence = ++request.current;
		if (!selected) return;
		try {
			const next = await chain.getStatus(selected);
			if (sequence !== request.current || selected !== currentAccount.current)
				return;
			setStatus(next);
			setStatusError("");
		} catch (e) {
			if (sequence !== request.current || selected !== currentAccount.current)
				return;
			setStatus(null);
			setStatusError(errorMessage(e));
		}
	}, []);
	const selectAccount = useCallback((value: string) => {
		currentAccount.current = value.trim();
		setAccount(value.trim());
		setStatus(null);
		setStatusError("");
		void refresh();
	}, [refresh]);
	useEffect(() => {
		void refresh();
	}, [refresh]);
	useEffect(() => {
		const clock = window.setInterval(() => setNow(Date.now()), 1_000);
		const poll = window.setInterval(() => {
			if (!locked.current) void refresh();
		}, 15_000);
		return () => {
			clearInterval(clock);
			clearInterval(poll);
			request.current++;
		};
	}, [refresh]);
	useEffect(() => {
		const unsubscribe = chain.subscribeWallet((next) => {
			setWallet(next);
			if (next && !currentAccount.current) selectAccount(next);
		});
		void chain.startWallet().catch((e) => notify(errorMessage(e), true));
		return unsubscribe;
	}, [notify, selectAccount]);
	const run = async (work: () => Promise<string>) => {
		if (locked.current) return;
		locked.current = true;
		setBusy(true);
		setTransaction("");
		notify(
			"Waiting for NEAR… Keep this tab open while the transaction completes.",
		);
		try {
			notify(await work());
		} catch (e) {
			notify(errorMessage(e), true);
		} finally {
			locked.current = false;
			setBusy(false);
			void refresh();
		}
	};
	const call = (method: string, args: object, success: string) => {
		const selected = currentAccount.current;
		void run(async () => {
			await chain.callSwitch(selected, method, args, setTransaction);
			return success;
		});
	};
	const state: State = {
		page,
		setPage,
		wallet,
		account,
		selectAccount,
		status,
		statusError,
		busy,
		now,
		showConnect: () => setConnectOpen(true),
		notify,
		run,
		onHash: setTransaction,
		call,
	};
	return (
		<Context.Provider value={state}>
			<div className="app-shell">
				<aside className="sidebar">
					<a className="brand" href="/" aria-label="CKDutch home">
						<span className="brand-mark" aria-hidden="true">
							<span className="skull-mark" />
						</span>
						<span>
							ckdutch<small>LEAVE SOMETHING BEHIND</small>
						</span>
					</a>
					<div className="nav-label">YOUR SPACE</div>
					<nav aria-label="Main navigation">
						{(
							[
								"Overview",
								"Seal a capsule",
								"Open a capsule",
								"Set up a switch",
							] as Page[]
						).map((name, i) => (
							<button
									key={name}
									aria-label={name}
									aria-current={page === name ? "page" : undefined}
								className={`nav-item${page === name ? " active" : ""}`}
								disabled={busy}
								onClick={() => setPage(name)}
							>
								<Icon name={["grid", "lock", "unlock", "settings"][i]} />
								<span>{name}</span>
								<span className="nav-arrow">↗</span>
							</button>
						))}
					</nav>
					<div className="sidebar-bottom">
						<div className="little-orbit">✳</div>
						<h3>A little peace of mind.</h3>
						<p>
							For the words, memories, and things that should never be lost.
						</p>
						<div className="network">
							<span className="dot" />
							NEAR Testnet<span className="network-tag">DEVELOPMENT</span>
						</div>
					</div>
				</aside>
				<div className="workspace">
					<header className="topbar">
						<div className="breadcrumb">
							Your space<span>/</span>
							{page}
						</div>
						<button
							className="connect-button"
							disabled={busy}
							onClick={state.showConnect}
						>
							<span className="wallet-dot" />
							{wallet ?? "Connect account"}
							<Icon name="arrow" />
						</button>
					</header>
					<main>
						{message && (
							<div
								className={`notice${error ? " notice-error" : ""}`}
								role="status"
								aria-live="polite"
							>
								<span>{message}</span>
								{transaction && (
									<a
										target="_blank"
										rel="noopener noreferrer"
										href={`https://testnet.nearblocks.io/txns/${encodeURIComponent(transaction)}`}
									>
										View transaction ↗
									</a>
								)}
							</div>
						)}
						{page === "Overview" ? (
							<Overview />
						) : page === "Seal a capsule" ? (
							<Seal />
						) : page === "Open a capsule" ? (
							<Open />
						) : (
							<Setup />
						)}
						<footer>
							<span>Built for a future you can’t predict.</span>
							<span>
								Encrypted in your browser. Secured by NEAR.
								<Icon name="shield" />
							</span>
						</footer>
					</main>
				</div>
				{connectOpen && <Connect close={() => setConnectOpen(false)} />}
			</div>
		</Context.Provider>
	);
}

function Heading({
	eyebrow,
	title,
	children,
}: {
	eyebrow: string;
	title: string;
	children: ReactNode;
}) {
	return (
		<div className="page-heading">
			<div className="eyebrow">{eyebrow}</div>
			<h1>{title}</h1>
			<p>{children}</p>
		</div>
	);
}
function AccountPicker() {
	const s = useApp();
	const id = useId();
	const [draft, setDraft] = useState(s.account);
	useEffect(() => setDraft(s.account), [s.account]);
	return (
		<form
			className="account-picker"
			onSubmit={(e) => {
				e.preventDefault();
				s.selectAccount(draft);
			}}
		>
			<label htmlFor={id}>Switch account</label>
			<div className="input-button">
				<input
					id={id}
					placeholder="your-switch.testnet"
					value={draft}
					onChange={(e) => setDraft(e.target.value)}
					disabled={s.busy}
				/>
				<button className="button secondary" type="submit" disabled={s.busy}>
					Look up
					<Icon name="arrow" />
				</button>
			</div>
		</form>
	);
}
function Overview() {
	const s = useApp();
	const [confirmed, setConfirmed] = useState(false);
	useEffect(() => setConfirmed(false), [s.account]);
	const status = s.status;
	const deadline = status?.challenge_deadline_ms;
	const canCheckIn =
		status &&
		s.wallet &&
		(s.wallet === status.owner || status.friends.includes(s.wallet));
	return (
		<>
			<Heading
				eyebrow="A LITTLE PEACE OF MIND"
				title="Some things should live on."
			>
				Keep your important things safe today. Make sure they can be found
				tomorrow.
			</Heading>
			<section className="hero-card">
				<div className="hero-copy">
					<div className="pill">
						<span className="dot" />
						YOUR LEGACY, ON YOUR TERMS
					</div>
					<h2>
						A safe place for
						<br />
						your just-in-case.
					</h2>
					<p>
						Seal a message or file. You stay in control. If you’re no longer
						here to check in, the people you leave behind can open it.
					</p>
					<button
						className="button primary"
						disabled={s.busy}
						onClick={() => s.setPage("Seal a capsule")}
					>
						Seal your first capsule
						<Icon name="arrow" />
					</button>
					<span className="hero-caption">
						<Icon name="lock" />
						Only encrypted data leaves your browser.
					</span>
				</div>
				<div className="hero-art" aria-hidden="true">
					<div className="orbit orbit-one" />
					<div className="orbit orbit-two" />
					<div className="orbit orbit-three" />
					<span className="art-star star-one">✧</span>
					<span className="art-star star-two">✦</span>
					<div className="capsule-shadow" />
					<div className="envelope">
						<div className="envelope-paper">
							<span />
							<span />
							<span />
						</div>
						<div className="envelope-back" />
						<div className="envelope-front" />
						<div className="envelope-seal">
							<span className="skull-mark" />
						</div>
					</div>
					<div className="art-label">
						<span className="dot" />
						For when it matters.
					</div>
				</div>
			</section>
			<div className="section-title">
				<h2>Your switch</h2>
				<span className="small-tag">ON-CHAIN PROTECTION</span>
			</div>
			<section className="card switch-card">
				<AccountPicker />
				{s.statusError && <p className="field-error">{s.statusError}</p>}
				<div className="metrics">
					<div>
						<span className="metric-label">STATUS</span>
						<strong className="status-value">
							<span className="dot" />
							{!status
								? "No switch selected"
								: status.key_is_public
									? "Key is releasable"
									: deadline != null
										? "Response needed"
										: "Standing by"}
						</strong>
					</div>
					<div>
						<span className="metric-label">RESPONSE WINDOW</span>
						<strong>
							{status ? duration(status.challenge_delay_ms) : "—"}
						</strong>
					</div>
					<div>
						<span className="metric-label">TRUSTED FRIENDS</span>
						<strong>
							{status?.friends.length ?? "—"}
							<small>can check in for you</small>
						</strong>
					</div>
				</div>
				{deadline != null && (
					<div className="deadline">
						<Icon name="clock" />
						<div>
							<strong>
								Release deadline: {new Date(deadline).toUTCString()}
							</strong>
							<p>
								{deadline <= s.now
									? "The local countdown has ended. Chain status determines when recovery is allowed."
									: `${duration(deadline - s.now)} remaining to check in`}
							</p>
						</div>
					</div>
				)}
				<div className="switch-bottom">
					<p>
						<Icon name="shield" />
						{status
							? "A challenge starts the clock. Check in to cancel it."
							: "Already have a switch? Enter its account above, or create one."}
					</p>
					{canCheckIn && (
						<button
							className="button primary compact"
							disabled={s.busy}
							onClick={() =>
								s.call(
									"claim_owner_is_alive",
									{},
									"Check-in confirmed. The challenge is cancelled.",
								)
							}
						>
							I’m still here
							<Icon name="check" />
						</button>
					)}
					{!status && (
						<button
							className="text-button"
							disabled={s.busy}
							onClick={() => s.setPage("Set up a switch")}
						>
							Set up a switch ↗
						</button>
					)}
				</div>
				{!!status?.friends.length && (
					<div className="friends-list">
						Trusted friends: {status.friends.join(", ")}
					</div>
				)}
				{status && (
					<details className="challenge-details">
						<summary>Looking after someone else’s capsule?</summary>
						<p>
							Start a challenge if the owner may no longer be here. The owner or
							a trusted friend can cancel it during the response window. After
							the deadline, anyone can request the key.
						</p>
						<label className="checkbox">
							<input
								type="checkbox"
								checked={confirmed}
								disabled={s.busy}
								onChange={(e) => setConfirmed(e.target.checked)}
							/>
							<span>
								I understand this starts the public release countdown.
							</span>
						</label>
						<button
							className="button danger"
							disabled={s.busy || !confirmed || !s.wallet || deadline != null}
							onClick={() =>
								s.call(
									"claim_owner_is_dead",
									{},
									"Challenge started. The owner or a trusted friend can now respond.",
								)
							}
						>
							Start a challenge
							<Icon name="clock" />
						</button>
						{!s.wallet && (
							<p className="hint">Connect an account to submit a challenge.</p>
						)}
					</details>
				)}
			</section>
			<div className="section-title">
				<h2>A simple promise. Three steps.</h2>
				<span>How it works</span>
			</div>
			<div className="steps-grid">
				{[
					[
						"lock",
						"Seal something meaningful",
						"A letter, a memory, a document. Encrypt it with a key derived for your switch.",
					],
					[
						"shield",
						"Stay in control",
						"Share the sealed capsule anywhere. Only you can obtain its key while your switch is protected.",
					],
					[
						"sun",
						"Let it find its way",
						"An unanswered challenge unlocks recovery. Anyone with the capsule can then open it.",
					],
				].map(([icon, title, body], i) => (
					<article className="step-card" key={icon}>
						<div className="step-top">
							<span className="step-icon">
								<Icon name={icon} />
							</span>
							<span>0{i + 1}</span>
						</div>
						<h3>{title}</h3>
						<p>{body}</p>
					</article>
				))}
			</div>
		</>
	);
}

function Seal() {
	const s = useApp();
	const [note, setNote] = useState("");
	const [filename, setFilename] = useState("a-note-for-you.txt");
	const [file, setFile] = useState<File>();
	const [capsule, setCapsule] = useState<chain.Envelope>();
	const fileInput = useRef<HTMLInputElement>(null);
	const seal = () =>
		void s.run(async () => {
			setCapsule(undefined);
			if (!s.wallet)
				throw new Error("Connect the account that owns your switch first");
			if (!filename.trim()) throw new Error("Give your capsule a filename");
			if (new TextEncoder().encode(filename).length > 255)
				throw new Error("Filename is too long");
			const data = file ? await readFile(file) : new TextEncoder().encode(note);
			try {
				if (!data.length)
					throw new Error("Write a message or select a file first");
				if (data.length > chain.MAX_FILE_BYTES)
					throw new Error("File is too large");
				const { key, mpcKey } = await chain.deriveKey(s.wallet, s.onHash);
				try {
					setCapsule(JSON.parse(key.encrypt(s.wallet, mpcKey, filename, data)));
				} finally {
					key.free();
				}
				setNote("");
				setFile(undefined);
				if (fileInput.current) fileInput.current.value = "";
				s.selectAccount(s.wallet);
				return "Your capsule is sealed. Download it and share the encrypted file with the people who should have it.";
			} finally {
				data.fill(0);
			}
		});
	return (
		<>
			<Heading
				eyebrow="01 / SEAL A CAPSULE"
				title="Give your words a tomorrow."
			>
				Write a note or choose a file. Its contents are encrypted here, in your
				browser.
			</Heading>
			<div className="two-column">
				<section className="card form-card">
					<div className="card-heading">
						<span className="step-icon">
							<Icon name="lock" />
						</span>
						<div>
							<h2>What would you like to leave?</h2>
							<p>A little care, tucked away for later.</p>
						</div>
					</div>
					<label htmlFor="capsule-name">File name</label>
					<input
						id="capsule-name"
						maxLength={255}
						value={filename}
						disabled={s.busy}
						onChange={(e) => setFilename(e.target.value)}
					/>
					<label htmlFor="capsule-note">Your message</label>
					<textarea
						id="capsule-note"
						rows={8}
						placeholder="To the people I love…"
						value={note}
						disabled={s.busy || !!file}
						onChange={(e) => setNote(e.target.value)}
					/>
					<div className="or-divider">
						<span />
						OR CHOOSE A FILE
						<span />
					</div>
					<label className="upload-zone" htmlFor="seal-file">
						<Icon name="upload" />
						<strong>{file?.name ?? "Choose something to keep safe"}</strong>
						<span>Any file · up to 10 MiB</span>
						<input
							ref={fileInput}
							id="seal-file"
							type="file"
							disabled={s.busy}
							onChange={(e) => {
								const next = e.target.files?.[0];
								if (!next) return;
								if (next.size > chain.MAX_FILE_BYTES) {
									e.target.value = "";
									s.notify("File is too large", true);
									return;
								}
								setFile(next);
								setFilename(next.name);
							}}
						/>
					</label>
					{file && (
						<button
							className="text-button"
							disabled={s.busy}
							onClick={() => {
								setFile(undefined);
								if (fileInput.current) fileInput.current.value = "";
							}}
						>
							Remove file and write a note
						</button>
					)}
					<div className="form-actions">
						<button
							className="button primary"
							disabled={s.busy || !s.wallet}
							onClick={seal}
						>
							<Icon name="lock" />
							{s.busy ? "Sealing…" : "Encrypt & seal capsule"}
						</button>
					</div>
					{!s.wallet && (
						<p className="hint">
							Connect the account that owns your switch to seal a capsule.
						</p>
					)}
				</section>
				<aside className="info-column">
					<div className="info-card">
						<span className="eyebrow">PRIVATE BY DESIGN</span>
						<h3>Your browser holds the pen. And the key.</h3>
						<p>
							NEAR’s MPC network derives a key for your switch and encrypts it
							to a fresh public key from this browser. The response is verified
							before your file is encrypted.
						</p>
						<div className="info-divider" />
						<p>
							The file name and switch account are public. Your message or file
							contents stay encrypted.
						</p>
						<p>
							All capsules from the same switch use the same derived key. A
							release makes all of them recoverable.
						</p>
					</div>
					{capsule && (
						<div className="info-card success-card">
							<span className="step-icon">
								<Icon name="check" />
							</span>
							<h3>Sealed, and ready to share.</h3>
							<p>
								Keep the downloaded .ckdutch.json file somewhere others can find
								it. This app doesn’t upload or store it.
							</p>
							<button
								className="button primary"
								onClick={() => {
									download(
										`${capsule.filename}.ckdutch.json`,
										new TextEncoder().encode(JSON.stringify(capsule, null, 2)),
										"application/json",
									);
									s.notify(
										"Capsule download started. Share this encrypted file publicly or with your recipients.",
									);
								}}
							>
								<Icon name="download" />
								Download capsule
							</button>
						</div>
					)}
				</aside>
			</div>
		</>
	);
}

function Open() {
	const s = useApp();
	const [capsule, setCapsule] = useState<chain.Envelope>();
	const [plaintext, setPlaintext] = useState<Uint8Array>();
	const recovered = useRef<Uint8Array | undefined>(undefined);
	const upload = useRef(0);
	const [loading, setLoading] = useState(false);
	const clear = () => {
		recovered.current?.fill(0);
		recovered.current = undefined;
		setPlaintext(undefined);
	};
	useEffect(
		() => () => {
			recovered.current?.fill(0);
			upload.current++;
		},
		[],
	);
	const read = async (file: File | undefined) => {
		if (!file) return;
		const sequence = ++upload.current;
		clear();
		setCapsule(undefined);
		setLoading(true);
		try {
			const bytes = await readFile(file, chain.MAX_FILE_BYTES * 2);
			const crypto = await loadCrypto();
			const next: chain.Envelope = JSON.parse(
				crypto.validateCapsule(
					new TextDecoder("utf-8", { fatal: true }).decode(bytes),
				),
			);
			if (sequence !== upload.current) return;
			setCapsule(next);
			s.selectAccount(next.contract);
		} catch (e) {
			if (sequence === upload.current) s.notify(errorMessage(e), true);
		} finally {
			if (sequence === upload.current) setLoading(false);
		}
	};
	const open = () =>
		void s.run(async () => {
			clear();
			if (!capsule) throw new Error("Choose a capsule first");
			const { key, mpcKey } = await chain.deriveKey(capsule.contract, s.onHash);
			try {
				if (mpcKey !== capsule.mpc_public_key)
					throw new Error(
						"The MPC domain key has changed since this capsule was sealed",
					);
				const data = key.decrypt(JSON.stringify(capsule));
				recovered.current = data;
				setPlaintext(data);
			} finally {
				key.free();
			}
			return "Capsule opened and authenticated. Download the recovered file below.";
		});
	const status = s.status?.owner === capsule?.contract ? s.status : null;
	return (
		<>
			<Heading eyebrow="02 / OPEN A CAPSULE" title="For when the time comes.">
				Choose a shared capsule to see its switch, check the release status, and
				recover its contents.
			</Heading>
			<div className="two-column">
				<section className="card form-card">
					<label className="upload-zone large" htmlFor="open-file">
						<span className="step-icon">
							<Icon name="unlock" />
						</span>
						<strong>
							{loading ? "Checking capsule…" : "Choose a sealed capsule"}
						</strong>
						<span>Select a .ckdutch.json file</span>
						<input
							id="open-file"
							type="file"
							accept=".json,.ckdutch.json"
							disabled={s.busy}
							onChange={(e) => {
								void read(e.target.files?.[0]);
								e.target.value = "";
							}}
						/>
					</label>
					{capsule && (
						<>
							<div className="capsule-details">
								<span className="eyebrow">CAPSULE DETAILS</span>
								<h2>{capsule.filename}</h2>
								<dl>
									<dt>Switch account</dt>
									<dd>{capsule.contract}</dd>
									<dt>Protection</dt>
									<dd>AES-256-GCM</dd>
									<dt>Release status</dt>
									<dd>
										{!status
											? "Checking contract…"
											: status.key_is_public
												? "Key is releasable"
												: status.challenge_deadline_ms != null
													? "Challenge in progress"
													: "Protected — no active challenge"}
									</dd>
								</dl>
							</div>
							{s.statusError && <p className="field-error">{s.statusError}</p>}
							<button
								className="button primary"
								disabled={
									s.busy ||
									!s.wallet ||
									!status ||
									(!status.key_is_public && s.wallet !== status.owner)
								}
								onClick={open}
							>
								<Icon name="unlock" />
								{s.busy ? "Recovering…" : "Recover key & open"}
							</button>
							<p className="hint">
								The owner can open a capsule at any time. Everyone else must
								wait for an unanswered challenge to expire and connect an
								account to pay transaction gas.
							</p>
						</>
					)}
					{plaintext && capsule && (
						<div className="recovered">
							<h3>Your capsule is open.</h3>
							<button
								className="button primary"
								onClick={() =>
									download(
										capsule.filename,
										plaintext,
										"application/octet-stream",
									)
								}
							>
								<Icon name="download" />
								Download recovered file
							</button>
							<button className="text-button" onClick={clear}>
								Clear recovered contents
							</button>
						</div>
					)}
				</section>
				<aside className="info-column">
					<div className="info-card">
						<span className="eyebrow">A MOMENT OF CARE</span>
						<h3>There’s time to respond.</h3>
						<p>
							A capsule stays protected until someone starts a challenge and its
							response window passes without a check-in.
						</p>
						<p>
							The owner or any trusted friend can cancel a challenge. Once
							someone has recovered a key, a later check-in cannot take that key
							back.
						</p>
						<button
							className="text-button"
							disabled={s.busy}
							onClick={() => s.setPage("Overview")}
						>
							View switch & challenge →
						</button>
					</div>
				</aside>
			</div>
		</>
	);
}

function Setup() {
	const s = useApp();
	const [global, setGlobal] = useState(chain.GLOBAL_CONTRACT);
	const [delay, setDelay] = useState("10080");
	const [friends, setFriends] = useState("");
	const [friend, setFriend] = useState("");
	const [acknowledged, setAcknowledged] = useState(false);
	useEffect(() => setAcknowledged(false), [s.wallet]);
	const create = () =>
		void s.run(async () => {
			const owner = s.wallet;
			if (!owner) throw new Error("Connect an account first");
			await chain.setupSwitch(
				global,
				Number(delay) * 60_000,
				friends
					.split(/[,\n]/)
					.map((v) => v.trim())
					.filter(Boolean)
					.map(chain.accountId),
				owner,
				s.onHash,
			);
			s.selectAccount(owner);
			s.setPage("Overview");
			return "Your switch is ready. You can now seal your first capsule.";
		});
	const manage = (method: string, success: string) => {
		try {
			s.call(method, { friend: chain.accountId(friend) }, success);
		} catch (e) {
			s.notify(errorMessage(e), true);
		}
	};
	const owner =
		!!s.status && s.wallet === s.status.owner && s.wallet === s.account;
	return (
		<>
			<Heading
				eyebrow="03 / SET UP YOUR SWITCH"
				title="Make a plan. Find some calm."
			>
				Your own NEAR account. Your own response window. A little preparation
				for the unknown.
			</Heading>
			<div className="two-column">
				<section className="card form-card">
					<div className="card-heading">
						<span className="step-icon">
							<Icon name="settings" />
						</span>
						<div>
							<h2>Create your switch</h2>
							<p>Use a fresh, dedicated testnet account.</p>
						</div>
					</div>
					<label>Owner account</label>
					<div className="readonly-field">
						{s.wallet ?? "Connect an account first"}
					</div>
					<label htmlFor="global-contract">Global contract</label>
					<input
						id="global-contract"
						value={global}
						disabled={s.busy}
						onChange={(e) => setGlobal(e.target.value)}
					/>
					<p className="hint">
						Meteor currently blocks attaching a shared contract. Creating a new
						switch requires wallet support for this action.
					</p>
					<label htmlFor="response-window">Response window</label>
					<select
						id="response-window"
						value={delay}
						disabled={s.busy}
						onChange={(e) => setDelay(e.target.value)}
					>
						<option value="1">1 minute · test only</option>
						<option value="60">1 hour</option>
						<option value="1440">24 hours</option>
						<option value="10080">7 days</option>
						<option value="43200">30 days</option>
					</select>
					<p className="hint">
						The timer starts when someone challenges, not when you create the
						switch.
					</p>
					<label htmlFor="trusted-friends">
						Trusted friends <span className="optional">optional</span>
					</label>
					<textarea
						id="trusted-friends"
						rows={3}
						placeholder="friend.testnet, another-friend.testnet"
						value={friends}
						disabled={s.busy}
						onChange={(e) => setFriends(e.target.value)}
					/>
					<p className="hint">
						These accounts can cancel a challenge on your behalf. Separate
						accounts with commas or new lines.
					</p>
					<label className="checkbox">
						<input
							type="checkbox"
							checked={acknowledged}
							disabled={s.busy}
							onChange={(e) => setAcknowledged(e.target.checked)}
						/>
						<span>
							I understand the global publisher can update the code, and the
							response window cannot be changed after setup.
						</span>
					</label>
					<button
						className="button primary"
						disabled={s.busy || !s.wallet || !acknowledged}
						onClick={create}
					>
						Create switch
						<Icon name="arrow" />
					</button>
				</section>
				<aside className="info-column">
					<div className="info-card">
						<span className="eyebrow">START WITH AN ACCOUNT</span>
						<h3>A home for your switch.</h3>
						<p>
							Create and fund a dedicated testnet account in Meteor, then
							connect it here. Your switch belongs to that account, and Meteor
							will ask you to approve setup and each action.
						</p>
						<button
							className="button secondary"
							disabled={s.busy}
							onClick={s.showConnect}
						>
							Manage account
							<Icon name="arrow" />
						</button>
					</div>
					<div className="info-card">
						<span className="eyebrow">ALREADY SET UP?</span>
						<h3>Keep your circle up to date.</h3>
						<p>
							Connect as the owner and select your switch below to add or remove
							a trusted friend.
						</p>
						<AccountPicker />
						{s.statusError && <p className="field-error">{s.statusError}</p>}
						<label htmlFor="friend-account">Friend’s account</label>
						<input
							id="friend-account"
							placeholder="friend.testnet"
							value={friend}
							disabled={s.busy}
							onChange={(e) => setFriend(e.target.value)}
						/>
						<div className="button-row">
							<button
								className="button secondary"
								disabled={s.busy || !owner}
								onClick={() => manage("add_friend", "Trusted friend added.")}
							>
								Add friend
							</button>
							<button
								className="text-button"
								disabled={s.busy || !owner}
								onClick={() =>
									manage("remove_friend", "Trusted friend removed.")
								}
							>
								Remove
							</button>
						</div>
					</div>
				</aside>
			</div>
		</>
	);
}

function Connect({ close }: { close: () => void }) {
	const s = useApp();
	const [connecting, setConnecting] = useState(false);
	const [error, setError] = useState("");
	const dialog = useRef<HTMLElement>(null);
	useEffect(() => {
		const previous = document.activeElement as HTMLElement | null;
		dialog.current?.focus();
		return () => previous?.focus();
	}, []);
	const connect = async () => {
		setConnecting(true);
		setError("");
		try {
			await chain.connect();
			close();
			s.notify(
				"Connected through Meteor. Approve each transaction in your wallet.",
			);
		} catch (e) {
			setError(errorMessage(e));
		} finally {
			setConnecting(false);
		}
	};
	const disconnect = async () => {
		setConnecting(true);
		setError("");
		try {
			await chain.disconnect();
			s.notify("Wallet disconnected.");
		} catch (e) {
			setError(errorMessage(e));
		} finally {
			setConnecting(false);
		}
	};
	return (
		<div className="modal-backdrop">
			<section
				ref={dialog}
				className="modal"
				role="dialog"
				aria-modal="true"
				aria-labelledby="connect-title"
				tabIndex={-1}
				onKeyDown={(e) => {
					if (e.key === "Escape" && !connecting) close();
					if (e.key === "Tab") {
						const items = Array.from(
							dialog.current?.querySelectorAll<HTMLElement>(
								"button:not(:disabled), a[href], input:not(:disabled)",
							) ?? [],
						);
						const first = items[0],
							last = items.at(-1);
						if (
							e.shiftKey &&
							(document.activeElement === first ||
								document.activeElement === dialog.current)
						) {
							e.preventDefault();
							last?.focus();
						} else if (!e.shiftKey && document.activeElement === last) {
							e.preventDefault();
							first?.focus();
						}
					}
				}}
			>
				<div className="modal-heading">
					<span className="step-icon">
						<Icon name="wallet" />
					</span>
					<button
						className="close-button"
						aria-label="Close account dialog"
						disabled={connecting}
						onClick={close}
					>
						×
					</button>
				</div>
				<h2 id="connect-title">Your account, your switch.</h2>
				<p>
					Connect your Meteor wallet on NEAR Testnet. Your wallet holds your
					account keys and asks you to approve each transaction.
				</p>
				{error && (
					<p className="field-error" role="alert">
						{error}
					</p>
				)}
				{s.wallet && (
					<div className="connected-account">
						<span className="dot" />
						<strong>{s.wallet}</strong>
						<button
							className="text-button"
							disabled={connecting}
							onClick={() => void disconnect()}
						>
							Disconnect
						</button>
					</div>
				)}
				<button
					className="button primary full"
					disabled={connecting}
					onClick={() => void connect()}
				>
					{connecting
						? "Waiting for Meteor…"
						: s.wallet
							? "Choose account in Meteor"
							: "Connect Meteor"}
					<Icon name="arrow" />
				</button>
				<p className="hint">
					Use a dedicated, funded testnet account for your switch. To change
					accounts, choose it in Meteor and reconnect here.
				</p>
			</section>
		</div>
	);
}
