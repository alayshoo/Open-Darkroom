<!-- lib/components/export/ExportProgress.svelte -->
<!-- The progress of an export started from the save dialog rather than the modal, and its failure. -->
<script lang="ts">
    import { exportDialog, type ExportPhase } from "$lib/export/exportDialog.svelte";

    const PHASE_LABELS: Record<ExportPhase, string> = {
        preparing: "Preparing",
        rendering: "Rendering",
        encoding: "Encoding",
    };

    const current = $derived(exportDialog.progress);
</script>

{#if exportDialog.quickError}
    <div class="overlay fixed z-1000 flex items-center justify-center">
        <div class="modal flex flex-col gap-3.5 px-9 py-7 rounded-[10px]" role="alertdialog" aria-labelledby="export-failed">
            <p id="export-failed" class="label m-0">Export failed</p>
            <p class="reason m-0">{exportDialog.quickError}</p>
            <button class="dismiss self-center px-4 py-1.25 border-0 rounded-[6px]" onclick={exportDialog.dismissError}>OK</button>
        </div>
    </div>
{:else if exportDialog.quick && current}
    <div class="overlay fixed z-1000 flex items-center justify-center">
        <div class="modal flex flex-col gap-3.5 px-9 py-7 rounded-[10px]">
            <p class="label m-0" role="status">
                {PHASE_LABELS[current.phase]}…{current.indeterminate ? "" : ` ${Math.round(current.progress * 100)}%`}
            </p>
            <div class="track relative w-full h-1 rounded-[2px]">
                <div class="bar h-full rounded-[2px]" style="width: {current.progress * 100}%"></div>
                {#if current.indeterminate}
                    <!-- The part still to come, moving to show work without a measure. -->
                    <div class="pending absolute top-0 bottom-0 right-0" style="left: {current.progress * 100}%"></div>
                {/if}
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

    .reason {
        font-size: 13px;
        color: var(--color3);
        text-align: center;
        max-width: 360px;
    }

    .dismiss {
        font-size: 13px;
        background: var(--bg4);
        color: var(--color2);
    }

    .dismiss:hover {
        background: var(--bg5);
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
