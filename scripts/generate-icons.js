import { spawnSync } from 'node:child_process';
import { openSync, readSync, closeSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve, relative } from 'node:path';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const source = resolve(root, 'resources/icon.png');
const output = resolve(root, 'src-tauri/icons');
const EXPECTED = 1024;

function readPngSize(path) {
	const header = Buffer.alloc(24);
	let fd;
	try {
		fd = openSync(path, 'r');
	} catch {
		fail(`Source icon not found at ${relative(root, path)}`);
	}
	const read = readSync(fd, header, 0, 24, 0);
	closeSync(fd);

	if (read < 24 || header.toString('ascii', 1, 4) !== 'PNG') {
		fail(`${relative(root, path)} is not a valid PNG file`);
	}
	return { width: header.readUInt32BE(16), height: header.readUInt32BE(20) };
}

function fail(message) {
	console.error(`\x1b[31merror\x1b[0m ${message}`);
	process.exit(1);
}

const { width, height } = readPngSize(source);
if (width !== height) {
	fail(`Source icon must be square, got ${width}x${height}`);
}
if (width < EXPECTED) {
	fail(`Source icon must be at least ${EXPECTED}x${EXPECTED}, got ${width}x${height}`);
}
if (width > EXPECTED) {
	console.warn(`\x1b[33mwarn\x1b[0m  source is ${width}x${height}, ${EXPECTED}x${EXPECTED} is recommended`);
}

console.log(`Generating icons from ${relative(root, source)} into ${relative(root, output)}`);

// Relative paths: the root path may contain spaces, which the Windows shell would split.
const result = spawnSync(
	'pnpm',
	['exec', 'tauri', 'icon', relative(root, source), '--output', relative(root, output)],
	{ stdio: 'inherit', cwd: root, shell: process.platform === 'win32' }
);

if (result.error) {
	fail(result.error.message);
}
process.exit(result.status ?? 1);
