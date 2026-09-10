import { MAX_FILE_BYTES } from "./chain";

export async function readFile(file: File | undefined, limit = MAX_FILE_BYTES) {
	if (!file) throw new Error("Choose a file first");
	if (file.size > limit) throw new Error("File is too large");
	return new Uint8Array(await file.arrayBuffer());
}

export function duration(ms: number): string {
	const s = Math.ceil(Math.max(0, ms) / 1_000);
	if (s >= 86_400)
		return `${Math.floor(s / 86_400)}d ${Math.floor((s % 86_400) / 3_600)}h`;
	if (s >= 3_600)
		return `${Math.floor(s / 3_600)}h ${Math.floor((s % 3_600) / 60)}m`;
	if (s >= 60) return `${Math.floor(s / 60)}m ${s % 60}s`;
	return `${s}s`;
}
