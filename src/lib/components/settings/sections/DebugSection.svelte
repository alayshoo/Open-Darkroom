<!-- lib/components/settings/sections/DebugSection.svelte -->
<script lang="ts">
    import SettingsGroup from "../SettingsGroup.svelte";
    import SettingsRow from "../SettingsRow.svelte";
    import SettingsToggle from "../SettingsToggle.svelte";

    import { getContext } from "svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import { debugStats } from "$lib/gpu/debugStats.svelte";
    import { WORKING_FORMAT, type GPUSession } from "$lib/types/gpuTypes";

    // The layout shares a getter rather than the session itself, so reading it
    // through a derived keeps the panel right even when it is opened before the
    // device has finished initialising.
    const session = getContext<() => GPUSession | null>("gpu");
    let gpu = $derived(session());

    /* The master switch owns what it switches off: the overlays keep their own
       state, and the histogram's is invisible once the tools are hidden. */
    function toggleDebugTools() {
        settings.debugTools = !settings.debugTools;
        debugStats.enabled = settings.debugTools;
        if (!settings.debugTools) debugStats.histogramEnabled = true;
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
            description="Frame stats overlay (F1) and the live histogram switch (F2)."
        >
            <SettingsToggle
                checked={settings.debugTools}
                label="Toggle debug tools"
                onclick={toggleDebugTools}
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
