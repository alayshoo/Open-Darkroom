// Tauri doesn't have a Node.js server to do proper SSR
// so we use adapter-static with a fallback to index.html to put the site in SPA mode
// See: https://svelte.dev/docs/kit/single-page-apps
// See: https://v2.tauri.app/start/frontend/sveltekit/ for more info
import adapter from "@sveltejs/adapter-static";
import { vitePreprocess } from "@sveltejs/vite-plugin-svelte";

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  // The app's own components always compile in runes mode, whether or not
  // they use a rune themselves. Dependencies keep the mode they infer.
  vitePlugin: {
    dynamicCompileOptions: ({ filename }) =>
      filename.includes("/node_modules/") ? undefined : { runes: true },
  },
  kit: {
    adapter: adapter({
      fallback: "index.html",
    }),
  },
};

export default config;
