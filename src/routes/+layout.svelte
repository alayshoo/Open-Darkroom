<!-- routes/+layout.svelte -->
<script lang="ts">
  import "$lib/styles/global.css";
  import "$lib/styles/palette.css";

  import "@fontsource-variable/figtree";

  let { children } = $props();
</script>

{@render children()}
