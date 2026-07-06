// workers/presets-cloud/src/index.ts
import { readIndex, handleUpload, handleGetPreset, handleRename, handleDelete } from './handlers.js';

export interface Env {
	PRESETS_BUCKET: R2Bucket;
}

function withCors(res: Response): Response {
	const headers = new Headers(res.headers);
	headers.set('Access-Control-Allow-Origin', '*');
	headers.set('Access-Control-Allow-Methods', 'GET, POST, PATCH, DELETE, OPTIONS');
	headers.set('Access-Control-Allow-Headers', 'Content-Type');
	return new Response(res.body, { status: res.status, headers });
}

export default {
	async fetch(request: Request, env: Env): Promise<Response> {
		if (request.method === 'OPTIONS') return withCors(new Response(null, { status: 204 }));

		const url = new URL(request.url);
		const token = url.searchParams.get('token') ?? '';

		if (request.method === 'POST' && url.pathname === '/presets') {
			const body = await request.json() as { token?: string; name?: string; data?: unknown };
			const result = await handleUpload(env.PRESETS_BUCKET, body.token ?? '', body.name ?? '', body.data);
			if ('error' in result) return withCors(Response.json({ error: result.error }, { status: result.status }));
			return withCors(Response.json(result));
		}

		if (request.method === 'GET' && url.pathname === '/presets') {
			return withCors(Response.json(await readIndex(env.PRESETS_BUCKET, token)));
		}

		const idMatch = url.pathname.match(/^\/presets\/([^/]+)$/);
		if (idMatch) {
			const id = idMatch[1];
			if (request.method === 'GET') {
				const preset = await handleGetPreset(env.PRESETS_BUCKET, token, id);
				if (!preset) return withCors(new Response(null, { status: 404 }));
				return withCors(Response.json(preset));
			}
			if (request.method === 'PATCH') {
				const body = await request.json() as { name?: string };
				const result = await handleRename(env.PRESETS_BUCKET, token, id, body.name ?? '');
				if ('error' in result) return withCors(Response.json({ error: result.error }, { status: result.status }));
				return withCors(Response.json(result));
			}
			if (request.method === 'DELETE') {
				const result = await handleDelete(env.PRESETS_BUCKET, token, id);
				if ('error' in result) return withCors(Response.json({ error: result.error }, { status: result.status }));
				return withCors(Response.json(result));
			}
		}

		return withCors(new Response('Not found', { status: 404 }));
	},
};
