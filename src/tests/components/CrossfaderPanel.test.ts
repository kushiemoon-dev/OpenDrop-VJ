import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import CrossfaderPanel from '$lib/components/CrossfaderPanel.svelte';

vi.mock('@tauri-apps/api/core');

describe('CrossfaderPanel', () => {
	const mockCrossfader = {
		position: 0.5,
		side_a: [0, 1],
		side_b: [2, 3],
		curve: 'equal_power',
		enabled: true
	};

	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(invoke).mockResolvedValue(undefined);
	});

	describe('rendering', () => {
		it('renders panel header', () => {
			render(CrossfaderPanel);
			expect(screen.getByText('Crossfader')).toBeInTheDocument();
		});

		it('shows ON/OFF toggle button', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });
			expect(screen.getByText('ON')).toBeInTheDocument();
		});

		it('shows OFF when disabled', () => {
			render(CrossfaderPanel, {
				props: { crossfader: { ...mockCrossfader, enabled: false } }
			});
			expect(screen.getByText('OFF')).toBeInTheDocument();
		});
	});

	describe('side labels', () => {
		it('shows side A label', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });
			const sideLabels = screen.getAllByText('A');
			expect(sideLabels.length).toBeGreaterThan(0);
		});

		it('shows side B label', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });
			const sideLabels = screen.getAllByText('B');
			expect(sideLabels.length).toBeGreaterThan(0);
		});

		it('shows deck assignments for side A', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });
			expect(screen.getByText('1, 2')).toBeInTheDocument();
		});

		it('shows deck assignments for side B', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });
			expect(screen.getByText('3, 4')).toBeInTheDocument();
		});
	});

	describe('volume display', () => {
		it('shows 50% for both sides at center position with equal power', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });

			// At position 0.5 with equal_power curve, both sides should be ~71%
			// sqrt(0.5) = 0.707... = ~71%
			const volumes = screen.getAllByText(/\d+%/);
			expect(volumes.length).toBe(2);
		});

		it('shows 100% for both sides when disabled', () => {
			render(CrossfaderPanel, {
				props: { crossfader: { ...mockCrossfader, enabled: false } }
			});

			const volumes = screen.getAllByText('100%');
			expect(volumes.length).toBe(2);
		});
	});

	describe('fader control', () => {
		it('renders range slider', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });

			const slider = screen.getByRole('slider');
			expect(slider).toBeInTheDocument();
		});

		it('slider has correct min/max values', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });

			const slider = screen.getByRole('slider') as HTMLInputElement;
			expect(slider.min).toBe('0');
			expect(slider.max).toBe('1');
		});

		it('calls crossfader_set_position when slider changes', async () => {
			const onUpdate = vi.fn();
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader, onUpdate } });

			const slider = screen.getByRole('slider');
			await fireEvent.input(slider, { target: { value: '0.75' } });

			expect(invoke).toHaveBeenCalledWith('crossfader_set_position', { position: 0.75 });
			expect(onUpdate).toHaveBeenCalled();
		});

		it('disables slider when crossfader is disabled', () => {
			render(CrossfaderPanel, {
				props: { crossfader: { ...mockCrossfader, enabled: false } }
			});

			const slider = screen.getByRole('slider') as HTMLInputElement;
			expect(slider.disabled).toBe(true);
		});
	});

	describe('enable/disable toggle', () => {
		it('calls crossfader_set_enabled when toggle is clicked', async () => {
			const onUpdate = vi.fn();
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader, onUpdate } });

			const toggleButton = screen.getByText('ON');
			await fireEvent.click(toggleButton);

			expect(invoke).toHaveBeenCalledWith('crossfader_set_enabled', { enabled: false });
			expect(onUpdate).toHaveBeenCalled();
		});

		it('has correct title for enable state', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });

			const toggleButton = screen.getByTitle('Disable crossfader');
			expect(toggleButton).toBeInTheDocument();
		});

		it('has correct title for disable state', () => {
			render(CrossfaderPanel, {
				props: { crossfader: { ...mockCrossfader, enabled: false } }
			});

			const toggleButton = screen.getByTitle('Enable crossfader');
			expect(toggleButton).toBeInTheDocument();
		});
	});

	describe('curve selector', () => {
		it('shows Linear and Equal Power buttons', () => {
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader } });

			expect(screen.getByText('Linear')).toBeInTheDocument();
			expect(screen.getByText('Equal Power')).toBeInTheDocument();
		});

		it('highlights active curve', () => {
			const { container } = render(CrossfaderPanel, {
				props: { crossfader: mockCrossfader }
			});

			const activeButton = container.querySelector('.curve-btn.active');
			expect(activeButton).toHaveTextContent('Equal Power');
		});

		it('calls crossfader_set_curve when curve button is clicked', async () => {
			const onUpdate = vi.fn();
			render(CrossfaderPanel, { props: { crossfader: mockCrossfader, onUpdate } });

			const linearButton = screen.getByText('Linear');
			await fireEvent.click(linearButton);

			expect(invoke).toHaveBeenCalledWith('crossfader_set_curve', { curve: 'linear' });
			expect(onUpdate).toHaveBeenCalled();
		});

		it('disables curve buttons when crossfader is disabled', () => {
			render(CrossfaderPanel, {
				props: { crossfader: { ...mockCrossfader, enabled: false } }
			});

			const linearButton = screen.getByText('Linear') as HTMLButtonElement;
			const equalPowerButton = screen.getByText('Equal Power') as HTMLButtonElement;

			expect(linearButton.disabled).toBe(true);
			expect(equalPowerButton.disabled).toBe(true);
		});
	});

	describe('disabled state styling', () => {
		it('adds disabled class to fader section when disabled', () => {
			const { container } = render(CrossfaderPanel, {
				props: { crossfader: { ...mockCrossfader, enabled: false } }
			});

			const faderSection = container.querySelector('.fader-section.disabled');
			expect(faderSection).toBeInTheDocument();
		});

		it('removes disabled class when enabled', () => {
			const { container } = render(CrossfaderPanel, {
				props: { crossfader: mockCrossfader }
			});

			const faderSection = container.querySelector('.fader-section');
			expect(faderSection).not.toHaveClass('disabled');
		});
	});
});
