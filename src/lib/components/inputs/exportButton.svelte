<script lang="ts">
    let {
        onexport,
    }: {
        onexport: () => Promise<void>;
    } = $props();

    let isExporting = $state(false);

    async function handleExport() {
        if (isExporting) return;
        isExporting = true;
        try {
            await onexport();
        } catch (e) {
            // Dialog cancelled or error — ignore silently
        } finally {
            isExporting = false;
        }
    }
</script>

<div class="container">
    <button class="export-button" onclick={handleExport} disabled={isExporting}>
        Export
    </button>
    <div class="arrow-button">
        <svg width="13" height="8" viewBox="0 0 13 8" fill="none" xmlns="http://www.w3.org/2000/svg">
            <path
                d="M0.200379 7.20012C-0.0582523 6.91434 -0.0675507 6.48192 0.178557 6.18529L4.71051 0.722961C5.51027 -0.240983 6.98917 -0.240982 7.78893 0.722961L12.3209 6.18529C12.567 6.48193 12.5577 6.91434 12.2991 7.20012C11.981 7.55156 11.4248 7.53767 11.1247 7.17081L6.63672 1.68484C6.43666 1.4403 6.06277 1.4403 5.86272 1.68484L1.37477 7.17081C1.07465 7.53767 0.518432 7.55156 0.200379 7.20012Z"
                fill="currentColor"
            />
        </svg>
    </div>
</div>

<style>
    .container {
        display: flex;
        flex-direction: row;
        height: 28px;
        gap: 1px;
    }

    .export-button {
        display: flex;
        width: 63px;
        height: 28px;
        justify-content: center;
        align-items: center;

        background: var(--bg2);
        color: var(--color2);
        border: none;

        font-size: 16px;
        font-weight: 500;

        border-top-left-radius: 6px;
        border-bottom-left-radius: 6px;
        border-top-right-radius: 3px;
        border-bottom-right-radius: 3px;

        transition: opacity 0.2s ease;
    }

    .export-button:disabled {
        opacity: 0.6;
    }

    .arrow-button {
        display: flex;
        width: 27px;
        height: 28px;
        justify-content: center;
        align-items: center;

        background: var(--bg2);
        color: var(--color2);
        transition: color 0.5s ease;

        font-size: 16px;
        font-weight: 500;

        border-top-left-radius: 3px;
        border-bottom-left-radius: 3px;
        border-top-right-radius: 6px;
        border-bottom-right-radius: 6px;
    }
</style>
