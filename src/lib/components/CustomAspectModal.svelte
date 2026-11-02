<!-- src/lib/components/CustomAspectModal.svelte -->
<script lang="ts">
    import { MAX_CUSTOM_RATIO, validCustom } from "$lib/types/cropAspect";

    let {
        open = $bindable(false),
        initial = [1, 1],
        onapply,
    }: {
        open?: boolean;
        // The sides the fields start from.
        initial?: [number, number];
        onapply?: (w: number, h: number) => void;
    } = $props();

    let width = $state("");
    let height = $state("");
    let widthField: HTMLInputElement | undefined = $state();

    $effect(() => {
        if (!open) return;
        width = String(initial[0]);
        height = String(initial[1]);
        queueMicrotask(() => widthField?.select());
    });

    let w = $derived(Number(width));
    let h = $derived(Number(height));
    let valid = $derived(width.trim() !== "" && height.trim() !== "" && validCustom(w, h));

    function apply() {
        if (!valid) return;
        open = false;
        onapply?.(w, h);
    }

    // The crop tool also answers Enter and Escape, so they stop here.
    function keydown(e: KeyboardEvent) {
        if (e.key === "Enter") apply();
        else if (e.key === "Escape") open = false;
        else return;
        e.preventDefault();
        e.stopPropagation();
    }
</script>

{#if open}
    <div class="overlay fixed z-1000 flex items-center justify-center">
        <div class="scrim absolute inset-0" role="presentation" onpointerdown={() => (open = false)}></div>
        <div
            class="modal relative flex flex-col gap-4 px-6 py-5 rounded-[10px]"
            role="dialog"
            aria-modal="true"
            aria-label="Custom aspect ratio"
            tabindex="-1"
            onkeydown={keydown}
        >
            <h2 class="title m-0">Custom aspect ratio</h2>
            <div class="fields flex items-center gap-2">
                <input
                    bind:this={widthField}
                    bind:value={width}
                    class="field rounded-[6px] border-0"
                    type="text"
                    inputmode="decimal"
                    aria-label="Width"
                />
                <span class="colon">:</span>
                <input
                    bind:value={height}
                    class="field rounded-[6px] border-0"
                    type="text"
                    inputmode="decimal"
                    aria-label="Height"
                />
            </div>
            <p class="hint m-0" class:shown={!valid}>
                Two positive numbers, at most {MAX_CUSTOM_RATIO}:1 either way.
            </p>
            <div class="actions flex justify-end gap-2">
                <button class="pill rounded-[6px] border-0" onclick={() => (open = false)}>Cancel</button>
                <button class="pill primary rounded-[6px] border-0" disabled={!valid} onclick={apply}>Apply</button>
            </div>
        </div>
    </div>
{/if}

<style>
    .overlay {
        inset: 0;
    }

    .scrim {
        background: rgba(0, 0, 0, 0.6);
        backdrop-filter: blur(4px);
    }

    .modal {
        background: var(--bg3);
        box-shadow: 0 12px 40px rgba(0, 0, 0, 0.7);
        min-width: 260px;
        outline: none;
        animation: modalIn 0.15s cubic-bezier(0.2, 0, 0, 1) forwards;
    }

    @keyframes modalIn {
        from {
            opacity: 0;
            transform: scale(0.96);
        }
        to {
            opacity: 1;
            transform: scale(1);
        }
    }

    .title {
        font-size: 15px;
        font-weight: 500;
        color: var(--color1);
    }

    .field {
        width: 90px;
        padding: 6px 10px;
        font: 14px var(--font-ui);
        font-variant-numeric: tabular-nums;
        text-align: center;
        background: var(--bg4);
        color: var(--color1);
        outline: 1px solid transparent;
        transition: outline-color 0.15s ease;
    }

    .field:focus {
        outline-color: color-mix(in srgb, var(--color1) 50%, transparent);
    }

    .colon {
        font-size: 16px;
        color: var(--color2);
    }

    .hint {
        font-size: 11px;
        color: var(--color3);
        visibility: hidden;
    }

    .hint.shown {
        visibility: visible;
    }

    .pill {
        padding: 5px 14px;
        font: 13px var(--font-ui);
        background: var(--bg4);
        color: var(--color2);
        cursor: pointer;
        transition: background 0.2s cubic-bezier(0.2, 0, 0, 1);
    }

    .pill:hover {
        background: var(--bg5);
    }

    .pill.primary {
        background: var(--color1);
        color: var(--bg1);
        font-weight: 600;
    }

    .pill:disabled {
        opacity: 0.4;
        cursor: default;
    }
</style>
