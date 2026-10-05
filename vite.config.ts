import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    rollupOptions: {
      input: {
        palette: "palette.html",
        settings: "settings.html",
        overlay: "overlay.html",
      },
    },
  },
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
