import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Tauri expects a fixed port and fails if it's taken.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
    // WebKitGTK keeps its HTTP cache across dev runs. A cached App.svelte means
    // a fresh Vite never compiles it, so its CSS module fails to load and the
    // shell renders unstyled. Dev server only; builds are unaffected.
    headers: { "Cache-Control": "no-store" },
  },
});
