<script lang="ts">
    import type { ExportSettings } from "$lib/types/exportSettings";
    import SingleValSliderInt from "./values/SingleValSliderInt.svelte";

    let {
        settings = $bindable(),
        isRgb = true,
    }: {
        settings: ExportSettings;
        isRgb?: boolean;
    } = $props();

    $effect(() => {
        settings.isBw = !isRgb;
    });
</script>

<div class="menu absolute flex flex-col gap-3 rounded-[8px] p-3 mr-4 ml-2 z-100" role="menu" aria-label="Export settings">
    {#if settings.format === "png"}
        <div class="setting flex flex-col gap-1.5">
            <div class="setting-header flex justify-between items-baseline">
                <span class="setting-label">Compression</span>
                <span class="setting-value">{settings.pngCompression}</span>
            </div>
            <SingleValSliderInt
                min={0}
                max={9}
                step={1}
                defaultValue={6}
                bind:value={settings.pngCompression}
                ></SingleValSliderInt>
            <div class="setting-hints flex justify-between -mt-2">
                <span>Fast</span>
                <span>Small</span>
            </div>
        </div>
    {/if}

    {#if settings.format === "jpeg"}
        <div class="setting flex flex-col gap-1.5">
            <div class="setting-header flex justify-between items-baseline">
                <span class="setting-label">Quality</span>
                <span class="setting-value">{settings.jpegQuality}</span>
            </div>
            <SingleValSliderInt
                min={1}
                max={100}
                step={1}
                defaultValue={90}
                bind:value={settings.jpegQuality}
                stepAnimation={false}
                springy={false}
                ></SingleValSliderInt>
            <div class="setting-hints flex justify-between -mt-2">
                <span>Low</span>
                <span>High</span>
            </div>
        </div>
    {/if}

    {#if settings.format === "webp"}
        <div class="setting flex flex-col gap-1.5">
            <div class="format-row flex flex-row gap-2">
                <button
                    class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
                    class:active={settings.webpLossless}
                    onclick={() => (settings.webpLossless = true)}
                >
                    Lossless
                </button>
                <button
                    class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
                    class:active={!settings.webpLossless}
                    onclick={() => (settings.webpLossless = false)}
                >
                    Lossy
                </button>
            </div>
        </div>
        {#if settings.webpLossless}
            <div class="setting flex flex-col gap-1.5">
                <div class="setting-header flex justify-between items-baseline">
                    <span class="setting-label">Compression</span>
                    <span class="setting-value">{settings.webpCompression}</span>
                </div>
                <SingleValSliderInt
                    min={0}
                    max={9}
                    step={1}
                    defaultValue={6}
                    bind:value={settings.webpCompression}
                    ></SingleValSliderInt>
                <div class="setting-hints flex justify-between -mt-2">
                    <span>Fast</span>
                    <span>Small</span>
                </div>
            </div>
        {:else}
            <div class="setting flex flex-col gap-1.5">
                <div class="setting-header flex justify-between items-baseline">
                    <span class="setting-label">Quality</span>
                    <span class="setting-value">{settings.webpQuality}</span>
                </div>
                <SingleValSliderInt
                    min={1}
                    max={100}
                    step={1}
                    defaultValue={90}
                    bind:value={settings.webpQuality}
                    stepAnimation={false}
                    springy={false}
                    ></SingleValSliderInt>
                <div class="setting-hints flex justify-between -mt-2">
                    <span>Low</span>
                    <span>High</span>
                </div>
            </div>
        {/if}
    {/if}

    {#if settings.format === "tiff"}
        <div class="setting flex flex-col gap-1.5">
            <div class="format-row flex flex-row gap-2">
                <button
                    class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
                    class:active={settings.tiffBitDepth === 8}
                    onclick={() => (settings.tiffBitDepth = 8)}
                >
                    {isRgb ? "RGB" : "Greyscale"} 8-bit
                </button>
                <button
                    class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
                    class:active={settings.tiffBitDepth === 16}
                    onclick={() => (settings.tiffBitDepth = 16)}
                >
                    {isRgb ? "RGB" : "Greyscale"} 16-bit
                </button>
            </div>
        </div>
    {/if}

    <div class="rule h-0.25 shrink-0"></div>

    <div class="format-row flex flex-row gap-2">
        <button
            class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
            class:active={settings.format === "webp"}
            onclick={() => (settings.format = "webp")}
        >
            WebP
        </button>
        <button
            class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
            class:active={settings.format === "png"}
            onclick={() => (settings.format = "png")}
        >
            PNG
        </button>
        <button
            class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
            class:active={settings.format === "jpeg"}
            onclick={() => (settings.format = "jpeg")}
            >JPEG
        </button>
        <button
            class="pill flex-1 px-0 py-1.25 border-0 rounded-[6px]"
            class:active={settings.format === "tiff"}
            onclick={() => (settings.format = "tiff")}
        >
            TIFF
        </button>
    </div>
</div>

<style>
    .menu {
        bottom: calc(100% + 22px);
        left: 0;
        right: 0;

        background: var(--bg3);

        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);

        animation: menuFadeIn 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
    }

    @keyframes menuFadeIn {
        from {
            opacity: 0;
            transform: translateY(50px) scale(0.95);
        }
        to {
            opacity: 1;
            transform: translateY(0) scale(1);
        }
    }

    .rule {
        background: var(--bg5);
    }

    .pill {
        font-size: 13px;

        background: var(--bg4);
        color: var(--color2);

        transition:
            background 0.2s cubic-bezier(0.2, 0, 0, 1),
            color 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .pill:hover {
        background: var(--bg5);
    }

    .pill.active {
        background: var(--color1);
        color: var(--bg1);
        font-weight: 600;
    }

    .setting-label {
        font-size: 12px;
        font-weight: 500;
        color: var(--color2);
    }

    .setting-value {
        font-size: 12px;
        color: var(--color3);
        min-width: 20px;
        text-align: right;
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
