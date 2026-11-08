<script lang="ts">
    let { onclick }: { onclick: () => void } = $props();
</script>

<button
    class="export-button flex w-22.75 h-7 justify-center items-center border-0 rounded-[6px]"
    {onclick}
    title="Export…"
>
    Export…
</button>

<style>
    .export-button {
        background: var(--bg2);
        color: var(--color2);

        font-size: 16px;
        font-weight: 500;

        /* Naming a transition here replaces the theme's colour fade from
           palette.css, so the two theme properties are restated. */
        transition:
            color 0.5s ease,
            background-color 0.5s ease;
    }
</style>
