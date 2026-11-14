<!-- lib/components/library/Library.svelte -->
<script lang="ts">
    import { library } from "$lib/library/library.svelte";
    import { settings } from "$lib/settings/settings.svelte";
    import { createKeydownHandler, isTextField } from "$lib/config/keyboardShortcuts";
    import { rollSummary } from "$lib/types/rollInfo";
    import type { ImageRecord } from "$lib/library/catalogApi";
    import LibraryNav from "./LibraryNav.svelte";
    import ImageCell from "./ImageCell.svelte";
    import RollCard from "./RollCard.svelte";
    import DetailsPanel from "./DetailsPanel.svelte";
    import EditableName from "./EditableName.svelte";
    import ContextMenu, { type MenuItem } from "./ContextMenu.svelte";

    let { active = true }: { active?: boolean } = $props();

    let menu = $state<{ x: number; y: number; items: MenuItem[] } | null>(null);

    let title = $derived(library.currentRoll?.name ?? library.currentLibrary?.name ?? "");
    let subtitle = $derived.by(() => {
        if (library.currentRoll) {
            const summary = rollSummary(library.currentRoll.info, library.stockNameOf(library.currentRoll));
            const parts = [`In ${library.currentLibrary?.name ?? ""}`, summary].filter(Boolean);
            return parts.join(" · ");
        }
        if (!settings.groupRolls) {
            const count = library.shownImages.length;
            return `${count} image${count === 1 ? "" : "s"}`;
        }
        const rolls = library.shownRolls.length;
        const loose = library.shownImages.length;
        return [
            rolls ? `${rolls} roll${rolls === 1 ? "" : "s"}` : null,
            `${loose} loose image${loose === 1 ? "" : "s"}`,
        ]
            .filter(Boolean)
            .join(" · ");
    });

    function rename(name: string) {
        if (library.currentRoll) library.updateRoll(library.currentRoll.id, { name });
        else if (library.currentLibrary) void library.renameLibrary(library.currentLibrary.id, name);
    }

    function select(image: ImageRecord, e: MouseEvent) {
        library.select(image.id, { toggle: e.ctrlKey || e.metaKey, range: e.shiftKey });
    }

    function imageMenu(image: ImageRecord, e: MouseEvent) {
        e.preventDefault();
        if (!library.selection.includes(image.id)) library.select(image.id);
        const ids = library.selection;
        const many = ids.length > 1;
        menu = {
            x: e.clientX,
            y: e.clientY,
            items: [
                ...(many ? [] : [{ label: "Open", action: () => void library.openImage(image.id) }]),
                ...(image.missing ? [{ label: "Locate file…", action: () => void library.locate(image.id) }] : []),
                { separator: true },
                { heading: many ? `Move ${ids.length} images to` : "Move to" },
                ...library.moveTargets.map((t) => ({
                    label: t.label,
                    indent: t.depth,
                    disabled:
                        t.place.rollId === null
                            ? ids.every((id) => {
                                  const i = library.images.find((x) => x.id === id);
                                  return i?.rollId === null && i.libraryId === t.place.libraryId;
                              })
                            : ids.every((id) => library.images.find((x) => x.id === id)?.rollId === t.place.rollId),
                    action: () => void library.moveImages([...ids], t.place),
                })),
                { separator: true },
                {
                    label: many ? `Remove ${ids.length} from catalog…` : "Remove from catalog…",
                    danger: true,
                    action: () => library.removeImages([...ids]),
                },
            ],
        };
    }

    function rollMenu(id: number, e: MouseEvent) {
        e.preventDefault();
        menu = {
            x: e.clientX,
            y: e.clientY,
            items: [
                { label: "Open roll", action: () => library.show({ kind: "roll", id }) },
                { separator: true },
                { label: "Delete roll…", danger: true, action: () => library.deleteRoll(id) },
            ],
        };
    }

    const handleShortcut = createKeydownHandler({
        handlers: {
            openImage: library.openFile,
            showLibrary: library.toggleView,
            toggleSettings: () => settings.toggle(),
        },
        closeSettings: () => settings.close(),
        isSettingsOpen: () => settings.open,
    });

    function onKeydown(e: KeyboardEvent) {
        if (!active || menu || e.defaultPrevented) return;
        if (library.notice && !settings.open) {
            if (e.key === "Escape") library.notice = null;
            else if (e.key === "Enter" && library.notice.confirm) confirmNotice();
            else return;
            e.preventDefault();
            return;
        }
        if (!settings.open && !isTextField(e.target)) {
            const selection = library.selection;
            if (e.key === "Enter" && selection.length === 1) {
                void library.openImage(selection[0]);
                e.preventDefault();
                return;
            }
            if ((e.key === "Delete" || e.key === "Backspace") && selection.length > 0) {
                library.removeImages([...selection]);
                e.preventDefault();
                return;
            }
            if (e.key === "a" && (e.ctrlKey || e.metaKey)) {
                library.selectAll();
                e.preventDefault();
                return;
            }
            if (e.key === "Escape" && selection.length > 0) {
                library.selection = [];
                e.preventDefault();
                return;
            }
        }
        handleShortcut(e);
    }

    function confirmNotice() {
        const confirm = library.notice?.confirm;
        library.notice = null;
        confirm?.run();
    }
