<!-- lib/components/settings/sections/HomeLabSection.svelte -->
<script lang="ts">
    import GearList from "../GearList.svelte";
</script>

<div class="section flex flex-col gap-4">
    <p class="intro m-0">
        The chemistry you develop with at home. Picking a setup for a roll fills in its dilution and temperature; the
        time is kept with each roll, since it depends on the film.
    </p>
    <GearList kind="chemistry" title="Chemistry setups" />
</div>

<style>
    .intro {
        padding: 0 12px;
        font-size: 12.5px;
        color: var(--color3);
    }
</style>
