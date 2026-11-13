import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// The Tauri host loads `dist/` in release and the dev server (port 1420) in development.
export default defineConfig({
  plugins: [solid()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
  },
});
