import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  base: "./",
  plugins: [react()],
  server: {
    host: "0.0.0.0",
    port: 5173,
    proxy: {
      "/api/agentd": {
        target: process.env.TEAMD_AGENTD_BASE_URL ?? "http://127.0.0.1:5140",
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/api\/agentd/, ""),
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq) => {
            const token = process.env.TEAMD_AGENTD_TOKEN;
            if (token) {
              proxyReq.setHeader("Authorization", `Bearer ${token}`);
            }
          });
        }
      }
    }
  },
  preview: {
    host: "0.0.0.0",
    port: 5173
  }
});
