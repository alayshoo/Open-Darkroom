<!-- lib/components/settings/sections/GearSection.svelte -->
<script lang="ts">
    import GearList from "../GearList.svelte";
</script>

<div class="section flex flex-col gap-4">
    <p class="intro m-0">
        The cameras, lenses and scanners a roll can be shot and scanned with. They are kept in the catalog and go with it.
        Lenses that share a camera's mount are offered first for its rolls.
    </p>
    <GearList kind="camera" />
    <GearList kind="lens" />
    <GearList kind="scanner" />
</div>

<style>
    .intro {
        padding: 0 12px;
        font-size: 12.5px;
        color: var(--color3);
    }
</style>
