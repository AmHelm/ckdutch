const paths: Record<string, string> = {
	sail: "M12 3v13H3L12 3ZM14 6l7 10h-7V6ZM3 19h18l-4 3H7l-4-3Z",
	grid: "M3 3h7v7H3V3Zm11 0h7v7h-7V3ZM3 14h7v7H3v-7Zm11 0h7v7h-7v-7Z",
	lock: "M6 10V7a6 6 0 0 1 12 0v3M5 10h14v11H5V10Zm7 5v2",
	unlock: "M7 10V7a5 5 0 0 1 9-3M5 10h14v11H5V10Zm7 5v2",
	shield: "M12 3 3 6v6c0 5 9 9 9 9s9-4 9-9V6l-9-3Zm-4 9 3 3 5-6",
	arrow: "M4 12h16m-6-6 6 6-6 6",
	clock: "M12 7v5l3 2M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0",
	check: "m5 12 4 4L19 6",
	sun: "M12 2v2m0 16v2M2 12h2m16 0h2M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2M17 12a5 5 0 1 1-10 0 5 5 0 0 1 10 0",
	upload: "M12 16V3m-5 5 5-5 5 5M4 16v5h16v-5",
	download: "M12 3v13m-5-5 5 5 5-5M4 16v5h16v-5",
	wallet: "M3 5h16v4H3V5Zm0 4v12h18V9H3Zm12 4h6v4h-6v-4",
};

export function Icon({ name }: { name: string }) {
	return (
		<svg
			width="20"
			height="20"
			viewBox="0 0 24 24"
			fill="none"
			stroke="currentColor"
			strokeWidth="1.6"
			strokeLinecap="round"
			strokeLinejoin="round"
			aria-hidden="true"
		>
			<path d={paths[name] ?? "M4 7h16M4 17h16M8 4v6m8 4v6"} />
		</svg>
	);
}
