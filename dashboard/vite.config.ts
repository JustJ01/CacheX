import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The metrics API sends CORS headers itself, so the dev server does not need
// a proxy; the dashboard polls each node's /metrics endpoint directly.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    host: "127.0.0.1",
  },
});
