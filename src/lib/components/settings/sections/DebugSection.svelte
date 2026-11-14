<!-- lib/components/settings/sections/DebugSection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import SettingsRow from "../SettingsRow.svelte";
    import SettingsToggle from "../SettingsToggle.svelte";
    import GlassButton from "$lib/components/inputs/GlassButton.svelte";

    import { getContext, onMount } from "svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import { debugStats } from "$lib/gpu/debugStats.svelte";
    import { colorSpaceDebug } from "$lib/gpu/colorSpaceDebug.svelte";
    import { WORKING_FORMAT, type GPUSession } from "$lib/types/gpuTypes";
    import { createLogger } from "$lib/utils/log";
    import {
        copyDiagnostics,
        loggingStatus,
        revealLogFile,
        type LoggingStatus,
    } from "$lib/utils/diagnostics";

    const log = createLogger("settings");

    // The layout shares a getter rather than the session itself, so reading it
    // through a derived keeps the panel right even when it is opened before the
    // device has finished initialising.
    const session = getContext<() => GPUSession | null>("gpu");
    let gpu = $derived(session());

    let logs = $state<LoggingStatus | null>(null);
    let copied = $state(false);
    let copiedTimer: ReturnType<typeof setTimeout> | undefined;

    onMount(() => {
        loggingStatus()
            .then((status) => (logs = status))
            .catch((e) => log.warn("Failed to read the logging status", e));
        return () => clearTimeout(copiedTimer);
    });

    function showLogFile() {
        revealLogFile().catch((e) => log.error("Failed to show the log file", e));
    }

    async function copyReport() {
        try {
            await copyDiagnostics(gpu);
        } catch (e) {
            log.error("Failed to copy the diagnostics", e);
            return;
        }
        copied = true;
        clearTimeout(copiedTimer);
        copiedTimer = setTimeout(() => (copied = false), 1500);
    }

    /* The master switch owns what it switches off: the overlays keep their own
       state, and the histogram's is invisible once the tools are hidden. */
    function toggleDebugTools() {
        settings.debugTools = !settings.debugTools;
        debugStats.enabled = settings.debugTools;
        if (!settings.debugTools) {
            debugStats.histogramEnabled = true;
            colorSpaceDebug.enabled = false;
        }
        debugStats.clear();
    }

    // Limits worth reading: the ones this app's textures, buffers and histogram
    // workgroups are actually sized against.
    const LIMIT_KEYS = [
        "maxTextureDimension2D",
        "maxBufferSize",
        "maxStorageBufferBindingSize",
        "maxUniformBufferBindingSize",
        "maxComputeInvocationsPerWorkgroup",
        "maxComputeWorkgroupStorageSize",
        "maxSampledTexturesPerShaderStage",
        "maxStorageTexturesPerShaderStage",
    ] as const;

    function formatLimit(value: number) {
        if (value >= 1024 * 1024) {
            return `${(value / (1024 * 1024)).toFixed(0)} MiB`;
        }
        return value.toLocaleString();
    }

    /** Turns maxTextureDimension2D into "max texture dimension 2D". */
    function humanise(key: string) {
        return key
            .replace(/([a-z])([A-Z])/g, "$1 $2")
            .replace(/([A-Z])([A-Z][a-z])/g, "$1 $2")
            .toLowerCase()
            .replace(/^./, (c) => c.toUpperCase());
    }

    /* Every GPUAdapterInfo string is allowed to come back empty. Chromium masks
       device and description unless the WebView is started with WebGPU
       developer features — see additionalBrowserArgs in tauri.conf.json — so an
       empty string here is the browser declining, not a missing value. */
    function reported(value: string | undefined) {
        if (value === undefined) return "unavailable";
        return value === "" ? "withheld" : value;
    }

    let adapter = $derived(gpu?.adapterInfo ?? null);

    let deviceFeatures = $derived(
        gpu ? [...gpu.device.features].sort().join(", ") || "none" : "—",
    );
