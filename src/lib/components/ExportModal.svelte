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
    <div class="overlay">
        <div class="modal">
            <p class="label">Exporting… {Math.round(progress * 100)}%</p>
            <div class="track">
                <div class="bar" style="width: {progress * 100}%"></div>
            </div>
        </div>
    </div>
{/if}

<style>
    .overlay {
        position: fixed;
        inset: 0;
        z-index: 9999;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
    }

    .modal {
        display: flex;
        flex-direction: column;
        gap: 14px;
        padding: 28px 36px;
        background: var(--bg3);
        border-radius: 10px;
        box-shadow: 0 12px 40px rgba(0, 0, 0, 0.7);
        min-width: 260px;
    }

    .label {
        margin: 0;
        font-size: 15px;
        font-weight: 500;
        color: var(--color2);
        text-align: center;
    }

    .track {
        width: 100%;
        height: 4px;
        background: var(--bg5);
        border-radius: 2px;
        overflow: hidden;
    }

    .bar {
        height: 100%;
        background: var(--color1);
        border-radius: 2px;
        transition: width 0.15s ease-out;
    }
</style>
