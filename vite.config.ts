import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // Don't inline any asset as a data: URI. Vite's default 4 KB inlining
    // bites us with the flag SVGs (all <3 KB): once inlined, the resulting
    // `background-image: url(data:...)` gets blocked by the production CSP
    // (`default-src 'self'` has no `data:`). Shipping every asset as a
    // discrete file makes dev and prod behave identically. The CSP also
    // allows `data:` for `img-src` now (belt-and-braces).
    assetsInlineLimit: 0,
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        settings: resolve(__dirname, "settings.html"),
        permissions: resolve(__dirname, "permissions.html"),
        "vulkan-warning": resolve(__dirname, "vulkan-warning.html"),
        welcome: resolve(__dirname, "welcome.html"),
      },
    },
  },
});
