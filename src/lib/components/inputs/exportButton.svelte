<script lang="ts">
    import { type ExportSettings } from "$lib/types/exportSettings";

    let {
        onexport,
        menuOpen = $bindable(false),
        settings,
    }: {
        onexport: () => Promise<void>;
        menuOpen?: boolean;
        settings: ExportSettings;
    } = $props();

    let isExporting = $state(false);

    async function handleExport() {
        if (isExporting) return;
        menuOpen = false;
        isExporting = true;
        try {
            await onexport();
        } catch {
            // Dialog cancelled or error — ignore silently
        } finally {
            isExporting = false;
        }
    }

    function toggleMenu() {
        menuOpen = !menuOpen;
    }
</script>

<div class="wrapper relative flex flex-row h-7 gap-0.25">
    <button
        class="export-button flex w-15.75 h-7 justify-center items-center border-0 rounded-l-[6px] rounded-r-[3px]"
        onclick={handleExport}
        disabled={isExporting}
    >
        Export
    </button>
    <button
        class="arrow-button flex w-6.75 h-7 justify-center items-center border-0 cursor-pointer rounded-l-[3px] rounded-r-[6px]"
        class:open={menuOpen}
        onclick={toggleMenu}
        aria-label="Export settings"
        aria-expanded={menuOpen}
    >
        <svg width="13" height="8" viewBox="0 0 13 8" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path
                d="M0.200379 7.20012C-0.0582523 6.91434 -0.0675507 6.48192 0.178557 6.18529L4.71051 0.722961C5.51027 -0.240983 6.98917 -0.240982 7.78893 0.722961L12.3209 6.18529C12.567 6.48193 12.5577 6.91434 12.2991 7.20012C11.981 7.55156 11.4248 7.53767 11.1247 7.17081L6.63672 1.68484C6.43666 1.4403 6.06277 1.4403 5.86272 1.68484L1.37477 7.17081C1.07465 7.53767 0.518432 7.55156 0.200379 7.20012Z"
                fill="currentColor"
            />
        </svg>
    </button>
</div>

<style>
    .export-button {
        background: var(--bg2);
        color: var(--color2);

        font-size: 16px;
        font-weight: 500;

        /* The theme's 0.5s colour fade comes from a `:root *` rule in
           palette.css, and this scoped declaration replaces it wholesale
           rather than adding to it — so listing only `opacity` here left the
           label snapping from grey to red. Anything that names a transition
           has to re-state the two theme properties. Neither colour has a
           hover state to stay quick for; only the disabled fade does. */
        transition:
            opacity 0.2s ease,
            color 0.5s ease,
            background-color 0.5s ease;
    }

    .export-button:disabled {
        opacity: 0.6;
    }

    .arrow-button {
        background: var(--bg2);
        color: var(--color2);

        font-size: 16px;
        font-weight: 500;

        /* Matches the label it is welded to — at 0.2s the arrow finished its
           swing to red while "Export" was still mid-fade beside it. */
        transition:
            color 0.5s ease,
            background-color 0.5s ease;
    }

    .arrow-button svg {
        transition: transform 0.2s ease;
    }

    .arrow-button.open svg {
        transform: rotate(180deg);
    }
</style>
