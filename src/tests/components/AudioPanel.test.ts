import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import AudioPanel from '$lib/components/AudioPanel.svelte';

describe('AudioPanel', () => {
	const mockDevices = [
		{ name: 'Built-in Microphone', is_default: true },
		{ name: 'USB Audio Device', is_default: false },
		{ name: 'External Interface Long Name That Should Be Truncated', is_default: false }
	];

	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('rendering', () => {
		it('renders the panel header', () => {
			render(AudioPanel);
			expect(screen.getByText('Audio Input')).toBeInTheDocument();
		});

		it('renders device options in select', () => {
			render(AudioPanel, { props: { devices: mockDevices } });

			const select = screen.getByRole('combobox');
			expect(select).toBeInTheDocument();

			const options = screen.getAllByRole('option');
			expect(options).toHaveLength(3);
		});

		it('shows (Default) label for default device', () => {
			render(AudioPanel, { props: { devices: mockDevices } });

			expect(screen.getByText(/Built-in Microphone.*\(Default\)/)).toBeInTheDocument();
		});

		it('truncates long device names', () => {
			render(AudioPanel, { props: { devices: mockDevices } });

			expect(screen.getByText(/External Interface Long Name/)).toBeInTheDocument();
			expect(screen.getByText(/\.\.\./)).toBeInTheDocument();
		});

		it('renders VuMeter components for L and R channels', () => {
			render(AudioPanel);

			expect(screen.getByText('L')).toBeInTheDocument();
			expect(screen.getByText('R')).toBeInTheDocument();
		});
	});

	describe('device selection', () => {
		it('allows selecting a device when not running', async () => {
			render(AudioPanel, {
				props: {
					devices: mockDevices,
					selectedDevice: 'Built-in Microphone',
					running: false
				}
			});

			const select = screen.getByRole('combobox') as HTMLSelectElement;
			expect(select.disabled).toBe(false);
		});

		it('disables device selection when running', () => {
			render(AudioPanel, {
				props: {
					devices: mockDevices,
					selectedDevice: 'Built-in Microphone',
					running: true
				}
			});

			const select = screen.getByRole('combobox') as HTMLSelectElement;
			expect(select.disabled).toBe(true);
		});
	});

	describe('start/stop controls', () => {
		it('shows Start button when not running', () => {
			render(AudioPanel, { props: { running: false } });

			expect(screen.getByText('Start')).toBeInTheDocument();
			expect(screen.queryByText('Stop')).not.toBeInTheDocument();
		});

		it('shows Stop button when running', () => {
			render(AudioPanel, { props: { running: true } });

			expect(screen.getByText('Stop')).toBeInTheDocument();
			expect(screen.queryByText('Start')).not.toBeInTheDocument();
		});

		it('calls onStart when Start button is clicked', async () => {
			const onStart = vi.fn();
			render(AudioPanel, { props: { running: false, onStart } });

			const startButton = screen.getByText('Start').closest('button')!;
			await fireEvent.click(startButton);

			expect(onStart).toHaveBeenCalledTimes(1);
		});

		it('calls onStop when Stop button is clicked', async () => {
			const onStop = vi.fn();
			render(AudioPanel, { props: { running: true, onStop } });

			const stopButton = screen.getByText('Stop').closest('button')!;
			await fireEvent.click(stopButton);

			expect(onStop).toHaveBeenCalledTimes(1);
		});
	});

	describe('refresh button', () => {
		it('renders refresh button', () => {
			render(AudioPanel);

			const refreshButton = screen.getByTitle('Refresh devices');
			expect(refreshButton).toBeInTheDocument();
		});

		it('calls onRefresh when refresh button is clicked', async () => {
			const onRefresh = vi.fn();
			render(AudioPanel, { props: { onRefresh } });

			const refreshButton = screen.getByTitle('Refresh devices');
			await fireEvent.click(refreshButton);

			expect(onRefresh).toHaveBeenCalledTimes(1);
		});
	});

	describe('status indicator', () => {
		it('shows inactive status when not running', () => {
			const { container } = render(AudioPanel, { props: { running: false } });

			const statusDot = container.querySelector('.status');
			expect(statusDot).not.toHaveClass('active');
		});

		it('shows active status when running', () => {
			const { container } = render(AudioPanel, { props: { running: true } });

			const statusDot = container.querySelector('.status.active');
			expect(statusDot).toBeInTheDocument();
		});
	});
});
