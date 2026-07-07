import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';

vi.mock('./cloud-presets.js', () => ({
	getOrCreateCloudToken: vi.fn(() => 'token-abc'),
	setCloudToken: vi.fn(),
	parsePresetFile: vi.fn((text: string) => JSON.parse(text)),
	getCloudPresetIndex: vi.fn(async () => []),
	uploadPreset: vi.fn(async () => ({ id: 'new-id' })),
	deleteCloudPreset: vi.fn(async () => {}),
	renameCloudPreset: vi.fn(async () => {}),
}));

import * as cloudPresetsApi from './cloud-presets.js';
import {
	cloudPresetsState, initCloudPresets, refreshCloudPresets, onCloudPresetFilePick,
	copyCloudToken, linkCloudDevice, renameCloudPresetEntry, deleteCloudPresetEntry,
} from './cloud-presets-store.svelte.js';

function fakeFileEvent(file: { name: string; text: () => Promise<string> } | undefined): Event {
	const input = { files: file ? [file] : [], value: '' };
	return { target: input } as unknown as Event;
}

describe('cloud-presets-store', () => {
	beforeEach(() => {
		vi.clearAllMocks();
		cloudPresetsState.token = '';
		cloudPresetsState.presets = [];
		cloudPresetsState.error = null;
		cloudPresetsState.copyLabel = 'Copy my token';
	});

	describe('initCloudPresets', () => {
		it('gets/creates the token then refreshes the list', async () => {
			vi.mocked(cloudPresetsApi.getCloudPresetIndex).mockResolvedValueOnce([
				{ id: '1', name: '☁ foo', sizeBytes: 10, uploadedAt: 0 },
			]);
			await initCloudPresets();
			expect(cloudPresetsState.token).toBe('token-abc');
			expect(cloudPresetsState.presets).toHaveLength(1);
		});
	});

	describe('refreshCloudPresets', () => {
		it('reloads the list with the current token', async () => {
			cloudPresetsState.token = 'my-token';
			await refreshCloudPresets();
			expect(cloudPresetsApi.getCloudPresetIndex).toHaveBeenCalledWith('my-token');
		});
	});

	describe('linkCloudDevice', () => {
		it('persists the new token and refreshes the list with it', async () => {
			await linkCloudDevice('other-token');
			expect(cloudPresetsApi.setCloudToken).toHaveBeenCalledWith('other-token');
			expect(cloudPresetsState.token).toBe('other-token');
			expect(cloudPresetsApi.getCloudPresetIndex).toHaveBeenCalledWith('other-token');
		});
	});

	describe('renameCloudPresetEntry', () => {
		it('renames then refreshes', async () => {
			cloudPresetsState.token = 'token-abc';
			await renameCloudPresetEntry('id-1', 'Nouveau nom');
			expect(cloudPresetsApi.renameCloudPreset).toHaveBeenCalledWith('token-abc', 'id-1', 'Nouveau nom');
			expect(cloudPresetsApi.getCloudPresetIndex).toHaveBeenCalled();
		});
	});

	describe('deleteCloudPresetEntry', () => {
		it('deletes then refreshes', async () => {
			cloudPresetsState.token = 'token-abc';
			await deleteCloudPresetEntry('id-1');
			expect(cloudPresetsApi.deleteCloudPreset).toHaveBeenCalledWith('token-abc', 'id-1');
			expect(cloudPresetsApi.getCloudPresetIndex).toHaveBeenCalled();
		});
	});

	describe('copyCloudToken', () => {
		afterEach(() => {
			vi.useRealTimers();
			vi.unstubAllGlobals();
		});

		it('copies the token, shows "Copied!" then reverts to the initial label after 1500ms', () => {
			vi.useFakeTimers();
			const writeText = vi.fn();
			vi.stubGlobal('navigator', { clipboard: { writeText } });
			cloudPresetsState.token = 'token-abc';
			copyCloudToken();
			expect(writeText).toHaveBeenCalledWith('token-abc');
			expect(cloudPresetsState.copyLabel).toBe('Copied!');
			vi.advanceTimersByTime(1500);
			expect(cloudPresetsState.copyLabel).toBe('Copy my token');
		});
	});

	describe('onCloudPresetFilePick', () => {
		it("does nothing if no file is selected", async () => {
			await onCloudPresetFilePick(fakeFileEvent(undefined));
			expect(cloudPresetsApi.uploadPreset).not.toHaveBeenCalled();
		});

		it('uploads the parsed file (name without .json extension) and refreshes on success', async () => {
			cloudPresetsState.token = 'token-abc';
			const file = { name: 'my-preset.json', text: async () => '{"frame_eqs_str":"a=1;"}' };
			await onCloudPresetFilePick(fakeFileEvent(file));
			expect(cloudPresetsApi.uploadPreset).toHaveBeenCalledWith('token-abc', 'my-preset', { frame_eqs_str: 'a=1;' });
			expect(cloudPresetsApi.getCloudPresetIndex).toHaveBeenCalled();
			expect(cloudPresetsState.error).toBeNull();
		});

		it("stores the error returned by uploadPreset without refreshing the list", async () => {
			vi.mocked(cloudPresetsApi.uploadPreset).mockResolvedValueOnce({ error: 'Quota exceeded' });
			const file = { name: 'x.json', text: async () => '{}' };
			await onCloudPresetFilePick(fakeFileEvent(file));
			expect(cloudPresetsState.error).toBe('Quota exceeded');
			expect(cloudPresetsApi.getCloudPresetIndex).not.toHaveBeenCalled();
		});

		it('stores an error message if the file is invalid JSON', async () => {
			const file = { name: 'bad.json', text: async () => 'not json' };
			await onCloudPresetFilePick(fakeFileEvent(file));
			expect(cloudPresetsState.error).toBeTruthy();
			expect(cloudPresetsApi.uploadPreset).not.toHaveBeenCalled();
		});
	});
});
