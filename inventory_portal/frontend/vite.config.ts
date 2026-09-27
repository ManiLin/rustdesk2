import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  // Консоль управления живёт по /managment, а не в корне: в корне сайта нет
  // ничего, кроме пустой 404-заглушки (чтобы сервис не опознавался по адресу).
  base: "/managment/",
  server: {
    port: 5173,
    proxy: {
      "/api": { target: "http://127.0.0.1:8080", changeOrigin: true },
    },
  },
});
