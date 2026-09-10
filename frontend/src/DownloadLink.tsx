import { useEffect, useState, type ReactNode } from "react";

export function DownloadLink({ name, content, mime, children }: {
	name: string;
	content: string | Uint8Array;
	mime: string;
	children: ReactNode;
}) {
	const [url, setUrl] = useState<string>();
	useEffect(() => {
		const bytes = typeof content === "string" ? content : new Uint8Array(content);
		const next = URL.createObjectURL(new Blob([bytes], { type: mime }));
		setUrl(next);
		return () => URL.revokeObjectURL(next);
	}, [content, mime]);
	return <a className="button primary" href={url} download={name.replace(/[\\/\x00-\x1f\x7f]/g, "_")}>
		{children}
	</a>;
}
