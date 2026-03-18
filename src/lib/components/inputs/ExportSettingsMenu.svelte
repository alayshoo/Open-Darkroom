<script lang="ts">
    import type { ExportSettings } from "$lib/types/exportSettings";
    import SingleValSliderInt from "./values/SingleValSliderInt.svelte";

    let {
        settings = $bindable(),
    }: {
        settings: ExportSettings;
    } = $props();
</script>

<div class="menu" role="menu" aria-label="Export settings">
    <div class="format-row">
        <button
            class="pill"
            class:active={settings.format === "png"}
            onclick={() => (settings.format = "png")}
        >
            WebP
        </button>
        <button
            class="pill"
            class:active={settings.format === "png"}
            onclick={() => (settings.format = "png")}
        >
            PNG
        </button>
        <button
            class="pill"
            class:active={settings.format === "jpeg"}
            onclick={() => (settings.format = "jpeg")}
            >JPEG
        </button>
        <button
            class="pill"
            class:active={settings.format === "jpeg"}
            onclick={() => (settings.format = "jpeg")}
        >
            TIFF
        </button>
    </div>

    {#if settings.format === "png"}
        <div class="setting">
            <div class="setting-header">
                <span class="setting-label">Compression</span>
                <span class="setting-value">{settings.pngCompression}</span>
            </div>
            <SingleValSliderInt
                min={0}
                max={9}
                step={1}
                defaultValue={9}
                bind:value={settings.pngCompression} 
                ></SingleValSliderInt>
            <div class="setting-hints">
                <span>Fast</span>
                <span>Small</span>
            </div>
        </div>
    {/if}

    {#if settings.format === "jpeg"}
        <div class="setting">
            <div class="setting-header">
                <span class="setting-label">Quality</span>
                <span class="setting-value">{settings.jpegQuality}</span>
            </div>
            <SingleValSliderInt
                min={1}
                max={100}
                step={1}
                defaultValue={100}
                bind:value={settings.jpegQuality} 
                stepAnimation={false}
                springy={false}
                ></SingleValSliderInt>
            <div class="setting-hints">
                <span>Low</span>
                <span>High</span>
            </div>
        </div>
    {/if}
</div>

<style>
    .menu {
        position: absolute;
        bottom: calc(100% + 8px);
        left: 0;
        right: 0;

        display: flex;
        flex-direction: column;
        gap: 12px;

        background: var(--bg3);
        border-radius: 8px;
        padding: 12px;

        margin-right: 16px;
        margin-left: 8px;

        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);

        z-index: 100;

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

    .format-row {
        display: flex;
        flex-direction: row;
        gap: 8px;
    }

    .pill {
        flex: 1;
        padding: 5px 0;
        border: none;
        border-radius: 6px;
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
    }

    .setting {
        display: flex;
        flex-direction: column;
        gap: 6px;
    }

    .setting-header {
        display: flex;
        justify-content: space-between;
        align-items: baseline;
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
        display: flex;
        justify-content: space-between;
        font-size: 10px;
        color: var(--color3);
    }
</style>
