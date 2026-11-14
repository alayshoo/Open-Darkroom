<!-- routes/+layout.svelte -->
<script lang="ts">
  import "$lib/styles/tailwind.css";
  import "$lib/styles/global.css";
  import "$lib/styles/palette.css";
  import "$lib/styles/glass.css";

  import "$lib/components/window/TitleBar.svelte"

  import "@fontsource-variable/figtree";
  import "@fontsource-variable/outfit";

  let { children } = $props();


  // ============ GPU INITIALIZATION ============

  import "$lib/gpu/gpuInit";
  import { type GPUSession } from "$lib/types/gpuTypes";
  import { initializeGPU } from "$lib/gpu/gpuInit"
  import { onDestroy, setContext } from "svelte";
    import TitleBar from "$lib/components/window/TitleBar.svelte";
    import SettingsModal from "$lib/components/settings/SettingsModal.svelte";

  import { createLogger, installErrorHandlers } from "$lib/utils/log";

  onDestroy(installErrorHandlers());

  let gpu = $state<GPUSession | null>(null);
  let gpuFailure = $state<string | null>(null);
  setContext("gpu", () => gpu);

  initializeGPU().then(
    (session) => {
      gpu = session;
    },
    (error) => {
      createLogger("gpu").error("WebGPU could not start", error);
      gpuFailure = error instanceof Error ? error.message : String(error);
    },
  );
  // ============================================


</script>


<!-- Wait for GPU before starting app -->
{#if gpu}
  {@render children()}
{:else}
  <TitleBar></TitleBar>
  <div class="init-div flex w-full items-center justify-center">
    <p>{gpuFailure ? `WebGPU could not start: ${gpuFailure}` : "Initializing GPU…"}</p>
  </div>
{/if}

<!-- Outside the GPU gate and above everything else, title bar included: it
     overlays whichever page is up rather than belonging to one. -->
<SettingsModal />


<style>
  .init-div {
    min-height: 100vh;
  }
</style>