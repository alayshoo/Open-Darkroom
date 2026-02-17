<script lang="ts">
  import "$lib/styles/global.css";

  import "$lib/components/TitleBar.svelte"

  import "$lib/components/SingleValSlider.svelte";
  import "$lib/components/PrecisionVal.svelte";

  import "@fontsource-variable/figtree";

  let { children } = $props();
</script>

{@render children()}
