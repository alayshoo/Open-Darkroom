<!-- lib/components/export/ExportFormatOptions.svelte -->
<script lang="ts">
    import { EXPORT_FORMATS, FORMAT_LABELS, type ExportSettings } from "$lib/types/exportSettings";
    import SingleValSliderInt from "$lib/components/inputs/values/SingleValSliderInt.svelte";

    let {
        settings = $bindable(),
        isRgb = true,
    }: {
        settings: ExportSettings;
        isRgb?: boolean;
    } = $props();

    const channels = $derived(isRgb ? "RGB" : "Greyscale");
</script>

{#snippet slider(label: string, shown: string, min: number, max: number, defaultValue: number, hints: [string, string], value: number, set: (v: number) => void, smooth = false)}
    <div class="setting flex flex-col gap-1.5">
        <div class="setting-header flex justify-between items-baseline">
            <span class="setting-label">{label}</span>
            <span class="setting-value">{shown}</span>
        </div>
        <SingleValSliderInt
            {min}
            {max}
            step={1}
            {defaultValue}
            bind:value={() => value, set}
            stepAnimation={!smooth}
            springy={!smooth}
        />
        <div class="setting-hints flex justify-between -mt-2">
            <span>{hints[0]}</span>
            <span>{hints[1]}</span>
        </div>
    </div>
{/snippet}

{#snippet pair(a: string, b: string, first: boolean, set: (first: boolean) => void)}
    <div class="pill-row grid grid-cols-2 gap-2">
        <button class="pill" class:active={first} aria-pressed={first} onclick={() => set(true)}>{a}</button>
        <button class="pill" class:active={!first} aria-pressed={!first} onclick={() => set(false)}>{b}</button>
    </div>
{/snippet}

<div class="options flex flex-col gap-4">
    <div class="pill-row grid grid-cols-5 gap-2" role="radiogroup" aria-label="Format">
        {#each EXPORT_FORMATS as format (format)}
            <button
                class="pill"
                class:active={settings.format === format}
                role="radio"
                aria-checked={settings.format === format}
                title={FORMAT_LABELS[format]}
                onclick={() => (settings.format = format)}
            >
                {format === "jxl" ? "JXL" : FORMAT_LABELS[format]}
            </button>
        {/each}
    </div>

    {#if settings.format === "jxl"}
        {@render pair("Lossless", "Lossy", settings.jxlLossless, (v) => (settings.jxlLossless = v))}
        {#if settings.jxlLossless}
            {@render slider(`${channels} bit depth`, `${settings.jxlBitDepth}-bit`, 8, 16, 16, ["Small", "Smooth gradients"], settings.jxlBitDepth, (v) => (settings.jxlBitDepth = v))}
        {:else}
            {@render slider("Quality", String(settings.jxlQuality), 1, 100, 95, ["Low", "Keeps grain"], settings.jxlQuality, (v) => (settings.jxlQuality = v), true)}
        {/if}
        {@render slider("Effort", String(settings.jxlEffort), 1, 9, 7, ["Fast", "Small"], settings.jxlEffort, (v) => (settings.jxlEffort = v))}
    {:else if settings.format === "jpeg"}
        {@render slider("Quality", String(settings.jpegQuality), 1, 100, 90, ["Low", "High"], settings.jpegQuality, (v) => (settings.jpegQuality = v), true)}
    {:else if settings.format === "png"}
        {@render slider("Compression", String(settings.pngCompression), 0, 9, 6, ["Fast", "Small"], settings.pngCompression, (v) => (settings.pngCompression = v))}
    {:else if settings.format === "tiff"}
        {@render pair(`${channels} 8-bit`, `${channels} 16-bit`, settings.tiffBitDepth === 8, (v) => (settings.tiffBitDepth = v ? 8 : 16))}
    {:else if settings.format === "webp"}
        {@render pair("Lossless", "Lossy", settings.webpLossless, (v) => (settings.webpLossless = v))}
        {#if settings.webpLossless}
            {@render slider("Compression", String(settings.webpCompression), 0, 9, 6, ["Fast", "Small"], settings.webpCompression, (v) => (settings.webpCompression = v))}
        {:else}
            {@render slider("Quality", String(settings.webpQuality), 1, 100, 90, ["Low", "High"], settings.webpQuality, (v) => (settings.webpQuality = v), true)}
        {/if}
    {/if}
</div>

<style>
    .setting-value {
        font-size: 12px;
        color: var(--color3);
        min-width: 20px;
        text-align: right;
        font-variant-numeric: tabular-nums;
    }

    .setting-hints {
        font-size: 10px;
        color: var(--color3);
    }

    .setting :global(.slider .left-bar),
    .setting :global(.slider .right-bar) {
        background: var(--sliderTrack);
    }
</style>
