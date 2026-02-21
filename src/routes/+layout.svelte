<!-- routes/+layout.svelte -->
<script lang="ts">
  import "$lib/styles/global.css";
  import "$lib/styles/palette.css";

  import "@fontsource-variable/figtree";

  let { children } = $props();


  // ============ GPU INITIALIZATION ============

  import "$lib/gpu/gpuInit";
  import { type GPUSession } from "$lib/types/gpuTypes";
  import { initializeGPU } from "$lib/gpu/gpuInit"
  import { setContext } from "svelte";

  let gpu = $state<GPUSession | null>(null);
  setContext("gpu", () => gpu);

  initializeGPU().then((session) => {
    gpu = session;
  });
  // ============================================


</script>


<!-- Wait for GPU before starting app -->
{#if gpu}
  {@render children()}
{:else}
  <p>Initializing GPU…</p>
{/if}