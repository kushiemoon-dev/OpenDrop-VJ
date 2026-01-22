import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import VideoOutputPanel from '$lib/components/VideoOutputPanel.svelte';

// Get mocked invoke
const mockInvoke = vi.mocked(invoke);

describe('VideoOutputPanel', () => {
	const mockDevices = ['/dev/video10:OpenDrop', '/dev/video11:Test'];
	const mockMonitors = [
		{ index: 0, name: 'Primary', width: 1920, height: 1080, is_primary: true },
		{ index: 1, name: 'Secondary', width: 1280, height: 720, is_primary: false }
	];

	beforeEach(() => {
		vi.clearAllMocks();
		// Default mocks
		mockInvoke.mockImplementation(async (cmd) => {
			if (cmd === 'list_video_outputs') return mockDevices;
			if (cmd === 'list_monitors') return mockMonitors;
			if (cmd === 'is_ndi_available') return true;
			if (cmd === 'set_deck_video_output') return null;
			if (cmd === 'set_deck_ndi_output') return null;
			return null;
		});
	});

	describe('rendering', () => {
		it('renders the video panel header', async () => {
			render(VideoOutputPanel);
			expect(screen.getByText('Video Output')).toBeInTheDocument();
		});

		it('shows deck indicator', async () => {
			render(VideoOutputPanel, { props: { deckId: 0 } });
			await waitFor(() => {
				expect(screen.getByText('Deck 1')).toBeInTheDocument();
			});
		});

		it('shows correct deck number for different deckId', async () => {
			render(VideoOutputPanel, { props: { deckId: 2 } });
			await waitFor(() => {
				expect(screen.getByText('Deck 3')).toBeInTheDocument();
			});
		});

		it('shows NDI section header', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText('NDI Network')).toBeInTheDocument();
			});
		});
	});

	describe('device loading', () => {
		it('calls list_video_outputs on mount', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('list_video_outputs');
			});
		});

		it('calls list_monitors on mount', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('list_monitors');
			});
		});

		it('displays devices in select dropdown', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText(/OpenDrop.*video10/)).toBeInTheDocument();
			});
		});

		it('shows no devices message when empty', async () => {
			mockInvoke.mockImplementation(async (cmd) => {
				if (cmd === 'list_video_outputs') return [];
				if (cmd === 'list_monitors') return [];
				if (cmd === 'is_ndi_available') return false;
				return null;
			});
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText('No v4l2 devices found')).toBeInTheDocument();
			});
		});

		it('shows help command for v4l2loopback', async () => {
			mockInvoke.mockImplementation(async (cmd) => {
				if (cmd === 'list_video_outputs') return [];
				if (cmd === 'list_monitors') return [];
				if (cmd === 'is_ndi_available') return false;
				return null;
			});
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText(/sudo modprobe v4l2loopback/)).toBeInTheDocument();
			});
		});
	});

	describe('video output toggle', () => {
		it('shows Enable Output button when not enabled', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable output/i })).toBeInTheDocument();
			});
		});

		it('calls set_deck_video_output when enabling', async () => {
			render(VideoOutputPanel, { props: { deckId: 1 } });
			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable output/i })).toBeInTheDocument();
			});

			const enableBtn = screen.getByRole('button', { name: /enable output/i });
			await fireEvent.click(enableBtn);

			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('set_deck_video_output', {
					deckId: 1,
					enabled: true,
					devicePath: '/dev/video10'
				});
			});
		});

		it('shows Disable button after enabling', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable output/i })).toBeInTheDocument();
			});

			const enableBtn = screen.getByRole('button', { name: /enable output/i });
			await fireEvent.click(enableBtn);

			await waitFor(() => {
				expect(screen.getByRole('button', { name: /disable/i })).toBeInTheDocument();
			});
		});
	});

	describe('monitor selection', () => {
		it('shows monitor select when multiple monitors', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByLabelText('Fullscreen Monitor')).toBeInTheDocument();
			});
		});

		it('does not show monitor select for single monitor', async () => {
			mockInvoke.mockImplementation(async (cmd) => {
				if (cmd === 'list_video_outputs') return mockDevices;
				if (cmd === 'list_monitors') return [mockMonitors[0]];
				if (cmd === 'is_ndi_available') return true;
				return null;
			});
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.queryByLabelText('Fullscreen Monitor')).not.toBeInTheDocument();
			});
		});

		it('shows primary monitor indicator', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText(/Primary.*1920x1080.*\[Primary\]/)).toBeInTheDocument();
			});
		});
	});

	describe('NDI output', () => {
		it('checks NDI availability on mount', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('is_ndi_available');
			});
		});

		it('shows Enable NDI button when NDI is available', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable ndi/i })).toBeInTheDocument();
			});
		});

		it('shows NDI unavailable message when not available', async () => {
			mockInvoke.mockImplementation(async (cmd) => {
				if (cmd === 'list_video_outputs') return mockDevices;
				if (cmd === 'list_monitors') return mockMonitors;
				if (cmd === 'is_ndi_available') return false;
				return null;
			});
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText('NDI runtime not installed')).toBeInTheDocument();
			});
		});

		it('shows link to NDI tools when unavailable', async () => {
			mockInvoke.mockImplementation(async (cmd) => {
				if (cmd === 'list_video_outputs') return mockDevices;
				if (cmd === 'list_monitors') return mockMonitors;
				if (cmd === 'is_ndi_available') return false;
				return null;
			});
			render(VideoOutputPanel);
			await waitFor(() => {
				const link = screen.getByRole('link', { name: /get ndi tools/i });
				expect(link).toHaveAttribute('href', 'https://ndi.video/tools/');
			});
		});

		it('calls set_deck_ndi_output when enabling NDI', async () => {
			render(VideoOutputPanel, { props: { deckId: 0 } });
			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable ndi/i })).toBeInTheDocument();
			});

			const enableBtn = screen.getByRole('button', { name: /enable ndi/i });
			await fireEvent.click(enableBtn);

			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('set_deck_ndi_output', {
					deckId: 0,
					enabled: true,
					name: null
				});
			});
		});

		it('shows NDI source name input', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByPlaceholderText(/source name/i)).toBeInTheDocument();
			});
		});

		it('uses custom NDI name when provided', async () => {
			render(VideoOutputPanel, { props: { deckId: 0 } });
			await waitFor(() => {
				expect(screen.getByPlaceholderText(/source name/i)).toBeInTheDocument();
			});

			const input = screen.getByPlaceholderText(/source name/i);
			await fireEvent.input(input, { target: { value: 'Custom Source' } });

			const enableBtn = screen.getByRole('button', { name: /enable ndi/i });
			await fireEvent.click(enableBtn);

			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('set_deck_ndi_output', {
					deckId: 0,
					enabled: true,
					name: 'Custom Source'
				});
			});
		});
	});

	describe('refresh', () => {
		it('shows refresh button', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByRole('button', { name: /refresh devices/i })).toBeInTheDocument();
			});
		});

		it('calls list_video_outputs when refresh is clicked', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('list_video_outputs');
			});

			// Clear previous calls
			mockInvoke.mockClear();

			const refreshBtn = screen.getByRole('button', { name: /refresh devices/i });
			await fireEvent.click(refreshBtn);

			await waitFor(() => {
				expect(mockInvoke).toHaveBeenCalledWith('list_video_outputs');
			});
		});
	});

	describe('error handling', () => {
		it('shows error message when device list fails', async () => {
			mockInvoke.mockImplementation(async (cmd) => {
				if (cmd === 'list_video_outputs') throw new Error('Failed to list devices');
				if (cmd === 'list_monitors') return [];
				if (cmd === 'is_ndi_available') return false;
				return null;
			});
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText(/failed to list devices/i)).toBeInTheDocument();
			});
		});
	});

	describe('status callback', () => {
		it('calls onStatusChange when video output is toggled', async () => {
			const onStatusChange = vi.fn();
			render(VideoOutputPanel, { props: { deckId: 0, onStatusChange } });

			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable output/i })).toBeInTheDocument();
			});

			const enableBtn = screen.getByRole('button', { name: /enable output/i });
			await fireEvent.click(enableBtn);

			await waitFor(() => {
				expect(onStatusChange).toHaveBeenCalled();
			});
		});

		it('calls onStatusChange when NDI output is toggled', async () => {
			const onStatusChange = vi.fn();
			render(VideoOutputPanel, { props: { deckId: 0, onStatusChange } });

			await waitFor(() => {
				expect(screen.getByRole('button', { name: /enable ndi/i })).toBeInTheDocument();
			});

			const enableBtn = screen.getByRole('button', { name: /enable ndi/i });
			await fireEvent.click(enableBtn);

			await waitFor(() => {
				expect(onStatusChange).toHaveBeenCalled();
			});
		});
	});

	describe('help text', () => {
		it('shows v4l2 help text', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText(/output to obs, vlc/i)).toBeInTheDocument();
			});
		});

		it('shows NDI help text', async () => {
			render(VideoOutputPanel);
			await waitFor(() => {
				expect(screen.getByText(/stream to ndi-compatible apps/i)).toBeInTheDocument();
			});
		});
	});
});
