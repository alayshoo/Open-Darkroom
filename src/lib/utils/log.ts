// $lib/utils/log.ts

/**
 * The frontend's side of the app log. Every line goes to the devtools console
 * and, inside Tauri, to the backend's log file under `webview::<scope>`. See
 * the Logging section of AGENTS.md for what belongs at which level.
 */

import { invoke, isTauri } from "@tauri-apps/api/core";

type Level = "error" | "warn" | "info" | "debug";

// The log plugin's own numbering.
const PLUGIN_LEVEL: Record<Level, number> = { debug: 2, info: 3, warn: 4, error: 5 };

/** Debug lines cost an IPC round trip each, so they are only sent when kept. */
let verbose = import.meta.env.DEV;

export function setVerboseLogging(on: boolean) {
    verbose = on || import.meta.env.DEV;
}

/** Past this the frames are the framework's, not the app's. */
const STACK_FRAMES = 12;

/** An error as text a log line can carry: its message, and the top of its stack. */
export function describe(error: unknown): string {
    if (error instanceof Error) {
        const frames = (error.stack ?? "").split("\n").filter((line) => line.trim());
        // V8 opens the stack with the message, ahead of its "at" frames; WebKit does not.
        const isFrame = (line: string) => line.trimStart().startsWith("at ");
        if (frames.length > 1 && !isFrame(frames[0]) && frames.slice(1).some(isFrame)) frames.shift();
        const kept = frames.slice(0, STACK_FRAMES);
        if (frames.length > kept.length) kept.push(`… ${frames.length - kept.length} more frames`);
        return [`${error.name}: ${error.message}`, ...kept].join("\n");
    }
    if (typeof error === "string") return error;
    if (error && typeof error === "object" && "message" in error) return String(error.message);
    try {
        return JSON.stringify(error);
    } catch {
        return String(error);
    }
}

function write(scope: string, level: Level, message: string, error?: unknown) {
    const line = error === undefined ? message : `${message}: ${describe(error)}`;
    console[level](`[${scope}] ${line}`);
    if (level === "debug" && !verbose) return;
    if (!isTauri()) return;
    // Called directly rather than through the plugin's helpers, which take the
    // location from the stack, and a minified bundle's stack names nothing.
    invoke("plugin:log|log", { level: PLUGIN_LEVEL[level], message: line, location: scope }).catch(() => {});
}

export interface Logger {
    error(message: string, error?: unknown): void;
    warn(message: string, error?: unknown): void;
    info(message: string): void;
    debug(message: string): void;
}

/** A logger whose lines are tagged with `scope`, e.g. `createLogger("export")`. */
export function createLogger(scope: string): Logger {
    return {
        error: (message, error) => write(scope, "error", message, error),
        warn: (message, error) => write(scope, "warn", message, error),
        info: (message) => write(scope, "info", message),
        debug: (message) => write(scope, "debug", message),
    };
}

/**
 * Logs the first few copies of each distinct error and only counts the rest,
 * so one repeated every frame cannot fill the log and rotate older files out.
 */
export function limitRepeats(logger: Logger, repeats = 3, distinct = 100) {
    const seen = new Map<string, number>();
    let saturated = false;
    /** `key` tells copies apart when `message` alone does not. */
    return (message: string, error?: unknown, key = message) => {
        if (!seen.has(key) && seen.size >= distinct) {
            if (!saturated) logger.error(`More than ${distinct} distinct errors, the rest are not logged`);
            saturated = true;
            return;
        }
        const count = (seen.get(key) ?? 0) + 1;
        seen.set(key, count);
        if (count > repeats) return;
        logger.error(count === repeats ? `${message} (further repeats are not logged)` : message, error);
    };
}

/** Records what nothing else caught: thrown errors and rejected promises. Returns their removal. */
export function installErrorHandlers(): () => void {
    const report = limitRepeats(createLogger("uncaught"));
    const controller = new AbortController();
    window.addEventListener(
        "error",
        (event) => {
            const where = event.filename ? ` at ${event.filename}:${event.lineno}:${event.colno}` : "";
            report(`Uncaught error${where}`, event.error ?? event.message, `${event.message}${where}`);
        },
        { signal: controller.signal },
    );
    window.addEventListener(
        "unhandledrejection",
        (event) => {
            const reason = describe(event.reason).split("\n")[0];
            report("Unhandled rejection", event.reason, reason);
        },
        { signal: controller.signal },
    );
    return () => controller.abort();
}
