import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import DeckPanel from '$lib/components/DeckPanel.svelte';

// Default props with noop handlers
const defaultProps = {
	running: false,
	currentPreset: '',
	onStart: vi.fn(),
	onStop: vi.fn(),
	onFullscreen: vi.fn()
};

describe('DeckPanel', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('rendering', () => {
		it('renders placeholder when not running', () => {
			render(DeckPanel, { props: { ...defaultProps, running: false } });

			expect(screen.getByText('Click Start to begin visualization')).toBeInTheDocument();
		});

		it('renders LIVE indicator when running', () => {
			render(DeckPanel, { props: { ...defaultProps, running: true } });

			expect(screen.getByText('LIVE')).toBeInTheDocument();
			expect(screen.queryByText('Click Start to begin visualization')).not.toBeInTheDocument();
		});

		it('shows "No preset loaded" when no preset is set', () => {
			render(DeckPanel, { props: { ...defaultProps, running: false, currentPreset: '' } });

			expect(screen.getByText('No preset loaded')).toBeInTheDocument();
		});
	});

	describe('preset name display', () => {
		it('displays preset name from path', () => {
			render(DeckPanel, {
				props: {
					...defaultProps,
					running: false,
					currentPreset: '/path/to/presets/Cool Visualization.milk'
				}
			});

			expect(screen.getByText('Cool Visualization')).toBeInTheDocument();
		});

		it('handles preset path without .milk extension', () => {
			render(DeckPanel, {
				props: {
					...defaultProps,
					running: false,
					currentPreset: '/path/to/presets/TestPreset'
				}
			});

			expect(screen.getByText('TestPreset')).toBeInTheDocument();
		});

		it('shows full path in title attribute', () => {
			const presetPath = '/path/to/presets/Cool Visualization.milk';
			render(DeckPanel, {
				props: {
					...defaultProps,
					running: false,
					currentPreset: presetPath
				}
			});

			const presetNameElement = screen.getByText('Cool Visualization');
			expect(presetNameElement).toHaveAttribute('title', presetPath);
		});
	});

	describe('start/stop controls', () => {
		it('shows Start button when not running', () => {
			render(DeckPanel, { props: { ...defaultProps, running: false } });

			expect(screen.getByText('Start')).toBeInTheDocument();
			expect(screen.queryByText('Stop')).not.toBeInTheDocument();
		});

		it('shows Stop button when running', () => {
			render(DeckPanel, { props: { ...defaultProps, running: true } });

			expect(screen.getByText('Stop')).toBeInTheDocument();
			expect(screen.queryByText('Start')).not.toBeInTheDocument();
		});

		it('calls onStart when Start button is clicked', async () => {
			const onStart = vi.fn();
			render(DeckPanel, { props: { ...defaultProps, running: false, onStart } });

			const startButton = screen.getByText('Start').closest('button')!;
			await fireEvent.click(startButton);

			expect(onStart).toHaveBeenCalledTimes(1);
		});

		it('calls onStop when Stop button is clicked', async () => {
			const onStop = vi.fn();
			render(DeckPanel, { props: { ...defaultProps, running: true, onStop } });

			const stopButton = screen.getByText('Stop').closest('button')!;
			await fireEvent.click(stopButton);

			expect(onStop).toHaveBeenCalledTimes(1);
		});
	});

	describe('fullscreen button', () => {
		it('shows fullscreen button only when running', () => {
			render(DeckPanel, { props: { ...defaultProps, running: true } });

			const fullscreenButton = screen.getByTitle('Fullscreen (F11)');
			expect(fullscreenButton).toBeInTheDocument();
		});

		it('hides fullscreen button when not running', () => {
			render(DeckPanel, { props: { ...defaultProps, running: false } });

			expect(screen.queryByTitle('Fullscreen (F11)')).not.toBeInTheDocument();
		});

		it('calls onFullscreen when fullscreen button is clicked', async () => {
			const onFullscreen = vi.fn();
			render(DeckPanel, { props: { ...defaultProps, running: true, onFullscreen } });

			const fullscreenButton = screen.getByTitle('Fullscreen (F11)');
			await fireEvent.click(fullscreenButton);

			expect(onFullscreen).toHaveBeenCalledTimes(1);
		});
	});

	describe('status indicator', () => {
		it('shows inactive status when not running', () => {
			const { container } = render(DeckPanel, { props: { ...defaultProps, running: false } });

			const statusDot = container.querySelector('.status');
			expect(statusDot).not.toHaveClass('active');
		});

		it('shows active status when running', () => {
			const { container } = render(DeckPanel, { props: { ...defaultProps, running: true } });

			const statusDot = container.querySelector('.status.active');
			expect(statusDot).toBeInTheDocument();
		});
	});

	describe('preview area styling', () => {
		it('adds active class to preview area when running', () => {
			const { container } = render(DeckPanel, { props: { ...defaultProps, running: true } });

			const previewArea = container.querySelector('.preview-area.active');
			expect(previewArea).toBeInTheDocument();
		});

		it('does not have active class when not running', () => {
			const { container } = render(DeckPanel, { props: { ...defaultProps, running: false } });

			const previewArea = container.querySelector('.preview-area');
			expect(previewArea).not.toHaveClass('active');
		});
	});
});
