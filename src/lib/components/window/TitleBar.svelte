<!-- lib/components/TitleBar.svelte -->
<script lang="ts">
    import { getCurrentWindow } from "@tauri-apps/api/window";
    import { onMount, onDestroy } from "svelte";
    import MenuButton from "./TitleBarMenuButton.svelte";

    import "$lib/styles/palette.css";

    const appWindow = getCurrentWindow();

    let {
        title = $bindable("Open Darkroom"),
        editable = false,
        undo = () => {},
        redo = () => {},
        open = () => {},
    }: {
        title?: string;
        editable?: boolean;
        undo?: () => void;
        redo?: () => void;
        open?: () => void;
    } = $props();

    let isMaximized = $state(false);
    let unlisten: (() => void) | undefined;

    async function checkMaximized() {
        // Small delay to let the window manager settle
        await new Promise((r) => setTimeout(r, 50));
        isMaximized = await appWindow.isMaximized();
    }

    async function handleToggleMaximize() {
        await appWindow.toggleMaximize();
        await checkMaximized();
    }

    onMount(async () => {
        isMaximized = await appWindow.isMaximized();

        unlisten = await appWindow.onResized(async () => {
            await checkMaximized();
        });
    });

    onDestroy(() => {
        unlisten?.();
    });
</script>