</script>

<svelte:window onkeydown={onKeydown} onfocus={library.findMovedOnFocus} />

<div class="library-layer" class:active inert={!active}>
    <div class="backdrop"></div>
    <div class="shell flex flex-row">
        <div class="nav-column shrink-0">
            <LibraryNav />
        </div>

        <main class="main flex flex-col flex-1 min-w-0">
            {#if library.view}
                <header class="head flex items-end">
                    <div class="flex flex-col flex-1 min-w-0">
                        <h1><EditableName value={title} label={library.currentRoll ? "Roll name" : "Library name"} onrename={rename} /></h1>
                        <p class="subtitle">{subtitle}</p>
                    </div>
                    <div class="actions flex">
                        {#if library.view.kind === "library"}
                            <button class="action" onclick={() => library.createRoll(library.view!.id)}>New roll</button>
                            <button class="action" onclick={library.importFolder}>Import folder as roll</button>
                        {/if}
                        <button class="action primary" onclick={library.importFiles}>
                            {library.view.kind === "roll" ? "Add frames" : "Import images"}
                        </button>
                        <button
                            class="panel-toggle"
                            class:on={settings.libraryDetails}
                            aria-label={settings.libraryDetails ? "Hide details" : "Show details"}
                            aria-pressed={settings.libraryDetails}
                            title={settings.libraryDetails ? "Hide details" : "Show details"}
                            onclick={() => (settings.libraryDetails = !settings.libraryDetails)}
                        >
                            <svg width="20" height="20" viewBox="0 0 24 24" aria-hidden="true">
                                <g fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round">
                                    <rect x="3.5" y="5" width="17" height="14" rx="2.5" />
                                    <path d="M14.5 5v14" />
                                </g>
                                <path class="fill" d="M14.5 5h3.5a2.5 2.5 0 0 1 2.5 2.5v9a2.5 2.5 0 0 1-2.5 2.5h-3.5z" fill="currentColor" />
                            </svg>
                        </button>
                    </div>
                </header>

                <div
                    class="scroll flex flex-col flex-1 min-h-0"
                    role="presentation"
                    onclick={(e) => {
                        if (e.target === e.currentTarget) library.selection = [];
                    }}
                >
                    {#if library.shownRolls.length > 0}
                        <h2>Rolls</h2>
                        <div class="rolls">
                            {#each library.shownRolls as roll (roll.id)}
                                <RollCard {roll} onmenu={(e) => rollMenu(roll.id, e)} />
                            {/each}
                        </div>
                    {/if}

                    {#if library.view.kind === "library" && library.shownRolls.length > 0 && library.shownImages.length > 0}
                        <h2>Loose images</h2>
                    {/if}
                    {#if library.shownImages.length > 0}
                        <div class="grid" role="listbox" aria-multiselectable="true" aria-label="Images">
                            {#each library.shownImages as image (image.id)}
                                <ImageCell
                                    {image}
                                    selected={library.selection.includes(image.id)}
                                    onselect={(e) => select(image, e)}
                                    onopen={() => library.openImage(image.id)}
                                    onmenu={(e) => imageMenu(image, e)}
                                />
                            {/each}
                        </div>
                    {:else if library.shownRolls.length === 0}
                        <div class="empty flex flex-col items-center justify-center">
                            {#if library.view.kind === "roll"}
                                <p>This roll has no frames yet.</p>
                                <p class="hint">Add scans with “Add frames”, or move images here from the library with a right click.</p>
                            {:else}
                                <p>Nothing in this library yet.</p>
                                <p class="hint">Import images, or import a folder of scans as a roll.</p>
                            {/if}
                        </div>
                    {/if}
                </div>
            {:else if library.loaded}
                <div class="empty flex flex-col items-center justify-center flex-1">
                    <p>Welcome to Open Darkroom.</p>
                    <p class="hint">A library holds your images, loose or on rolls of film.</p>
                    <div class="actions flex mt-4">
                        <button class="action" onclick={() => library.createLibrary()}>New library</button>
                        <button class="action primary" onclick={library.openFile}>Open an image</button>
                    </div>
                </div>
            {/if}
        </main>

        <div class="details-column shrink-0" class:closed={!settings.libraryDetails} inert={!settings.libraryDetails}>
            <div class="details-slide h-full">
                <DetailsPanel />
            </div>
        </div>
    </div>

    {#if library.notice}
        <div class="notice glass flex items-center" role="alertdialog" aria-label={library.notice.text}>
            <span class="flex-1">{library.notice.text}</span>
            {#if library.notice.confirm}
                <button class="action" onclick={() => (library.notice = null)}>Cancel</button>
                <button class="action primary" onclick={confirmNotice}>{library.notice.confirm.label}</button>
            {:else}
                <button class="action" onclick={() => (library.notice = null)}>OK</button>
            {/if}
        </div>
    {/if}
</div>

{#if menu}
    <ContextMenu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
    .library-layer {
        position: fixed;
        inset: 0;
        z-index: 0;
        visibility: hidden;
        transition: visibility 0s linear 0.4s;
    }

    .library-layer.active {
        visibility: visible;
        transition: none;
    }

    .backdrop {
        position: absolute;
        inset: 0;
        background: #141414;
    }

    .shell {
        position: absolute;
        top: 44px;
        bottom: 12px;
        left: 12px;
        right: 12px;
        gap: 12px;
    }

    .nav-column {
        width: 240px;
    }

    .details-column {
        width: 300px;
        transition:
            width 0.35s cubic-bezier(0.2, 0, 0, 1),
            margin-left 0.35s cubic-bezier(0.2, 0, 0, 1);
    }

    /* Closed, the grid takes its room and the panel slides off the window
       edge at its own width, so its fields never reflow on the way. */
    .details-column.closed {
        width: 0;
        margin-left: -12px;
    }

    .details-slide {
        width: 300px;
        transition: transform 0.35s cubic-bezier(0.2, 0, 0, 1);
    }

    .details-column.closed .details-slide {
        transform: translateX(calc(100% + 24px));
    }

    .panel-toggle {
        display: flex;
        align-items: center;
        justify-content: center;
        width: 30px;
        height: 30px;
        margin-left: 4px;
        border: 0;
        border-radius: 8px;
        background: transparent;
        color: var(--color2);
        cursor: default;
        transition: background 0.12s ease, color 0.12s ease;
    }

    .panel-toggle:hover {
        background: var(--bg4);
        color: var(--color1);
    }

    .panel-toggle .fill {
        opacity: 0;
        transition: opacity 0.15s ease;
    }

    .panel-toggle.on .fill {
        opacity: 0.35;
    }

    .head {
        gap: 16px;
        padding: 8px 8px 14px;
    }

    h1 {
        margin: 0;
        line-height: 1.3;
        font-family: var(--font-ui);
        font-size: 26px;
        font-weight: 600;
        color: var(--color1);
    }

    .subtitle {
        margin: 2px 0 0;
        line-height: 1.4;
        font-size: 13px;
        color: var(--color3);
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
    }

    .actions {
        gap: 8px;
        flex-shrink: 0;
    }

    .action {
        height: 30px;
        padding: 0 12px;
        border: 0;
        border-radius: 8px;
        background: var(--bg4);
        box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.06);
        color: var(--color1);
        font-family: var(--font-ui);
        font-size: 13px;
        white-space: nowrap;
        cursor: default;
        transition: background 0.12s ease;
    }

    .action:hover {
        background: var(--bg5);
    }

    .action.primary {
        background: var(--color1);
        color: #111;
        box-shadow: none;
        font-weight: 600;
    }

    .action.primary:hover {
        background: #fff;
    }

    .scroll {
        overflow-y: auto;
        padding: 0 4px 24px;
        scrollbar-width: thin;
        scrollbar-color: var(--color5) transparent;
    }

    h2 {
        margin: 6px 6px 10px;
        font-family: var(--font-ui);
        font-size: 15px;
        font-weight: 600;
        color: var(--color2);
    }

    .rolls {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
        gap: 12px;
        margin-bottom: 22px;
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
        gap: 4px;
        align-content: start;
    }

    .empty {
        flex: 1;
        gap: 4px;
        min-height: 240px;
        text-align: center;
        color: var(--color2);
        font-size: 15px;
    }

    .empty p {
        margin: 0;
    }

    .empty .hint {
        font-size: 13px;
        color: var(--color3);
        max-width: 360px;
    }

    .notice {
        position: fixed;
        left: 50%;
        bottom: 24px;
        transform: translateX(-50%);
        z-index: 50;
        gap: 10px;
        width: min(560px, calc(100vw - 48px));
        padding: 10px 10px 10px 16px;
        border-radius: 12px;
        box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
        font-size: 13px;
        color: var(--color1);
    }
</style>
