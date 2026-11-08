<!-- lib/components/export/ExportModal.svelte -->
<script lang="ts">
    import { tick } from "svelte";
    import { exportDialog, type ExportPhase } from "$lib/export/exportDialog.svelte";
    import { editor } from "$lib/editor/editor.svelte";
    import { proofing } from "$lib/gpu/proofing.svelte";
    import { OUTPUT_SPACE_IDS, OUTPUT_SPACE_LABELS } from "$lib/gpu/outputSpace";
    import { cropView } from "$lib/types/crop";
    import { metadataRows } from "$lib/types/imageMetadata";
    import {
        RESIZE_LABELS,
        RESIZE_MODES,
        exportFileName,
        targetSize,
        FORMAT_EXTENSIONS,
    } from "$lib/types/exportSettings";
    import { checkExportDestination, type DestinationCheck } from "$lib/utils/exportImage";
    import GlassButton from "$lib/components/inputs/GlassButton.svelte";
    import SettingsToggle from "$lib/components/settings/SettingsToggle.svelte";
    import ExportFormatOptions from "./ExportFormatOptions.svelte";

    const PHASE_LABELS: Record<ExportPhase, string> = {
        preparing: "Preparing",
        rendering: "Rendering",
        encoding: "Encoding",
    };

    const settings = $derived(exportDialog.settings);
    const extension = $derived(`.${FORMAT_EXTENSIONS[settings.format][0]}`);

    // The crop as it will be rendered, before any resize.
    const source = $derived.by((): [number, number] | null => {
        const image = editor.imagePayload;
        if (!image) return null;
        const view = cropView(editor.edit.crop, image.fullWidth, image.fullHeight);
        return [view.width, view.height];
    });
    const output = $derived(source ? targetSize(settings, ...source) : null);
    const resized = $derived(!!source && !!output && (output[0] !== source[0] || output[1] !== source[1]));

    const shotRows = $derived(
        editor.metadata
            ? metadataRows(editor.metadata, 0, 0).filter((r) => r.label !== "File" && r.label !== "Dimensions")
            : [],
    );

    // What the backend says about the folder and name, after the frontend's own check.
    let check = $state<DestinationCheck | { error: string } | null>(null);
    $effect(() => {
        if (!exportDialog.open) return;
        const { folder, fileName } = exportDialog;
        const format = settings.format;
        const local = exportFileName(fileName, format);
        if ("error" in local) {
            check = { error: local.error };
            return;
        }
        if (!folder) {
            check = { error: "Choose a folder to export to" };
            return;
        }
        // Until the backend answers for these, nothing can be exported over them.
        check = null;
        let stale = false;
        const timer = setTimeout(() => {
            checkExportDestination({ folder, fileName }, format)
                .then((result) => !stale && (check = result))
                .catch((e) => !stale && (check = { error: String(e) }));
        }, 120);
        return () => {
            stale = true;
            clearTimeout(timer);
        };
    });

    const problem = $derived(check && "error" in check ? check.error : null);
    const replaces = $derived(!!check && "exists" in check && check.exists);
    const canExport = $derived(!!check && !problem && !exportDialog.running && !!editor.imagePayload);
    const finalName = $derived.by(() => {
        const name = exportFileName(exportDialog.fileName, settings.format);
        return "name" in name ? name.name : null;
    });

    let dialogEl: HTMLElement | undefined = $state();
    $effect(() => {
        if (exportDialog.open) tick().then(() => dialogEl?.focus());
    });

    const usesPercent = $derived(settings.resize === "percent");

    function setResizeValue(raw: string, commit: boolean) {
        const max = usesPercent ? 100 : 65535;
        const value = Math.round(Number(raw));
        const valid = raw.trim() !== "" && Number.isFinite(value) && value >= 1;
        if (!valid && !commit) return;
        const clamped = valid ? Math.min(value, max) : usesPercent ? 50 : 2048;
        if (usesPercent) settings.resizePercent = clamped;
        else settings.resizePixels = clamped;
    }

    function fieldKeydown(e: KeyboardEvent) {
        if (e.key !== "Enter") return;
        e.preventDefault();
        (e.target as HTMLInputElement).blur();
        if (canExport) exportDialog.run();
    }

    function folderParts(path: string): [string, string] {
        const at = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
        return at < 0 ? ["", path] : [path.slice(0, at + 1), path.slice(at + 1) || path];
    }
    const [folderParent, folderName] = $derived(folderParts(exportDialog.folder));
</script>

