import { fileURLToPath, URL } from "node:url";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "vite";

// O Tauri sobe o Vite na porta 1420 (ver `tauri.conf.json`) e lê o `dist`.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // O build de produção não leva mapas de código-fonte nem comentários de
  // licença: o que vai para dentro do instalador é só o código minificado.
  build: { sourcemap: false, minify: true, cssMinify: true },
});
