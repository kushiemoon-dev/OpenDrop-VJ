import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import PresetBrowser from '$lib/components/PresetBrowser.svelte';
import * as favoritesStore from '$lib/stores/favorites';
import * as categoriesStore from '$lib/stores/categories';

describe('PresetBrowser', () => {
	const mockPresets = [
		{ name: 'Acid Trip', path: '/presets/Acid Trip.milk' },
		{ name: 'Blue Wave', path: '/presets/Blue Wave.milk' },
		{ name: 'Cosmic Dance', path: '/presets/Cosmic Dance.milk' },
		{ name: 'Digital Dreams', path: '/presets/Digital Dreams.milk' }
	];

	beforeEach(() => {
		vi.clearAllMocks();
		// Reset mock favorites
		vi.mocked(favoritesStore.getFavoriteCount).mockReturnValue(0);
		vi.mocked(favoritesStore.isFavorite).mockReturnValue(false);
		// Reset mock categories
		vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([]);
	});

	describe('rendering', () => {
		it('renders the header with "Presets" title', () => {
			render(PresetBrowser);
			expect(screen.getByText('Presets')).toBeInTheDocument();
		});

		it('shows preset count', () => {
			render(PresetBrowser, { props: { presets: mockPresets } });
			expect(screen.getByText('4')).toBeInTheDocument();
		});

		it('renders search input', () => {
			render(PresetBrowser);
			expect(screen.getByPlaceholderText('Search presets...')).toBeInTheDocument();
		});
	});

	describe('loading state', () => {
		it('shows loading spinner when loading', () => {
			render(PresetBrowser, { props: { loading: true } });
			expect(screen.getByText('Loading presets...')).toBeInTheDocument();
		});

		it('hides loading spinner when not loading', () => {
			render(PresetBrowser, { props: { loading: false, presets: mockPresets } });
			expect(screen.queryByText('Loading presets...')).not.toBeInTheDocument();
		});
	});

	describe('empty state', () => {
		it('shows "No presets loaded" when presets is empty', () => {
			render(PresetBrowser, { props: { presets: [], loading: false } });
			expect(screen.getByText('No presets loaded')).toBeInTheDocument();
		});

		it('shows "No presets match" when search has no results', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const searchInput = screen.getByPlaceholderText('Search presets...');
			await fireEvent.input(searchInput, { target: { value: 'nonexistent' } });

			expect(screen.getByText(/No presets match "nonexistent"/)).toBeInTheDocument();
		});
	});

	describe('search functionality', () => {
		it('filters presets by name', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const searchInput = screen.getByPlaceholderText('Search presets...');
			await fireEvent.input(searchInput, { target: { value: 'Blue' } });

			// Blue Wave should be visible, others should not
			expect(screen.getByText('Blue Wave')).toBeInTheDocument();
			expect(screen.queryByText('Acid Trip')).not.toBeInTheDocument();
		});

		it('search is case insensitive', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const searchInput = screen.getByPlaceholderText('Search presets...');
			await fireEvent.input(searchInput, { target: { value: 'blue' } });

			expect(screen.getByText('Blue Wave')).toBeInTheDocument();
		});

		it('shows clear button when search has value', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const searchInput = screen.getByPlaceholderText('Search presets...');
			await fireEvent.input(searchInput, { target: { value: 'test' } });

			expect(screen.getByTitle('Clear search')).toBeInTheDocument();
		});

		it('clears search when clear button is clicked', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const searchInput = screen.getByPlaceholderText('Search presets...') as HTMLInputElement;
			await fireEvent.input(searchInput, { target: { value: 'Blue' } });

			const clearButton = screen.getByTitle('Clear search');
			await fireEvent.click(clearButton);

			expect(searchInput.value).toBe('');
			// All presets should be visible again
			expect(screen.getByText('Acid Trip')).toBeInTheDocument();
		});
	});

	describe('selection', () => {
		it('calls onSelect when preset is clicked', async () => {
			const onSelect = vi.fn();
			render(PresetBrowser, { props: { presets: mockPresets, onSelect } });

			const presetCard = screen.getByText('Acid Trip');
			await fireEvent.click(presetCard);

			expect(onSelect).toHaveBeenCalledWith(mockPresets[0]);
		});

		it('highlights currently selected preset', () => {
			const { container } = render(PresetBrowser, {
				props: {
					presets: mockPresets,
					currentPreset: '/presets/Blue Wave.milk'
				}
			});

			const selectedCard = container.querySelector('.preset-card.selected');
			expect(selectedCard).toBeInTheDocument();
		});
	});

	describe('double click to load', () => {
		it('calls onSelect and onLoad when preset is double-clicked', async () => {
			const onSelect = vi.fn();
			const onLoad = vi.fn();
			render(PresetBrowser, { props: { presets: mockPresets, onSelect, onLoad } });

			const presetCard = screen.getByText('Acid Trip');
			await fireEvent.dblClick(presetCard);

			expect(onSelect).toHaveBeenCalledWith(mockPresets[0]);
			expect(onLoad).toHaveBeenCalledWith('/presets/Acid Trip.milk');
		});
	});

	describe('collapse/expand', () => {
		it('can toggle collapsed state', async () => {
			const { container } = render(PresetBrowser, { props: { presets: mockPresets } });

			const toggleButton = screen.getByText('Presets').closest('button')!;
			await fireEvent.click(toggleButton);

			expect(container.querySelector('.preset-browser.collapsed')).toBeInTheDocument();
		});

		it('disables search when collapsed', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const toggleButton = screen.getByText('Presets').closest('button')!;
			await fireEvent.click(toggleButton);

			const searchInput = screen.getByPlaceholderText('Search presets...') as HTMLInputElement;
			expect(searchInput.disabled).toBe(true);
		});
	});

	describe('preset limit', () => {
		it('limits displayed presets to 50', () => {
			const manyPresets = Array.from({ length: 100 }, (_, i) => ({
				name: `Preset ${i}`,
				path: `/presets/preset${i}.milk`
			}));

			render(PresetBrowser, { props: { presets: manyPresets } });

			// Check that we don't have all 100 presets rendered
			expect(screen.queryByText('Preset 60')).not.toBeInTheDocument();
		});
	});

	describe('favorites filter', () => {
		it('renders favorites filter button', () => {
			render(PresetBrowser, { props: { presets: mockPresets } });
			expect(screen.getByTitle('Show favorites only')).toBeInTheDocument();
		});

		it('shows favorite count when there are favorites', () => {
			vi.mocked(favoritesStore.getFavoriteCount).mockReturnValue(3);
			render(PresetBrowser, { props: { presets: mockPresets } });
			expect(screen.getByText('3')).toBeInTheDocument();
		});

		it('does not show favorite count when zero', () => {
			vi.mocked(favoritesStore.getFavoriteCount).mockReturnValue(0);
			const { container } = render(PresetBrowser, { props: { presets: mockPresets } });
			expect(container.querySelector('.fav-count')).not.toBeInTheDocument();
		});

		it('toggles favorites filter when button is clicked', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			const filterButton = screen.getByTitle('Show favorites only');
			await fireEvent.click(filterButton);

			expect(screen.getByTitle('Show all presets')).toBeInTheDocument();
		});

		it('shows "No favorites yet" message when filter is active and no favorites', async () => {
			vi.mocked(favoritesStore.getFavoriteCount).mockReturnValue(0);
			vi.mocked(favoritesStore.isFavorite).mockReturnValue(false);

			render(PresetBrowser, { props: { presets: mockPresets } });

			const filterButton = screen.getByTitle('Show favorites only');
			await fireEvent.click(filterButton);

			expect(screen.getByText('No favorites yet')).toBeInTheDocument();
			expect(screen.getByText('Click the star on any preset to add it')).toBeInTheDocument();
		});

		it('filters presets to show only favorites', async () => {
			vi.mocked(favoritesStore.getFavoriteCount).mockReturnValue(1);
			vi.mocked(favoritesStore.isFavorite).mockImplementation(
				(path) => path === '/presets/Blue Wave.milk'
			);

			render(PresetBrowser, { props: { presets: mockPresets } });

			const filterButton = screen.getByTitle('Show favorites only');
			await fireEvent.click(filterButton);

			expect(screen.getByText('Blue Wave')).toBeInTheDocument();
			expect(screen.queryByText('Acid Trip')).not.toBeInTheDocument();
		});

		it('combines search and favorites filter', async () => {
			vi.mocked(favoritesStore.getFavoriteCount).mockReturnValue(2);
			vi.mocked(favoritesStore.isFavorite).mockImplementation(
				(path) => path === '/presets/Blue Wave.milk' || path === '/presets/Cosmic Dance.milk'
			);

			render(PresetBrowser, { props: { presets: mockPresets } });

			// Enable favorites filter
			const filterButton = screen.getByTitle('Show favorites only');
			await fireEvent.click(filterButton);

			// Search for "Cosmic"
			const searchInput = screen.getByPlaceholderText('Search presets...');
			await fireEvent.input(searchInput, { target: { value: 'Cosmic' } });

			// Only Cosmic Dance should be visible (matches search AND is favorite)
			expect(screen.getByText('Cosmic Dance')).toBeInTheDocument();
			expect(screen.queryByText('Blue Wave')).not.toBeInTheDocument();
		});

		it('disables favorites filter button when collapsed', async () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			// Collapse the panel
			const toggleButton = screen.getByText('Presets').closest('button')!;
			await fireEvent.click(toggleButton);

			const filterButton = screen.getByTitle('Show favorites only') as HTMLButtonElement;
			expect(filterButton.disabled).toBe(true);
		});
	});

	describe('category filter', () => {
		it('does not render category select when no categories', () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([]);
			render(PresetBrowser, { props: { presets: mockPresets } });

			expect(screen.queryByTitle('Filter by category')).not.toBeInTheDocument();
		});

		it('does not render category select when only one category', () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([['presets', 4]]);
			render(PresetBrowser, { props: { presets: mockPresets } });

			expect(screen.queryByTitle('Filter by category')).not.toBeInTheDocument();
		});

		it('renders category select when multiple categories exist', () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([
				['milkdrop', 10],
				['custom', 5]
			]);
			render(PresetBrowser, { props: { presets: mockPresets } });

			expect(screen.getByTitle('Filter by category')).toBeInTheDocument();
		});

		it('shows all categories option', () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([
				['milkdrop', 10],
				['custom', 5]
			]);
			render(PresetBrowser, { props: { presets: mockPresets } });

			expect(screen.getByText('All Categories')).toBeInTheDocument();
		});

		it('shows category options with counts', () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([
				['milkdrop', 10],
				['custom', 5]
			]);
			render(PresetBrowser, { props: { presets: mockPresets } });

			expect(screen.getByText('milkdrop (10)')).toBeInTheDocument();
			expect(screen.getByText('custom (5)')).toBeInTheDocument();
		});

		it('calls buildCategoriesIndex when presets are provided', () => {
			render(PresetBrowser, { props: { presets: mockPresets } });

			expect(categoriesStore.buildCategoriesIndex).toHaveBeenCalledWith(mockPresets);
		});

		it('filters presets by selected category', async () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([
				['milkdrop', 2],
				['custom', 2]
			]);
			vi.mocked(categoriesStore.extractCategory).mockImplementation((path) => {
				if (path.includes('Acid') || path.includes('Blue')) return 'milkdrop';
				return 'custom';
			});

			render(PresetBrowser, { props: { presets: mockPresets } });

			const select = screen.getByTitle('Filter by category') as HTMLSelectElement;
			await fireEvent.change(select, { target: { value: 'milkdrop' } });

			// Should only show milkdrop presets
			expect(screen.getByText('Acid Trip')).toBeInTheDocument();
			expect(screen.getByText('Blue Wave')).toBeInTheDocument();
			expect(screen.queryByText('Cosmic Dance')).not.toBeInTheDocument();
			expect(screen.queryByText('Digital Dreams')).not.toBeInTheDocument();
		});

		it('disables category select when collapsed', async () => {
			vi.mocked(categoriesStore.getCategoriesWithCounts).mockReturnValue([
				['milkdrop', 10],
				['custom', 5]
			]);
			render(PresetBrowser, { props: { presets: mockPresets } });

			// Collapse the panel
			const toggleButton = screen.getByText('Presets').closest('button')!;
			await fireEvent.click(toggleButton);

			const select = screen.getByTitle('Filter by category') as HTMLSelectElement;
			expect(select.disabled).toBe(true);
		});
	});
});
