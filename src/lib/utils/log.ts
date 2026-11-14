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
        // V8 opens the stack with the message, WebKit does not.
        if (frames[0]?.includes(error.message)) frames.shift();
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

const uncaught = createLogger("uncaught");

/** Records what nothing else caught: thrown errors and rejected promises. */
export function installErrorHandlers() {
    window.addEventListener("error", (event) => {
        const where = event.filename ? ` at ${event.filename}:${event.lineno}:${event.colno}` : "";
        uncaught.error(`${event.message}${where}`, event.error ?? undefined);
    });
    window.addEventListener("unhandledrejection", (event) => {
        uncaught.error("Unhandled rejection", event.reason);
    });
}
