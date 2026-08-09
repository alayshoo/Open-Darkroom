<!-- ExportModal.svelte -->
<script lang="ts">
    import { listen, type UnlistenFn } from "@tauri-apps/api/event";
    import { onDestroy } from "svelte";

    let { visible }: { visible: boolean } = $props();

    let progress = $state(0);
    let unlisten: UnlistenFn | null = null;

    $effect(() => {
        if (visible) {
            progress = 0;
            listen<number>("export:progress", (e) => {
                progress = e.payload;
            }).then((fn) => {
                unlisten = fn;
            });
        } else {
            unlisten?.();
            unlisten = null;
        }
    });

    onDestroy(() => {
        unlisten?.();
    });
</script>

{#if visible}
    <div class="overlay fixed z-9999 flex items-center justify-center">
        <div class="modal flex flex-col gap-3.5 px-9 py-7 rounded-[10px]">
            <p class="label m-0">Exporting… {Math.round(progress * 100)}%</p>
            <div class="track w-full h-1 rounded-[2px]">
                <div class="bar h-full rounded-[2px]" style="width: {progress * 100}%"></div>
            </div>
        </div>
    </div>
{/if}

<style>
    .overlay {
        inset: 0;
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
    }

    .modal {
        background: var(--bg3);
        box-shadow: 0 12px 40px rgba(0, 0, 0, 0.7);
        min-width: 260px;
    }

    .label {
        font-size: 15px;
        font-weight: 500;
        color: var(--color2);
        text-align: center;
    }

    .track {
        background: var(--bg5);
        overflow: hidden;
    }

    .bar {
        background: var(--color1);
        transition: width 0.15s ease-out;
    }
</style>
