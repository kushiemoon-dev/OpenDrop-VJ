import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import PlaylistPanel from '$lib/components/PlaylistPanel.svelte';

vi.mock('@tauri-apps/api/core');

describe('PlaylistPanel', () => {
	const mockPlaylist = {
		name: 'Test Playlist',
		items: [
			{ name: 'Preset One', path: '/presets/one.milk' },
			{ name: 'Preset Two', path: '/presets/two.milk' },
			{ name: 'Preset Three', path: '/presets/three.milk' }
		],
		current_index: 0,
		shuffle: false,
		auto_cycle: false,
		cycle_duration_secs: 30
	};

	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(invoke).mockResolvedValue(undefined);
	});

	describe('rendering', () => {
		it('renders panel header with deck number', () => {
			render(PlaylistPanel, { props: { deckId: 0 } });
			expect(screen.getByText('Playlist - Deck 1')).toBeInTheDocument();
		});

		it('shows preset count', () => {
			render(PlaylistPanel, { props: { playlist: mockPlaylist } });
			expect(screen.getByText('3 presets')).toBeInTheDocument();
		});

		it('shows empty state when no items', () => {
			render(PlaylistPanel, {
				props: {
					playlist: { ...mockPlaylist, items: [] }
				}
			});
			expect(screen.getByText('No presets in playlist')).toBeInTheDocument();
			expect(screen.getByText('Click + on presets to add')).toBeInTheDocument();
		});
	});

	describe('playlist items', () => {
		it('renders playlist items', () => {
			render(PlaylistPanel, { props: { playlist: mockPlaylist } });

			expect(screen.getByText('Preset One')).toBeInTheDocument();
			expect(screen.getByText('Preset Two')).toBeInTheDocument();
			expect(screen.getByText('Preset Three')).toBeInTheDocument();
		});

		it('shows item indices', () => {
			render(PlaylistPanel, { props: { playlist: mockPlaylist } });

			expect(screen.getByText('1')).toBeInTheDocument();
			expect(screen.getByText('2')).toBeInTheDocument();
			expect(screen.getByText('3')).toBeInTheDocument();
		});

		it('highlights current item', () => {
			const { container } = render(PlaylistPanel, {
				props: { playlist: { ...mockPlaylist, current_index: 1 } }
			});

			const currentItem = container.querySelector('.playlist-item.current');
			expect(currentItem).toBeInTheDocument();
		});

		it('truncates long preset names', () => {
			const longNamePlaylist = {
				...mockPlaylist,
				items: [{ name: 'This is a very long preset name that should be truncated', path: '/test.milk' }]
			};
			render(PlaylistPanel, { props: { playlist: longNamePlaylist } });

			// truncateName(name, maxLen=25) takes 22 chars + "..."
			expect(screen.getByText(/This is a very long pr\.\.\./)).toBeInTheDocument();
		});
	});

	describe('navigation controls', () => {
		it('calls playlist_next when next button is clicked', async () => {
			const onUpdate = vi.fn();
			render(PlaylistPanel, { props: { playlist: mockPlaylist, deckId: 0, onUpdate } });

			const nextButton = screen.getByTitle('Next');
			await fireEvent.click(nextButton);

			expect(invoke).toHaveBeenCalledWith('playlist_next', { deckId: 0 });
			expect(onUpdate).toHaveBeenCalled();
		});

		it('calls playlist_previous when previous button is clicked', async () => {
			const onUpdate = vi.fn();
			render(PlaylistPanel, { props: { playlist: mockPlaylist, deckId: 0, onUpdate } });

			const prevButton = screen.getByTitle('Previous');
			await fireEvent.click(prevButton);

			expect(invoke).toHaveBeenCalledWith('playlist_previous', { deckId: 0 });
			expect(onUpdate).toHaveBeenCalled();
		});

		it('disables navigation buttons when playlist is empty', () => {
			render(PlaylistPanel, {
				props: { playlist: { ...mockPlaylist, items: [] } }
			});

			const nextButton = screen.getByTitle('Next') as HTMLButtonElement;
			const prevButton = screen.getByTitle('Previous') as HTMLButtonElement;

			expect(nextButton.disabled).toBe(true);
			expect(prevButton.disabled).toBe(true);
		});
	});

	describe('shuffle toggle', () => {
		it('shows shuffle button', () => {
			render(PlaylistPanel, { props: { playlist: mockPlaylist } });
			expect(screen.getByTitle('Shuffle')).toBeInTheDocument();
		});

		it('toggles shuffle mode', async () => {
			const onUpdate = vi.fn();
			render(PlaylistPanel, { props: { playlist: mockPlaylist, deckId: 0, onUpdate } });

			const shuffleButton = screen.getByTitle('Shuffle');
			await fireEvent.click(shuffleButton);

			expect(invoke).toHaveBeenCalledWith('playlist_set_settings', {
				deckId: 0,
				shuffle: true
			});
		});

		it('shows active state when shuffle is enabled', () => {
			const { container } = render(PlaylistPanel, {
				props: { playlist: { ...mockPlaylist, shuffle: true } }
			});

			const shuffleButton = container.querySelector('[title="Shuffle"].active');
			expect(shuffleButton).toBeInTheDocument();
		});
	});

	describe('auto-cycle toggle', () => {
		it('shows auto-cycle button', () => {
			render(PlaylistPanel, { props: { playlist: mockPlaylist } });
			expect(screen.getByTitle('Auto-cycle')).toBeInTheDocument();
		});

		it('toggles auto-cycle mode', async () => {
			const onUpdate = vi.fn();
			render(PlaylistPanel, { props: { playlist: mockPlaylist, deckId: 0, onUpdate } });

			const cycleButton = screen.getByTitle('Auto-cycle');
			await fireEvent.click(cycleButton);

			expect(invoke).toHaveBeenCalledWith('playlist_set_settings', {
				deckId: 0,
				autoCycle: true
			});
		});

		it('shows cycle duration input when auto-cycle is enabled', () => {
			render(PlaylistPanel, {
				props: { playlist: { ...mockPlaylist, auto_cycle: true } }
			});

			expect(screen.getByText('Cycle every')).toBeInTheDocument();
			expect(screen.getByRole('spinbutton')).toBeInTheDocument();
		});

		it('hides cycle duration input when auto-cycle is disabled', () => {
			render(PlaylistPanel, {
				props: { playlist: { ...mockPlaylist, auto_cycle: false } }
			});

			expect(screen.queryByText('Cycle every')).not.toBeInTheDocument();
		});
	});

	describe('clear all', () => {
		it('shows clear all button', () => {
			render(PlaylistPanel, { props: { playlist: mockPlaylist } });
			expect(screen.getByTitle('Clear all')).toBeInTheDocument();
		});

		it('disables clear button when playlist is empty', () => {
			render(PlaylistPanel, {
				props: { playlist: { ...mockPlaylist, items: [] } }
			});

			const clearButton = screen.getByTitle('Clear all') as HTMLButtonElement;
			expect(clearButton.disabled).toBe(true);
		});
	});

	describe('item removal', () => {
		it('calls playlist_remove when remove button is clicked', async () => {
			const onUpdate = vi.fn();
			render(PlaylistPanel, { props: { playlist: mockPlaylist, deckId: 0, onUpdate } });

			const removeButtons = screen.getAllByTitle('Remove');
			await fireEvent.click(removeButtons[0]);

			expect(invoke).toHaveBeenCalledWith('playlist_remove', { deckId: 0, index: 0 });
			expect(onUpdate).toHaveBeenCalled();
		});
	});

	describe('jump to item', () => {
		it('calls playlist_jump_to when item is clicked', async () => {
			const onUpdate = vi.fn();
			render(PlaylistPanel, { props: { playlist: mockPlaylist, deckId: 0, onUpdate } });

			const itemButton = screen.getByText('Preset Two');
			await fireEvent.click(itemButton);

			expect(invoke).toHaveBeenCalledWith('playlist_jump_to', { deckId: 0, index: 1 });
			expect(onUpdate).toHaveBeenCalled();
		});
	});
});
