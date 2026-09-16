// demo/scripts/serve-demo.mjs [port] — static server for demo/ with HTTP Range
// support, so reviewers can seek in the video behind the quick tunnel.
import { createServer } from "node:http";
import { createReadStream, statSync } from "node:fs";
import { extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
const port = Number(process.argv[2] ?? 8099);
const types = {
	".webm": "video/webm",
	".mkv": "video/x-matroska",
	".json": "application/json",
	".md": "text/markdown",
	".js": "text/javascript",
	".sh": "text/x-sh",
	".ffmetadata": "text/plain",
};

createServer((req, res) => {
	const url = decodeURIComponent(new URL(req.url, "http://x").pathname);
	const path = normalize(join(root, url));
	if (!path.startsWith(root)) {
		res.writeHead(403).end();
		return;
	}
	let stat;
	try {
		stat = statSync(path);
	} catch {
		res.writeHead(404).end("not found");
		return;
	}
	if (stat.isDirectory()) {
		res.writeHead(302, { Location: "/kubernetes-native.webm" }).end();
		return;
	}
	const headers = {
		"Content-Type": types[extname(path)] ?? "application/octet-stream",
		"Accept-Ranges": "bytes",
	};
	const range = /^bytes=(\d*)-(\d*)$/.exec(req.headers.range ?? "");
	if (range) {
		const start = range[1] ? Number(range[1]) : 0;
		const end = range[2] ? Number(range[2]) : stat.size - 1;
		if (start >= stat.size || end >= stat.size || start > end) {
			res.writeHead(416, { "Content-Range": `bytes */${stat.size}` }).end();
			return;
		}
		res.writeHead(206, {
			...headers,
			"Content-Range": `bytes ${start}-${end}/${stat.size}`,
			"Content-Length": end - start + 1,
		});
		createReadStream(path, { start, end }).pipe(res);
		return;
	}
	res.writeHead(200, { ...headers, "Content-Length": stat.size });
	createReadStream(path).pipe(res);
}).listen(port, "127.0.0.1", () => console.log(`demo server on http://127.0.0.1:${port}`));
