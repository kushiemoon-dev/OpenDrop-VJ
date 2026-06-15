import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			adapter: adapter({
				fallback: 'index.html'
			})
		})
	],
	server: {
		port: 1420
	},
	optimizeDeps: {
		// Force Vite to pre-bundle this CJS/UMD package into ESM.
		include: ['butterchurn']
	},
	ssr: {
		// Never try to SSR this browser-only package.
		noExternal: ['butterchurn']
	},
});
