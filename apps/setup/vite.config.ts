import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri serves the dev build on a fixed port and loads the production build
// from dist/, which tauri.conf.json names.
export default defineConfig({
  build: {
    outDir: 'dist',
    target: 'es2024',
  },
  clearScreen: false,
  plugins: [react()],
  server: {
    port: 1420,
    strictPort: true,
  },
});
