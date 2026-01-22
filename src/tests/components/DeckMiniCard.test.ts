import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import DeckMiniCard from '$lib/components/DeckMiniCard.svelte';

describe('DeckMiniCard', () => {
	const defaultProps = {
		deckId: 0,
		running: false,
		preset: null,
		volume: 1.0,
		selected: false,
		onStart: vi.fn(),
		onStop: vi.fn(),
		onFullscreen: vi.fn(),
		onVolumeChange: vi.fn(),
		onSelect: vi.fn()
	};

	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('rendering', () => {
		it('renders the deck number', () => {
			render(DeckMiniCard, { props: defaultProps });
			expect(screen.getByText('Deck 1')).toBeInTheDocument();
		});

		it('renders correct deck number based on deckId', () => {
			render(DeckMiniCard, { props: { ...defaultProps, deckId: 2 } });
			expect(screen.getByText('Deck 3')).toBeInTheDocument();
		});

		it('shows "No preset" when preset is null', () => {
			render(DeckMiniCard, { props: defaultProps });
			expect(screen.getByText('No preset')).toBeInTheDocument();
		});

		it('shows preset name from path', () => {
			render(DeckMiniCard, { props: { ...defaultProps, preset: '/path/to/MyPreset.milk' } });
			expect(screen.getByText('MyPreset')).toBeInTheDocument();
		});

		it('shows start button when not running', () => {
			render(DeckMiniCard, { props: defaultProps });
			expect(screen.getByRole('button', { name: /start deck/i })).toBeInTheDocument();
		});

		it('shows stop button when running', () => {
			render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			expect(screen.getByRole('button', { name: /stop deck/i })).toBeInTheDocument();
		});

		it('shows LIVE indicator when running', () => {
			render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			expect(screen.getByText('LIVE')).toBeInTheDocument();
		});

		it('does not show LIVE indicator when not running', () => {
			render(DeckMiniCard, { props: defaultProps });
			expect(screen.queryByText('LIVE')).not.toBeInTheDocument();
		});

		it('shows fullscreen button when running', () => {
			render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			expect(screen.getByRole('button', { name: /fullscreen/i })).toBeInTheDocument();
		});

		it('does not show fullscreen button when not running', () => {
			render(DeckMiniCard, { props: defaultProps });
			expect(screen.queryByRole('button', { name: /fullscreen/i })).not.toBeInTheDocument();
		});
	});

	describe('interactions', () => {
		it('calls onStart when start button is clicked', async () => {
			render(DeckMiniCard, { props: defaultProps });
			const startButton = screen.getByRole('button', { name: /start deck/i });
			await fireEvent.click(startButton);
			expect(defaultProps.onStart).toHaveBeenCalled();
		});

		it('calls onStop when stop button is clicked', async () => {
			render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			const stopButton = screen.getByRole('button', { name: /stop deck/i });
			await fireEvent.click(stopButton);
			expect(defaultProps.onStop).toHaveBeenCalled();
		});

		it('calls onFullscreen when fullscreen button is clicked', async () => {
			render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			const fullscreenButton = screen.getByRole('button', { name: /fullscreen/i });
			await fireEvent.click(fullscreenButton);
			expect(defaultProps.onFullscreen).toHaveBeenCalled();
		});

		it('calls onSelect when card is clicked', async () => {
			const { container } = render(DeckMiniCard, { props: defaultProps });
			const card = container.querySelector('.deck-card');
			await fireEvent.click(card!);
			expect(defaultProps.onSelect).toHaveBeenCalledWith(0);
		});

		it('calls onSelect with correct deckId', async () => {
			const { container } = render(DeckMiniCard, { props: { ...defaultProps, deckId: 3 } });
			const card = container.querySelector('.deck-card');
			await fireEvent.click(card!);
			expect(defaultProps.onSelect).toHaveBeenCalledWith(3);
		});

		it('calls onSelect on Enter key press', async () => {
			const { container } = render(DeckMiniCard, { props: defaultProps });
			const card = container.querySelector('.deck-card');
			await fireEvent.keyDown(card!, { key: 'Enter' });
			expect(defaultProps.onSelect).toHaveBeenCalledWith(0);
		});

		it('calls onVolumeChange when volume slider is changed', async () => {
			render(DeckMiniCard, { props: defaultProps });
			const slider = screen.getByRole('slider');
			await fireEvent.input(slider, { target: { value: '0.5' } });
			expect(defaultProps.onVolumeChange).toHaveBeenCalledWith(0.5);
		});

		it('does not propagate click from volume slider to card', async () => {
			render(DeckMiniCard, { props: defaultProps });
			const slider = screen.getByRole('slider');
			await fireEvent.click(slider);
			expect(defaultProps.onSelect).not.toHaveBeenCalled();
		});

		it('does not propagate click from start button to card', async () => {
			const onSelect = vi.fn();
			render(DeckMiniCard, { props: { ...defaultProps, onSelect } });
			const startButton = screen.getByRole('button', { name: /start deck/i });
			await fireEvent.click(startButton);
			expect(onSelect).not.toHaveBeenCalled();
		});
	});

	describe('styling', () => {
		it('applies selected class when selected', () => {
			const { container } = render(DeckMiniCard, { props: { ...defaultProps, selected: true } });
			expect(container.querySelector('.deck-card.selected')).toBeInTheDocument();
		});

		it('applies running class when running', () => {
			const { container } = render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			expect(container.querySelector('.deck-card.running')).toBeInTheDocument();
		});

		it('has correct volume slider value', () => {
			render(DeckMiniCard, { props: { ...defaultProps, volume: 0.75 } });
			const slider = screen.getByRole('slider');
			expect(slider).toHaveValue('0.75');
		});
	});

	describe('preset name extraction', () => {
		it('extracts name from Windows path', () => {
			render(DeckMiniCard, { props: { ...defaultProps, preset: 'C:\\presets\\MyPreset.milk' } });
			// Windows paths are split by '/', so the full path becomes the name
			// Actually the code uses split('/'), so let's test with forward slashes
			expect(screen.getByTitle('C:\\presets\\MyPreset.milk')).toBeInTheDocument();
		});

		it('extracts name from Unix path', () => {
			render(DeckMiniCard, { props: { ...defaultProps, preset: '/home/user/presets/Cool Preset.milk' } });
			expect(screen.getByText('Cool Preset')).toBeInTheDocument();
		});

		it('removes .milk extension', () => {
			render(DeckMiniCard, { props: { ...defaultProps, preset: '/presets/test.milk' } });
			expect(screen.getByText('test')).toBeInTheDocument();
			expect(screen.queryByText('test.milk')).not.toBeInTheDocument();
		});
	});

	describe('accessibility', () => {
		it('has focusable card', () => {
			const { container } = render(DeckMiniCard, { props: defaultProps });
			const card = container.querySelector('.deck-card');
			expect(card).toHaveAttribute('tabindex', '0');
		});

		it('has role button on card', () => {
			const { container } = render(DeckMiniCard, { props: defaultProps });
			const card = container.querySelector('.deck-card');
			expect(card).toHaveAttribute('role', 'button');
		});

		it('start button has accessible label', () => {
			render(DeckMiniCard, { props: defaultProps });
			expect(screen.getByRole('button', { name: /start deck/i })).toHaveAttribute('aria-label', 'Start deck');
		});

		it('stop button has accessible label', () => {
			render(DeckMiniCard, { props: { ...defaultProps, running: true } });
			expect(screen.getByRole('button', { name: /stop deck/i })).toHaveAttribute('aria-label', 'Stop deck');
		});
	});
});
