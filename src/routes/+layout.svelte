<!-- routes/+layout.svelte -->
<script lang="ts">
  import "$lib/styles/global.css";
  import "$lib/styles/palette.css";

  import "$lib/components/window/TitleBar.svelte"

  import "@fontsource-variable/figtree";

  let { children } = $props();


  // ============ GPU INITIALIZATION ============

  import "$lib/gpu/gpuInit";
  import { type GPUSession } from "$lib/types/gpuTypes";
  import { initializeGPU } from "$lib/gpu/gpuInit"
  import { setContext } from "svelte";
    import TitleBar from "$lib/components/window/TitleBar.svelte";

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
  <TitleBar></TitleBar>
  <div class= "init-div"><p>Initializing GPU…</p></div>
{/if}


<style>
  .init-div {
    display: flex;
    width: 100%;
    min-height: 100vh;
    align-items: center;
    justify-content: center;
  }
</style>