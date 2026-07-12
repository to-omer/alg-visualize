import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig({
  root: new URL(".", import.meta.url).pathname,
  base: "./",
  plugins: [react()],
  build: {
    outDir: "../../dist",
    emptyOutDir: true,
    sourcemap: true,
    target: "es2024",
  },
  worker: {
    format: "es",
  },
});
