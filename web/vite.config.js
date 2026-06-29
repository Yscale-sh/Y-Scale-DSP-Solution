import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import tailwindcss from '@tailwindcss/vite'

// IMPORTANT: base must be relative ('./') — the Rust server embeds web/dist and
// serves assets from the filesystem root, so every asset URL must be relative.
export default defineConfig({
  base: './',
  plugins: [vue(), tailwindcss()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
})
