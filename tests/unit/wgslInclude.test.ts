// tests/unit/wgslInclude.test.ts
//
// The include directive has a second resolver in src-tauri/build.rs, and the
// two can only agree while the grammar stays as narrow as it is. These pin the
// grammar, and the real shaders are checked against it at the end.

import { describe, expect, it } from "vitest";
import { mkdtempSync, writeFileSync, readdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { resolveIncludes } from "../../scripts/wgslInclude.js";

const SHADERS = fileURLToPath(new URL("../../src/lib/gpu/shaders", import.meta.url));

function root(files: Record<string, string>): string {
    const dir = mkdtempSync(path.join(tmpdir(), "wgsl-"));
    for (const [name, body] of Object.entries(files)) {
        writeFileSync(path.join(dir, name), body, "utf8");
    }
    return dir;
}

describe("resolveIncludes", () => {
    it("substitutes a chunk for its directive", () => {
        const dir = root({ "chunk.wgsl": "const A = 1;\n" });
        const { code } = resolveIncludes('//!include "chunk.wgsl"\nconst B = 2;', dir, "t.wgsl");

        expect(code).toBe("const A = 1;\nconst B = 2;");
    });

    it("reports which chunks it pulled in", () => {
        const dir = root({ "a.wgsl": "const A = 1;\n", "b.wgsl": "const B = 2;\n" });
        const { included } = resolveIncludes(
            '//!include "a.wgsl"\n//!include "b.wgsl"',
            dir,
            "t.wgsl",
        );

        expect(included.map((p) => path.basename(p))).toEqual(["a.wgsl", "b.wgsl"]);
    });

    it("leaves a file with no directives untouched", () => {
        const source = "const A = 1;\n// a comment\n";
        expect(resolveIncludes(source, root({}), "t.wgsl").code).toBe(source);
    });

    it("accepts a leading indent", () => {
        const dir = root({ "chunk.wgsl": "const A = 1;\n" });
        expect(resolveIncludes('    //!include "chunk.wgsl"', dir, "t.wgsl").code).toBe(
            "const A = 1;",
        );
    });

    it("rejects a chunk that includes another chunk", () => {
        const dir = root({ "outer.wgsl": '//!include "inner.wgsl"\n', "inner.wgsl": "" });
        expect(() => resolveIncludes('//!include "outer.wgsl"', dir, "t.wgsl")).toThrow(
            /may not include another chunk/,
        );
    });

    it("rejects a missing chunk", () => {
        expect(() => resolveIncludes('//!include "nope.wgsl"', root({}), "t.wgsl")).toThrow(
            /no such chunk/,
        );
    });

    it("rejects a path escaping the shaders root", () => {
        expect(() =>
            resolveIncludes('//!include "../secret.wgsl"', root({}), "t.wgsl"),
        ).toThrow(/escapes the shaders root/);
    });

    it("rejects a malformed directive rather than ignoring it", () => {
        // Silently passing this through would leave WGSL a comment where a
        // declaration belongs, and the failure would surface as a compile
        // error in a file that looks correct.
        expect(() => resolveIncludes("//!include chunk.wgsl", root({}), "t.wgsl")).toThrow(
            /malformed include/,
        );
    });
});

describe("the real shaders", () => {
    const names = readdirSync(SHADERS).filter((f) => f.endsWith(".wgsl"));

    it("has shaders to check", () => {
        expect(names.length).toBeGreaterThan(0);
    });

    it("every shader resolves, and its chunks are flat", () => {
        for (const name of names) {
            const source = readFileSync(path.join(SHADERS, name), "utf8");
            const { code } = resolveIncludes(source, SHADERS, name);
            expect(code).not.toContain("//!include");
        }
    });
});
