<!-- src/lib/components/outputs/Loupe.svelte -->
<script lang="ts">
    import type { GPUCanvasLink } from "$lib/types/gpuTypes";
    import { getGPU } from "$lib/gpu/gpuInit";
    import { linkCanvas2GPU } from "$lib/gpu/canvasConfig";
    import { loupe } from "$lib/gpu/loupe.svelte";

    const gpu = getGPU()!;
    const id = loupe.register();

    let wrapper: HTMLDivElement;
    let canvas: HTMLCanvasElement;
    let link: GPUCanvasLink | null = $state.raw(null);
    let size = $state(0);
    let onScreen = $state(false);

    $effect(() => {
        let destroyed = false;
        linkCanvas2GPU(canvas, gpu, "premultiplied").then((l) => {
            if (!destroyed) link = l;
        });
        return () => {
            destroyed = true;
        };
    });

    // Keeps the backing store at device pixels, square.
    $effect(() => {
        const ro = new ResizeObserver(([entry]) => {
            let w: number, h: number;
            if (entry.devicePixelContentBoxSize) {
                w = entry.devicePixelContentBoxSize[0].inlineSize;
                h = entry.devicePixelContentBoxSize[0].blockSize;
            } else {
                const dpr = window.devicePixelRatio || 1;
                w = Math.round(entry.contentBoxSize[0].inlineSize * dpr);
                h = Math.round(entry.contentBoxSize[0].blockSize * dpr);
            }
            const next = Math.max(0, Math.min(w, h));
            if (next !== canvas.width || next !== canvas.height) {
                canvas.width = canvas.height = next;
            }
            size = next;
        });
        try {
            ro.observe(canvas, { box: "device-pixel-content-box" });
        } catch {
            ro.observe(canvas, { box: "content-box" });
        }
        return () => ro.disconnect();
    });

    // Only rendered while some of it is on screen.
    $effect(() => {
        const io = new IntersectionObserver(([entry]) => (onScreen = entry.isIntersecting));
        io.observe(wrapper);
        return () => io.disconnect();
    });

    $effect(() => {
        loupe.show(id, link && onScreen && size > 0 ? { link, size } : null);
        return () => loupe.show(id, null);
    });

    function onKeydown(e: KeyboardEvent) {
        if (e.key !== "Enter" && e.key !== " ") return;
        e.preventDefault();
        loupe.toggle();
    }
</script>

<div
    bind:this={wrapper}
    class="loupe relative flex w-full aspect-square rounded-[6px] mb-3"
    class:picking={loupe.picking}
    data-loupe
    role="button"
    tabindex="0"
    aria-label="1:1 loupe. Press to place it on the image."
    aria-pressed={loupe.picking}
    onclick={() => loupe.toggle()}
    onkeydown={onKeydown}
>
    <canvas bind:this={canvas} class:hidden={loupe.status !== "ready"}></canvas>
    {#if loupe.status === "loading"}
        <span class="loading">Loading full resolution…</span>
    {/if}
</div>

<style>
    .loupe {
        background: black;
        overflow: hidden;
        cursor: pointer;
        outline: none;
    }

    /* Drawn over the image rather than around it, so it never moves it. */
    .loupe::after {
        content: "";
        position: absolute;
        inset: 0;
        border-radius: inherit;
        border: 2px solid transparent;
        pointer-events: none;
    }

    .loupe.picking::after,
    .loupe:focus-visible::after {
        border-color: var(--loupeAccent);
    }

    canvas {
        width: 100%;
        height: 100%;
        display: block;
    }

    canvas.hidden {
        visibility: hidden;
    }

    .loading {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 11px;
        color: var(--color3);
    }
</style>