<div class="titlebar h-8 fixed z-999 flex items-center justify-center">
    <div data-tauri-drag-region class="drag fixed w-full h-8"></div>
    <div class="icon flex absolute aspect-square h-5.5 -z-2">
        <img src="favicon.png" alt= "Darkroom Icon" aria-label="Darkroom Icon">
    </div>
    <div class="menu-buttons fixed flex gap-2">
        <MenuButton
            label="File"
            items={[
                { label: "Open…", shortcut: "Ctrl+O", action: open},
                { separator: true },
                { label: "Export…", shortcut: "Ctrl+Shift+E" },
                { separator: true },
                { label: "Settings", shortcut: "Ctrl+," },
            ]}
        />
        <MenuButton
            label="Edit"
            items={[
                { label: "Undo", shortcut: "Ctrl+Z", action: undo},
                { label: "Redo", shortcut: "Ctrl+Shift+Z", action: redo},
                { separator: true },
                { label: "Copy Adjustments", shortcut: "Ctrl+C" },
                { label: "Paste Adjustments", shortcut: "Ctrl+V" },
                { label: "Reset Adjustments" },
            ]}
        />
        <MenuButton
            label="Help"
            items={[
                { label: "Documentation" },
                { label: "Keyboard Shortcuts", shortcut: "Ctrl+/" },
                { separator: true },
                { label: "Report a Bug…" },
                { label: "About Open Darkroom" },
            ]}
        />
    </div>
    <div class="window-controls fixed flex gap-1 items-center">
        <button
            class="inline-flex justify-center items-center h-5.5 w-8 rounded-[6px] z-2 border-0 cursor-app"
            onclick={() => appWindow.minimize()}
            title="Minimize"
        >
            <svg
                width="16"
                height="1.4"
                viewBox="0 0 12 2"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
            >
                <path
                    d="M1 2C0.447715 2 0 1.55228 0 1C0 0.447715 0.447715 0 1 0H11C11.5523 0 12 0.447715 12 1C12 1.55228 11.5523 2 11 2H1Z"
                    fill="var(--color2)"
                />
            </svg>
        </button>
        <button
            class="inline-flex justify-center items-center h-5.5 w-8 rounded-[6px] z-2 border-0 cursor-app"
            onclick={handleToggleMaximize}
            title={isMaximized ? "Restore" : "Maximize"}
        >
            {#if isMaximized}
                <!-- Restore icon: overlapping squares -->
                <svg
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    xmlns="http://www.w3.org/2000/svg"
                >
                    <path
                        d="M5.9 19.5197C5.405 19.5197 4.98125 19.3465 4.62875 19C4.27625 18.6535 4.1 18.237 4.1 17.7505V9.78923C4.1 9.30271 4.27625 8.88621 4.62875 8.53975C4.98125 8.19329 5.405 8.02005 5.9 8.02005H7.7V6.2692C7.7 5.78268 7.87625 5.36618 8.22875 5.01971C8.58125 4.67325 9.005 4.50002 9.5 4.50002L19.2 4.50001C19.695 4.50001 20.1188 4.67325 20.4713 5.01971C20.8238 5.36617 21 5.78267 21 6.26919V14.2702C21 14.7567 20.8238 15.1732 20.4713 15.5197C20.1188 15.8662 19.695 16.0394 19.2 16.0394H17.4V17.7505C17.4 18.237 17.2238 18.6535 16.8713 19C16.5188 19.3465 16.095 19.5197 15.6 19.5197L5.9 19.5197ZM5.9 17.7505L15.6 17.7505V13.7699V9.78922L5.9 9.78923V17.7505ZM17.4 14.5H19C19.2762 14.5 19.5 14.2761 19.5 14V6.59442C19.5 6.31828 19.2762 6.09442 19 6.09442L9.82523 6.09443C9.54909 6.09443 9.32523 6.31828 9.32523 6.59443V8.02005L15.6 8.02005C16.095 8.02005 16.5188 8.19328 16.8713 8.53974C17.2238 8.8862 17.4 9.3027 17.4 9.78922V14.5Z"
                        fill="var(--color2)"
                    />
                </svg>
            {:else}
                <!-- Maximize icon: single square -->
                <svg
                    width="15"
                    height="15"
                    viewBox="0 0 24 24"
                    fill="none"
                    xmlns="http://www.w3.org/2000/svg"
                >
                    <rect
                        x="4.5"
                        y="6"
                        width="15"
                        height="13"
                        rx="1.5"
                        stroke="var(--color2)"
                        stroke-width="2"
                        fill="none"
                    />
                </svg>
            {/if}
        </button>
        <button
            class="inline-flex justify-center items-center h-5.5 w-8 rounded-[6px] z-2 border-0 cursor-app"
            onclick={() => appWindow.close()}
            title="Close"
        >
            <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="none"
                xmlns="http://www.w3.org/2000/svg"
            >
                <path
                    d="M7.1 18.3C6.7134 18.6866 6.0866 18.6866 5.7 18.3C5.3134 17.9134 5.3134 17.2866 5.7 16.9L10.6 12L5.7 7.1C5.3134 6.7134 5.3134 6.0866 5.7 5.7C6.0866 5.3134 6.7134 5.3134 7.1 5.7L12 10.6L16.9 5.7C17.2866 5.3134 17.9134 5.3134 18.3 5.7C18.6866 6.0866 18.6866 6.7134 18.3 7.1L13.4 12L18.3 16.9C18.6866 17.2866 18.6866 17.9134 18.3 18.3C17.9134 18.6866 17.2866 18.6866 16.9 18.3L12 13.4L7.1 18.3Z"
                    fill="var(--color2)"
                />
            </svg>
        </button>
    </div>
    <input
        bind:value={title}
        readonly={!editable}
        class="window-title h-5.5 rounded-[6px] px-2 py-1 border-2 border-transparent"
        class:z-2={editable}
        class:z-0={!editable}
        style="pointer-events: {editable ? 'auto' : 'none'}"
    />
</div>

<style>
    .titlebar {
        top: 0;
        left: 0;
        right: 0;

        background: black;

        font-family: "Figtree", sans-serif;
        font-size: 16px;
    }

    .icon {
        left: 5px;
    }

    .titlebar > .menu-buttons {
        top: 3px;
        left: 36px;
    }

    .titlebar > .window-controls {
        top: 3px;
        right: 3px;
    }
    .window-controls button {
        background: transparent;
        color: var(--color2);
        transition:
            color 0.5s ease,
            background 0.2s cubic-bezier(0.2, 0, 0, 1),
            border-radius 0.2s cubic-bezier(0.2, 0, 0, 1);
    }
    .window-controls button svg path,
    .window-controls button svg rect {
        transition: fill 0.5s ease, stroke 0.35s ease;
    }
    .window-controls button:hover {
        background: var(--bg3);
    }

    .window-title {
        font-size: 14px;
        color: var(--color1);
        background: transparent;
        outline: none;

        text-align: center;
        pointer-events: auto;
        transition:
            color 0.5s ease,
            border-color 0.2s cubic-bezier(0.2, 0, 0, 1),
            border-radius 0.2s cubic-bezier(0.2, 0, 0, 1),
            height 0.2s cubic-bezier(0.2, 0, 0, 1),
            background 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .window-title:hover {
        background-color: var(--bg2);
    }

    .window-title:focus {
        height: 20px;
        background-color: var(--bg2);
        border-radius: 8px;
        /* or whatever color you prefer */
    }

    /* Change text selection color */
    .window-title::selection {
        background: var(--bg5); 
        color: var(--color1); 
    }

</style>
