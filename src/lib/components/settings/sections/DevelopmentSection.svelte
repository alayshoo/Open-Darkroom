<!-- lib/components/settings/sections/DevelopmentSection.svelte -->
<script lang="ts">
    import GearList from "../GearList.svelte";
</script>

<div class="section flex flex-col gap-4">
    <GearList kind="chemistry" />
    <GearList kind="lab" />
</div>
