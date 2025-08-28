import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3001,
  },
  resolve: {
    alias: {
      '@': '/src',
    },
  },
  build: {
    sourcemap: true,
    minify: false
  }
});