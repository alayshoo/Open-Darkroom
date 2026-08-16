<!-- lib/components/settings/SettingsToggle.svelte -->
<script lang="ts">
    /* The plain on/off switch every settings row uses. Same material as the
       toolbar toggles, minus the icons — the stops are fixed here so the
       switch is identical wherever it appears. */
    import GlassToggle from "$lib/components/inputs/GlassToggle.svelte";

    let {
        checked,
        label,
        onclick,
    }: {
        checked: boolean;
        label: string;
        onclick?: () => void;
    } = $props();
</script>

<GlassToggle
    {checked}
    {label}
    {onclick}
    width={48}
    thumb={{ off: { x: 3, w: 22 }, on: { x: 23, w: 22 } }}
/>
