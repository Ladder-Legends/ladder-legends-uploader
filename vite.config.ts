import { defineConfig } from 'vite';
import { resolve } from 'path';

// https://vitejs.dev/config/
export default defineConfig({
  resolve: {
    alias: {
      '@': resolve(__dirname, './src'),
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'src/index.html'),
      },
    },
  },
  // Environment variable configuration
  define: {
    'import.meta.env.VITE_API_HOST': JSON.stringify(
      process.env.VITE_API_HOST || 'https://ladderlegendsacademy.com'
    ),
  },
});
