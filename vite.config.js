import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [sveltekit()],
	server: {
		port: 1420,
		strictPort: true
	},
	build: {
		target: ['es2021', 'chrome100', 'safari13'],
		minify: 'esbuild',
		sourcemap: false
	}
});
