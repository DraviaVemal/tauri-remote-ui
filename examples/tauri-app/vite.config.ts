import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  server: {
    port: 3001,
    proxy: {
      '/remote_ui_info': {
        target: 'http://localhost:9090',
        changeOrigin: true,
      },
      '/remote_ui_ws': {
        target: 'http://localhost:9090',
        changeOrigin: true,
        ws: true
      },
      '/remote_ui_disconnect': {
        target: 'http://localhost:9090',
        changeOrigin: true,
      },
    },
  },
  build: {
    sourcemap: true,
    minify: false
  }
});