{#if exportDialog.open}
    <div class="overlay fixed inset-0 z-1000 flex items-center justify-center" role="presentation">
        <div class="scrim absolute inset-0" role="presentation" onpointerdown={exportDialog.close}></div>
        <div
            bind:this={dialogEl}
            class="dialog relative flex flex-col rounded-[14px]"
            role="dialog"
            aria-modal="true"
            aria-labelledby="export-title"
            tabindex="-1"
        >
            <header class="header flex shrink-0 items-center justify-between pl-6 pr-2.5">
                <h2 id="export-title" class="title m-0">Export</h2>
                <div class="self-start mt-2.5">
                    <GlassButton label="Close export" disabled={exportDialog.running} onclick={exportDialog.close}>
                        <svg width="28" height="28" viewBox="0 0 24 24">
                            <path
                                d="M8.6 8.6l6.8 6.8M15.4 8.6l-6.8 6.8"
                                fill="none"
                                stroke="currentColor"
                                stroke-width="1.4"
                                stroke-linecap="round"
                            />
                        </svg>
                    </GlassButton>
                </div>
            </header>

            <div class="body grid gap-4 px-6 pb-5" inert={exportDialog.running}>
                <div class="column flex flex-col gap-4">
                    <section class="card flex flex-col gap-3" aria-labelledby="export-destination">
                        <h3 id="export-destination" class="card-title m-0">Destination</h3>
                        <div class="field-group flex flex-col gap-1.5">
                            <span class="setting-label">Folder</span>
                            <div class="flex gap-2 items-center">
                                <div class="folder field flex-1 min-w-0 flex items-center" title={exportDialog.folder}>
                                    {#if exportDialog.folder}
                                        <span class="folder-parent min-w-0">{folderParent}</span>
                                        <span class="folder-name shrink-0">{folderName}</span>
                                    {:else}
                                        <span class="placeholder">No folder chosen</span>
                                    {/if}
                                </div>
                                <button class="pill shrink-0 px-3" onclick={exportDialog.chooseFolder}>Choose…</button>
                            </div>
                        </div>
                        <label class="field-group flex flex-col gap-1.5">
                            <span class="setting-label">File name</span>
                            <div class="field name-field flex items-center">
                                <input
                                    class="name-input flex-1 min-w-0"
                                    type="text"
                                    spellcheck="false"
                                    autocomplete="off"
                                    bind:value={exportDialog.fileName}
                                    onkeydown={fieldKeydown}
                                />
                                <span class="extension shrink-0">{extension}</span>
                            </div>
                        </label>
                        <p class="hint m-0" class:warn={replaces || problem} aria-live="polite">
                            {#if problem}
                                {problem}.
                            {:else if replaces}
                                {finalName} is already in this folder and will be replaced.
                            {:else}
                                Saved as {finalName ?? "…"}
                            {/if}
                        </p>
                    </section>

                    <section class="card flex flex-col gap-3" aria-labelledby="export-size">
                        <h3 id="export-size" class="card-title m-0">Size</h3>
                        <div class="grid grid-cols-3 gap-2" role="radiogroup" aria-label="Resize by">
                            {#each RESIZE_MODES as mode (mode)}
                                <button
                                    class="pill"
                                    class:active={settings.resize === mode}
                                    role="radio"
                                    aria-checked={settings.resize === mode}
                                    onclick={() => (settings.resize = mode)}
                                >
                                    {RESIZE_LABELS[mode]}
                                </button>
                            {/each}
                        </div>
                        {#if settings.resize !== "none"}
                            <label class="flex items-center justify-between gap-3">
                                <span class="setting-label">{RESIZE_LABELS[settings.resize]}</span>
                                <span class="field number-field flex items-center">
                                    <input
                                        class="number-input"
                                        type="number"
                                        min="1"
                                        max={usesPercent ? 100 : 65535}
                                        step="1"
                                        value={usesPercent ? settings.resizePercent : settings.resizePixels}
                                        oninput={(e) => setResizeValue(e.currentTarget.value, false)}
                                        onchange={(e) => {
                                            setResizeValue(e.currentTarget.value, true);
                                            e.currentTarget.value = String(usesPercent ? settings.resizePercent : settings.resizePixels);
                                        }}
                                        onkeydown={fieldKeydown}
                                    />
                                    <span class="unit">{usesPercent ? "%" : "px"}</span>
                                </span>
                            </label>
                        {/if}
                        {#if source && output}
                            <p class="hint m-0">
                                {#if resized}
                                    {source[0]} × {source[1]} → <strong>{output[0]} × {output[1]}</strong> px
                                {:else}
                                    <strong>{source[0]} × {source[1]}</strong> px{settings.resize !== "none" ? ", never enlarged" : ""}
                                {/if}
                            </p>
                        {/if}
                    </section>

                    <section class="card flex flex-col gap-3" aria-labelledby="export-metadata">
                        <div class="flex items-center justify-between gap-3">
                            <div class="flex flex-col">
                                <h3 id="export-metadata" class="card-title m-0">Shooting info</h3>
                                <span class="description">Camera, lens, exposure and date taken</span>
                            </div>
                            <SettingsToggle
                                label="Write shooting info into the file"
                                checked={settings.metadata}
                                onclick={() => (settings.metadata = !settings.metadata)}
                            />
                        </div>
                        {#if settings.metadata}
                            <p class="hint m-0">
                                {#if shotRows.length}
                                    {shotRows.map((r) => (r.label === "ISO" ? `ISO ${r.value}` : r.value)).join(" · ")}
                                {:else}
                                    The original says nothing about how it was shot.
                                {/if}
                            </p>
                        {/if}
                    </section>
                </div>

                <div class="column flex flex-col gap-4">
                    <section class="card flex flex-col gap-3" aria-labelledby="export-format">
                        <h3 id="export-format" class="card-title m-0">Format</h3>
                        <ExportFormatOptions bind:settings={exportDialog.settings} isRgb={editor.edit.isRgb} />
                    </section>

                    <section class="card flex flex-col gap-3" aria-labelledby="export-colour">
                        <h3 id="export-colour" class="card-title m-0">Colour space</h3>
                        <div class="grid grid-cols-2 gap-2" role="radiogroup" aria-label="Colour space">
                            {#each OUTPUT_SPACE_IDS as id (id)}
                                <button
                                    class="pill"
                                    class:active={settings.outputSpace === id}
                                    role="radio"
                                    aria-checked={settings.outputSpace === id}
                                    onclick={() => (settings.outputSpace = id)}
                                >
                                    {OUTPUT_SPACE_LABELS[id]}
                                </button>
                            {/each}
                        </div>
                        <div class="toggle-row flex items-center justify-between gap-3">
                            <div class="flex flex-col">
                                <span class="setting-label">Soft proof</span>
                                <span class="description">Show the preview through this space's gamut</span>
                            </div>
                            <SettingsToggle label="Soft proof" checked={proofing.enabled} onclick={() => proofing.toggle()} />
                        </div>
                        <div class="toggle-row flex items-center justify-between gap-3">
                            <div class="flex flex-col">
                                <span class="setting-label">Out of gamut warning</span>
                                <span class="description">Mark the pixels this space cannot hold</span>
                            </div>
                            <SettingsToggle
                                label="Out of gamut warning"
                                checked={proofing.warning}
                                onclick={() => proofing.toggleWarning()}
                            />
                        </div>
                    </section>
                </div>
            </div>

            <footer class="footer flex shrink-0 items-center gap-4 px-6">
                {#if exportDialog.running && exportDialog.progress}
                    {@const p = exportDialog.progress}
                    <div class="progress flex flex-col flex-1 gap-2" role="status">
                        <span class="progress-label">
                            {PHASE_LABELS[p.phase]} {finalName}…{p.indeterminate ? "" : ` ${Math.round(p.progress * 100)}%`}
                        </span>
                        <div class="track relative w-full h-1 rounded-[2px]">
                            <div class="bar h-full rounded-[2px]" style="width: {p.progress * 100}%"></div>
                            {#if p.indeterminate}
                                <div class="pending absolute top-0 bottom-0 right-0" style="left: {p.progress * 100}%"></div>
                            {/if}
                        </div>
                    </div>
                {:else}
                    <p class="summary flex-1 min-w-0 m-0" class:error={!!exportDialog.error} role={exportDialog.error ? "alert" : undefined}>
                        {#if exportDialog.error}
                            Export failed: {exportDialog.error}
                        {:else if finalName && output}
                            {finalName} · {output[0]} × {output[1]} · {OUTPUT_SPACE_LABELS[settings.outputSpace]}
                        {/if}
                    </p>
                    <button class="pill action px-4" onclick={exportDialog.close}>Cancel</button>
                    <button class="pill action primary px-4" disabled={!canExport} onclick={exportDialog.run}>
                        {replaces ? "Replace" : "Export"}
                    </button>
                {/if}
            </footer>
        </div>
    </div>
{/if}

<style>
    .overlay {
        padding: 44px 12px 12px;
    }

    .scrim {
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
        animation: scrimIn 0.15s ease forwards;
    }

    .dialog {
        width: min(860px, 100%);
        max-height: 100%;
        background: var(--bg3);
        box-shadow: 0 20px 60px rgba(0, 0, 0, 0.65);
        font-family: var(--font-ui);
        outline: none;
        animation: dialogIn 0.18s cubic-bezier(0.2, 0, 0, 1) forwards;
    }

    @keyframes scrimIn {
        from {
            opacity: 0;
        }
        to {
            opacity: 1;
        }
    }

    @keyframes dialogIn {
        from {
            transform: translateY(-6px) scale(0.985);
        }
        to {
            transform: translateY(0) scale(1);
        }
    }

    .header {
        height: 56px;
    }

    .header :global(svg) {
        color: var(--color2);
    }

    .title {
        font-family: "Outfit Variable", var(--font-ui);
        font-size: 25px;
        font-weight: 700;
        color: var(--color1);
    }

    .body {
        grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
        align-items: start;
        overflow-y: auto;
        min-height: 0;
        scrollbar-width: thin;
        scrollbar-color: var(--sliderTrack) transparent;
    }

    .card {
        background: var(--bg4);
        border-radius: 10px;
        padding: 12px 14px 14px;
    }

    .card-title {
        font-size: 15px;
        font-weight: 600;
        color: var(--color1);
    }

    .description {
        font-size: 11.5px;
        line-height: 1.35;
        color: var(--color3);
    }

    .dialog :global(.setting-label) {
        font-size: 12px;
        font-weight: 500;
        color: var(--color2);
    }

    .hint {
        font-size: 12px;
        line-height: 1.4;
        color: var(--color3);
        overflow-wrap: anywhere;
    }

    .hint strong {
        color: var(--color1s);
        font-weight: 500;
    }

    .hint.warn {
        color: var(--loupeAccent);
    }

    .field {
        height: 30px;
        padding: 0 10px;
        border-radius: 6px;
        background: var(--bg2);
        font-size: 13px;
        color: var(--color1s);
    }

    .field:focus-within {
        box-shadow: 0 0 0 1.5px var(--color3);
    }

    .folder {
        white-space: nowrap;
        overflow: hidden;
    }

    .folder-parent {
        overflow: hidden;
        text-overflow: ellipsis;
        color: var(--color3);
    }

    .folder-name {
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .placeholder {
        color: var(--color3);
    }

    .name-input,
    .number-input {
        background: transparent;
        border: 0;
        outline: none;
        padding: 0;
        font: inherit;
        color: inherit;
    }

    .name-input::selection,
    .number-input::selection {
        background: var(--bg5);
        color: var(--color1);
    }

    .extension,
    .unit {
        color: var(--color3);
        padding-left: 2px;
    }

    .number-field {
        width: 110px;
    }

    .number-input {
        width: 100%;
        text-align: right;
        font-variant-numeric: tabular-nums;
        -moz-appearance: textfield;
        appearance: textfield;
    }

    .number-input::-webkit-inner-spin-button,
    .number-input::-webkit-outer-spin-button {
        -webkit-appearance: none;
        margin: 0;
    }

    .dialog :global(.pill) {
        height: 28px;
        font-family: inherit;
        font-size: 13px;
        border: 0;
        border-radius: 6px;
        background: var(--bg5);
        color: var(--color2);
        transition:
            background 0.2s cubic-bezier(0.2, 0, 0, 1),
            color 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .dialog :global(.pill:hover:not(:disabled)) {
        background: color-mix(in srgb, var(--bg5) 70%, var(--color3));
    }

    .dialog :global(.pill.active) {
        background: var(--color1);
        color: var(--bg1);
        font-weight: 600;
    }

    .dialog :global(.pill:disabled) {
        opacity: 0.45;
    }

    .footer {
        height: 64px;
        border-top: 1px solid var(--bg5);
    }

    .summary {
        font-size: 12px;
        color: var(--color3);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .summary.error {
        color: var(--loupeAccent);
        white-space: normal;
        line-height: 1.35;
    }

    .action {
        height: 32px;
        font-size: 14px;
        font-weight: 500;
    }

    .pill.primary {
        background: var(--color1);
        color: var(--bg1);
        font-weight: 600;
    }

    .pill.primary:hover:not(:disabled) {
        background: var(--color1s);
    }

    .progress-label {
        font-size: 13px;
        font-weight: 500;
        color: var(--color2);
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .track {
        background: var(--bg5);
        overflow: hidden;
    }

    .bar {
        background: var(--color1);
        transition: width 0.15s ease-out;
    }

    .pending {
        overflow: hidden;
    }

    .pending::after {
        content: "";
        position: absolute;
        top: 0;
        bottom: 0;
        width: 40%;
        background: linear-gradient(90deg, transparent, var(--color3), transparent);
        animation: sweep 1.4s ease-in-out infinite;
    }

    @keyframes sweep {
        from {
            left: -40%;
        }
        to {
            left: 100%;
        }
    }

    @media (prefers-reduced-motion: reduce) {
        .pending::after {
            animation: none;
            left: 0;
            width: 100%;
            opacity: 0.4;
        }
    }
</style>
