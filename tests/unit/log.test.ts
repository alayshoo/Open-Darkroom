// tests/unit/log.test.ts

import { describe as suite, expect, it, vi } from "vitest";
import { describe, limitRepeats, type Logger } from "$lib/utils/log";

function errorWithStack(message: string, stack: string) {
    const error = new Error(message);
    error.stack = stack;
    return error;
}

suite("describe", () => {
    it("drops V8's message header and keeps every frame", () => {
        const error = errorWithStack("boom", "Error: boom\n    at a (x.js:1:1)\n    at b (x.js:2:1)");
        expect(describe(error)).toBe("Error: boom\n    at a (x.js:1:1)\n    at b (x.js:2:1)");
    });

    it("keeps WebKit's top frame when the message is empty", () => {
        const error = errorWithStack("", "a@x.js:1:1\nb@x.js:2:1");
        expect(describe(error)).toBe("Error: \na@x.js:1:1\nb@x.js:2:1");
    });

    it("cuts a long stack and says how much was left out", () => {
        const frames = Array.from({ length: 20 }, (_, i) => `f${i}@x.js:${i}:1`);
        const lines = describe(errorWithStack("deep", frames.join("\n"))).split("\n");
        expect(lines).toHaveLength(14);
        expect(lines[1]).toBe("f0@x.js:0:1");
        expect(lines[13]).toBe("… 8 more frames");
    });
});

suite("limitRepeats", () => {
    function recorder() {
        const error = vi.fn();
        const logger: Logger = { error, warn: vi.fn(), info: vi.fn(), debug: vi.fn() };
        return { logger, error };
    }

    it("logs the first copies of a message and then only counts it", () => {
        const { logger, error } = recorder();
        const report = limitRepeats(logger, 3);
        for (let i = 0; i < 10; i++) report("same");
        report("other");
        expect(error.mock.calls.map((c) => c[0])).toEqual([
            "same",
            "same",
            "same (further repeats are not logged)",
            "other",
        ]);
    });

    it("counts copies by key when one is given", () => {
        const { logger, error } = recorder();
        const report = limitRepeats(logger, 1);
        report("Unhandled rejection", "a", "a");
        report("Unhandled rejection", "b", "b");
        report("Unhandled rejection", "a", "a");
        expect(error.mock.calls).toEqual([
            ["Unhandled rejection (further repeats are not logged)", "a"],
            ["Unhandled rejection (further repeats are not logged)", "b"],
        ]);
    });

    it("stops taking new messages past the distinct limit, saying so once", () => {
        const { logger, error } = recorder();
        const report = limitRepeats(logger, 3, 2);
        for (let i = 0; i < 5; i++) report(`message ${i}`);
        expect(error.mock.calls.map((c) => c[0])).toEqual([
            "message 0",
            "message 1",
            "More than 2 distinct errors, the rest are not logged",
        ]);
    });
});
