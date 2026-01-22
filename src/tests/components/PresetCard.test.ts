import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import PresetCard from '$lib/components/PresetCard.svelte';
import * as favoritesStore from '$lib/stores/favorites';

describe('PresetCard', () => {
	const defaultProps = {
		name: 'Test Preset',
		path: '/presets/test.milk'
	};

	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(favoritesStore.isFavorite).mockReturnValue(false);
	});

	describe('rendering', () => {
		it('renders the preset name', () => {
			render(PresetCard, { props: defaultProps });
			expect(screen.getByText('Test Preset')).toBeInTheDocument();
		});

		it('truncates long names', () => {
			const longName = 'This is a very long preset name that should be truncated';
			render(PresetCard, { props: { ...defaultProps, name: longName } });

			// Should show truncated name (32 chars + ...)
			// Original: "This is a very long preset name that should be truncated" (56 chars)
			// Truncated: slice(0,32) + "..." = "This is a very long preset name ..." (includes trailing space)
			expect(screen.getByText('This is a very long preset name ...')).toBeInTheDocument();
		});

		it('shows path in title attribute', () => {
			const { container } = render(PresetCard, { props: defaultProps });
			const card = container.querySelector('.preset-card');
			expect(card).toHaveAttribute('title', '/presets/test.milk');
		});

		it('applies selected class when selected', () => {
			const { container } = render(PresetCard, {
				props: { ...defaultProps, selected: true }
			});
			expect(container.querySelector('.preset-card.selected')).toBeInTheDocument();
		});

		it('does not apply selected class when not selected', () => {
			const { container } = render(PresetCard, {
				props: { ...defaultProps, selected: false }
			});
			expect(container.querySelector('.preset-card.selected')).not.toBeInTheDocument();
		});
	});

	describe('click interactions', () => {
		it('calls onclick when card is clicked', async () => {
			const onclick = vi.fn();
			const { container } = render(PresetCard, { props: { ...defaultProps, onclick } });

			const card = container.querySelector('.preset-card')!;
			await fireEvent.click(card);

			expect(onclick).toHaveBeenCalled();
		});

		it('calls ondblclick when card is double-clicked', async () => {
			const ondblclick = vi.fn();
			const { container } = render(PresetCard, { props: { ...defaultProps, ondblclick } });

			const card = container.querySelector('.preset-card')!;
			await fireEvent.dblClick(card);

			expect(ondblclick).toHaveBeenCalled();
		});

		it('calls onclick when Enter key is pressed', async () => {
			const onclick = vi.fn();
			const { container } = render(PresetCard, { props: { ...defaultProps, onclick } });

			const card = container.querySelector('.preset-card')!;
			await fireEvent.keyDown(card, { key: 'Enter' });

			expect(onclick).toHaveBeenCalled();
		});

		it('does not call onclick for other keys', async () => {
			const onclick = vi.fn();
			const { container } = render(PresetCard, { props: { ...defaultProps, onclick } });

			const card = container.querySelector('.preset-card')!;
			await fireEvent.keyDown(card, { key: 'Space' });

			expect(onclick).not.toHaveBeenCalled();
		});
	});

	describe('add to playlist button', () => {
		it('shows add button when onAddToPlaylist is provided', () => {
			const onAddToPlaylist = vi.fn();
			render(PresetCard, { props: { ...defaultProps, onAddToPlaylist } });

			expect(screen.getByTitle('Add to playlist')).toBeInTheDocument();
		});

		it('hides add button when onAddToPlaylist is not provided', () => {
			render(PresetCard, { props: defaultProps });

			expect(screen.queryByTitle('Add to playlist')).not.toBeInTheDocument();
		});

		it('calls onAddToPlaylist with preset data when clicked', async () => {
			const onAddToPlaylist = vi.fn();
			render(PresetCard, { props: { ...defaultProps, onAddToPlaylist } });

			const addButton = screen.getByTitle('Add to playlist');
			await fireEvent.click(addButton);

			expect(onAddToPlaylist).toHaveBeenCalledWith({
				name: 'Test Preset',
				path: '/presets/test.milk'
			});
		});

		it('stops event propagation when add button is clicked', async () => {
			const onclick = vi.fn();
			const onAddToPlaylist = vi.fn();
			render(PresetCard, { props: { ...defaultProps, onclick, onAddToPlaylist } });

			const addButton = screen.getByTitle('Add to playlist');
			await fireEvent.click(addButton);

			// onclick should not be called because event was stopped
			expect(onclick).not.toHaveBeenCalled();
			expect(onAddToPlaylist).toHaveBeenCalled();
		});
	});

	describe('favorite button', () => {
		it('renders favorite button', () => {
			render(PresetCard, { props: defaultProps });
			expect(screen.getByTitle('Add to favorites')).toBeInTheDocument();
		});

		it('shows "Remove from favorites" title when preset is favorite', () => {
			vi.mocked(favoritesStore.isFavorite).mockReturnValue(true);
			render(PresetCard, { props: defaultProps });

			expect(screen.getByTitle('Remove from favorites')).toBeInTheDocument();
		});

		it('calls toggleFavorite when favorite button is clicked', async () => {
			render(PresetCard, { props: defaultProps });

			const favoriteButton = screen.getByTitle('Add to favorites');
			await fireEvent.click(favoriteButton);

			expect(favoritesStore.toggleFavorite).toHaveBeenCalledWith('/presets/test.milk');
		});

		it('stops event propagation when favorite button is clicked', async () => {
			const onclick = vi.fn();
			render(PresetCard, { props: { ...defaultProps, onclick } });

			const favoriteButton = screen.getByTitle('Add to favorites');
			await fireEvent.click(favoriteButton);

			// onclick should not be called because event was stopped
			expect(onclick).not.toHaveBeenCalled();
		});

		it('applies active class when preset is favorite', () => {
			vi.mocked(favoritesStore.isFavorite).mockReturnValue(true);
			const { container } = render(PresetCard, { props: defaultProps });

			expect(container.querySelector('.favorite-btn.active')).toBeInTheDocument();
		});

		it('does not apply active class when preset is not favorite', () => {
			vi.mocked(favoritesStore.isFavorite).mockReturnValue(false);
			const { container } = render(PresetCard, { props: defaultProps });

			expect(container.querySelector('.favorite-btn.active')).not.toBeInTheDocument();
		});

		it('shows filled star icon when preset is favorite', () => {
			vi.mocked(favoritesStore.isFavorite).mockReturnValue(true);
			const { container } = render(PresetCard, { props: defaultProps });

			const starSvg = container.querySelector('.favorite-btn svg');
			expect(starSvg).toHaveAttribute('fill', 'currentColor');
		});

		it('shows empty star icon when preset is not favorite', () => {
			vi.mocked(favoritesStore.isFavorite).mockReturnValue(false);
			const { container } = render(PresetCard, { props: defaultProps });

			const starSvg = container.querySelector('.favorite-btn svg');
			expect(starSvg).toHaveAttribute('fill', 'none');
		});
	});

	describe('accessibility', () => {
		it('card has role="button"', () => {
			const { container } = render(PresetCard, { props: defaultProps });
			const card = container.querySelector('.preset-card');
			expect(card).toHaveAttribute('role', 'button');
		});

		it('has tabindex="0"', () => {
			const { container } = render(PresetCard, { props: defaultProps });
			const card = container.querySelector('.preset-card');
			expect(card).toHaveAttribute('tabindex', '0');
		});

		it('favorite button has aria-label', () => {
			render(PresetCard, { props: defaultProps });
			const button = screen.getByTitle('Add to favorites');
			expect(button).toHaveAttribute('aria-label', 'Add to favorites');
		});

		it('add button has aria-label when present', () => {
			const onAddToPlaylist = vi.fn();
			render(PresetCard, { props: { ...defaultProps, onAddToPlaylist } });

			const button = screen.getByTitle('Add to playlist');
			expect(button).toHaveAttribute('aria-label', 'Add to playlist');
		});
	});
});
