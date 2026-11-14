<!-- lib/components/settings/sections/GearSection.svelte -->
<script lang="ts">
    import GearList from "../GearList.svelte";
</script>

<div class="section flex flex-col gap-4">
    <GearList kind="camera" />
    <GearList kind="lens" />
    <GearList kind="scanner" />
</div>

