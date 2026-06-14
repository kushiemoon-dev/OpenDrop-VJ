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
		// Force Vite to pre-bundle these CJS/UMD packages into ESM.
		// butterchurn is a UMD bundle that needs conversion to work with import().
		include: ['butterchurn', 'butterchurn-presets']
	},
	ssr: {
		// Never try to SSR these browser-only packages.
		noExternal: ['butterchurn', 'butterchurn-presets']
	},
});
