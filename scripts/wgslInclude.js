// scripts/wgslInclude.js
//
// WGSL has no `#include`, so shared shader code is pulled in at build time by a
// directive of our own:
//
//     //!include "chunks/tone.wgsl"
//
// The grammar is deliberately minimal — one include per line, a path relative
// to the shaders root, and chunks may not include chunks. A second resolver
// lives in src-tauri/build.rs for the export path, which never sees vite, and
// the two can only stay honest if there is nothing to disagree about.

import { readFileSync } from "node:fs";
import path from "node:path";

// A carriage return is allowed before the line end: a CRLF checkout split on
// a bare newline leaves one attached, while build.rs' `lines()` drops it for
// free — so without this the two resolvers disagree on every shader on Windows.
const DIRECTIVE = /^[ \t]*\/\/!include[ \t]+"([^"]+)"[ \t]*\r?$/;

/**
 * Resolve every directive in `source`, reporting each chunk it pulled in.
 *
 * @param {string} source
 * @param {string} shaderRoot
 * @param {string} origin  name used in error messages
 * @returns {{ code: string, included: string[] }}
 */
export function resolveIncludes(source, shaderRoot, origin) {
    /** @type {string[]} */
    const included = [];

    const out = source.split("\n").map((line) => {
        const match = DIRECTIVE.exec(line);
        if (!match) {
            if (line.includes("//!include")) {
                throw new Error(
                    `${origin}: malformed include — expected //!include "path", got: ${line.trim()}`,
                );
            }
            return line;
        }

        const chunk = path.resolve(shaderRoot, match[1]);
        if (!chunk.startsWith(path.resolve(shaderRoot) + path.sep)) {
            throw new Error(`${origin}: include escapes the shaders root: ${match[1]}`);
        }

        let text;
        try {
            text = readFileSync(chunk, "utf8");
        } catch {
            throw new Error(`${origin}: no such chunk: ${match[1]}`);
        }
        if (DIRECTIVE.test(text) || text.includes("//!include")) {
            throw new Error(`${match[1]}: a chunk may not include another chunk`);
        }

        included.push(chunk);
        return text.replace(/\r?\n$/, "");
    });

    return { code: out.join("\n"), included };
}

/**
 * Vite plugin resolving the directive in `.wgsl` imports.
 *
 * It loads the shader rather than transforming it: vite's own asset plugin
 * answers `?raw` in its load hook, and by then the WGSL is already wrapped in
 * a JS string literal. Taking the load keeps this working on the text.
 *
 * @param {{ shaderRoot: string }} options
 */
export function wgslInclude({ shaderRoot }) {
    /** @type {import("vite").Plugin} */
    const plugin = {
        name: "wgsl-include",
        enforce: "pre",
        load(id) {
            const file = id.split("?")[0];
            if (!file.endsWith(".wgsl")) return null;

            const source = readFileSync(file, "utf8");
            const { code, included } = resolveIncludes(
                source,
                shaderRoot,
                path.basename(file),
            );
            // Editing a chunk has to invalidate every shader that pulled it in.
            for (const chunk of included) this.addWatchFile(chunk);

            return `export default ${JSON.stringify(code)};`;
        },
    };
    return plugin;
}
