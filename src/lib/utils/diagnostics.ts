/* $lib/utils/diagnostics.ts */

import { invoke, isTauri } from "@tauri-apps/api/core";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { describeAdapter } from "$lib/gpu/gpuInit";
import type { GPUSession } from "$lib/types/gpuTypes";
import { setVerboseLogging } from "./log";

export interface LoggingStatus {
    /** With the home folder shown as `~`. */
    folder: string;
    /** A session before this one ended without shutting down. */
    previousSessionUnclean: boolean;
}

export function loggingStatus(): Promise<LoggingStatus> {
    return invoke("logging_status");
}

/** Shows the current log file in the system's file manager. */
export function revealLogFile(): Promise<void> {
    return invoke("reveal_log_file");
}

export async function applyVerboseLogging(on: boolean) {
    setVerboseLogging(on);
    if (isTauri()) await invoke("set_verbose_logging", { verbose: on });
}

/** Puts system details, the GPU and the recent log on the clipboard for a bug report. */
export async function copyDiagnostics(gpu: GPUSession | null) {
    const { summary, logs } = await invoke<{ summary: string; logs: string }>("diagnostics");
    const device = gpu?.device.limits;
    const gpuLines = gpu
        ? [
              `GPU: ${describeAdapter(gpu.adapterInfo)}`,
              `Canvas: ${gpu.format}, display ${gpu.displaySpace}`,
              `Max texture: ${device?.maxTextureDimension2D}px, max buffer ${Math.round((device?.maxBufferSize ?? 0) / 2 ** 20)} MiB`,
              `Device features: ${[...gpu.device.features].sort().join(", ") || "none"}`,
          ]
        : ["GPU: not initialised"];
    const screenLine = `Screen: ${screen.width}×${screen.height} at ${window.devicePixelRatio}x`;
    await writeText([summary, ...gpuLines, screenLine, "", logs].join("\n"));
}