</script>

<div class="section flex flex-col gap-4">
    <SettingsGroup
        note="Off in a release build until turned on here."
    >
        <SettingsRow
            label="Debug tools"
            description="Frame stats overlay (F1), the live histogram switch (F2) and the colour space panel (F3)."
        >
            <SettingsToggle
                checked={settings.debugTools}
                label="Toggle debug tools"
                onclick={toggleDebugTools}
            />
        </SettingsRow>
    </SettingsGroup>

    <SettingsGroup
        title="Logs"
        note={logs ? `Kept in ${logs.folder}` : undefined}
    >
        {#if logs?.previousSessionUnclean}
            <SettingsRow
                label="Last session"
                description="It did not shut down cleanly. The diagnostics include its log."
                value="ended unexpectedly"
            />
        {/if}
        <SettingsRow
            label="Log file"
            description="Shows this session's log in the file manager."
        >
            <GlassButton label="Show log file" size={24} onclick={showLogFile}>
                <svg width="24" height="24" viewBox="0 0 24 24">
                    <path
                        d="M5.5 8v8.5h13V9.8h-6.3L10.7 8z"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.2"
                        stroke-linejoin="round"
                    />
                </svg>
            </GlassButton>
        </SettingsRow>
        <SettingsRow
            label="Copy diagnostics"
            description="System details and the recent log, ready to paste into a bug report."
        >
            <GlassButton
                label={copied ? "Diagnostics copied" : "Copy diagnostics"}
                size={24}
                onclick={copyReport}
            >
                <svg width="24" height="24" viewBox="0 0 24 24">
                    <path
                        d={copied
                            ? "M8.3 12.4l2.5 2.5 4.9-5"
                            : "M9.5 9.5h7v7h-7zM7.5 14.5v-7h7"}
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.2"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                    />
                </svg>
            </GlassButton>
        </SettingsRow>
        <SettingsRow
            label="Verbose logging"
            description="Also records timings and step-by-step detail. Turn on when asked for a detailed log."
        >
            <SettingsToggle
                checked={settings.verboseLogging}
                label="Toggle verbose logging"
                onclick={() => (settings.verboseLogging = !settings.verboseLogging)}
            />
        </SettingsRow>
    </SettingsGroup>

    {#if !gpu}
        <SettingsGroup title="WebGPU">
            <SettingsRow label="Status" value="initialising" />
        </SettingsGroup>
    {:else}
        <SettingsGroup title="Adapter">
            <SettingsRow label="Vendor" value={reported(adapter?.vendor)} />
            <SettingsRow
                label="Architecture"
                value={reported(adapter?.architecture)}
            />
            <SettingsRow label="Device" value={reported(adapter?.device)} />
            <SettingsRow
                label="Description"
                value={reported(adapter?.description)}
            />
            <SettingsRow
                label="Fallback adapter"
                value={adapter?.isFallbackAdapter ? "yes" : "no"}
            />
        </SettingsGroup>

        <SettingsGroup title="Device">
            <SettingsRow label="Canvas format" value={gpu.format} />
            <SettingsRow label="Working format" value={WORKING_FORMAT} />
            <SettingsRow
                label="Timestamp queries"
                description="Per-pass GPU timings in the frame stats."
                value={gpu.canTimestamp ? "enabled" : "unavailable"}
            />
            <SettingsRow label="Enabled features" value={deviceFeatures} />
            <SettingsRow
                label="Adapter features"
                value={gpu.adapterFeatures.join(", ") || "none"}
            />
        </SettingsGroup>

        <SettingsGroup title="Limits">
            {#each LIMIT_KEYS as key (key)}
                <SettingsRow
                    label={humanise(key)}
                    value={formatLimit(gpu.device.limits[key])}
                />
            {/each}
        </SettingsGroup>
    {/if}
</div>
