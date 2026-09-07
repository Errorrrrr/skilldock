import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'
import { fileURLToPath, URL } from 'node:url'

export default defineConfig({
  plugins: [vue(), tailwindcss()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  server: { port: 1420, strictPort: true, watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**', '**/work/**'] } },
  clearScreen: false,
})
