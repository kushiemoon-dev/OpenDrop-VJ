import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/svelte';
import { invoke } from '@tauri-apps/api/core';
import MidiPanel from '$lib/components/MidiPanel.svelte';

vi.mock('@tauri-apps/api/core');

describe('MidiPanel', () => {
	const mockPorts = [
		{ index: 0, name: 'Akai APC Mini' },
		{ index: 1, name: 'Novation Launchpad' }
	];

	const mockStatus = {
		connected: false,
		learning: false,
		port_name: null,
		mapping_count: 0
	};

	const mockMappings = [
		{ id: '1', name: 'Volume 1', midi_type: 'CC', action: 'deck_volume', enabled: true },
		{ id: '2', name: 'Play 1', midi_type: 'Note', action: 'deck_toggle', enabled: true }
	];

	const mockBuiltinPresets = [
		{ name: 'Generic 4 Decks', description: 'Generic controller mapping', controller: 'Generic', mapping_count: 20 },
		{ name: 'Akai APC Mini', description: 'Akai APC Mini mapping', controller: 'Akai APC Mini', mapping_count: 32 }
	];

	beforeEach(() => {
		vi.clearAllMocks();
		vi.mocked(invoke).mockImplementation(async (cmd) => {
			switch (cmd) {
				case 'list_midi_ports':
					return mockPorts;
				case 'midi_get_status':
					return mockStatus;
				case 'midi_get_mappings':
					return [];
				case 'midi_list_builtin_presets':
					return mockBuiltinPresets;
				default:
					return undefined;
			}
		});
	});

	describe('rendering', () => {
		it('renders panel header', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('MIDI Control')).toBeInTheDocument();
			});
		});

		it('shows status indicator', async () => {
			render(MidiPanel);
			await waitFor(() => {
				const { container } = render(MidiPanel);
				const statusIndicator = container.querySelector('.status');
				expect(statusIndicator).toBeInTheDocument();
			});
		});
	});

	describe('no devices state', () => {
		it('shows no devices message when no MIDI ports', async () => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return [];
				if (cmd === 'midi_get_status') return mockStatus;
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return [];
				return undefined;
			});

			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('No MIDI devices found')).toBeInTheDocument();
			});
		});
	});

	describe('port selection', () => {
		it('shows port dropdown when devices available', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByRole('combobox')).toBeInTheDocument();
			});
		});

		it('lists available ports', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Akai APC Mini')).toBeInTheDocument();
				expect(screen.getByText('Novation Launchpad')).toBeInTheDocument();
			});
		});

		it('shows refresh button', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByTitle('Refresh')).toBeInTheDocument();
			});
		});

		it('calls list_midi_ports when refresh is clicked', async () => {
			render(MidiPanel);
			await waitFor(() => screen.getByTitle('Refresh'));

			const refreshButton = screen.getByTitle('Refresh');
			await fireEvent.click(refreshButton);

			expect(invoke).toHaveBeenCalledWith('list_midi_ports');
		});
	});

	describe('connect/disconnect', () => {
		it('shows Connect button when not connected', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Connect')).toBeInTheDocument();
			});
		});

		it('calls midi_connect when Connect is clicked', async () => {
			render(MidiPanel);
			await waitFor(() => screen.getByText('Connect'));

			const connectButton = screen.getByText('Connect');
			await fireEvent.click(connectButton);

			expect(invoke).toHaveBeenCalledWith('midi_connect', { portIndex: 0 });
		});

		it('shows Disconnect button when connected', async () => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true };
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});

			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Disconnect')).toBeInTheDocument();
			});
		});

		it('disables port select when connected', async () => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true };
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});

			render(MidiPanel);
			await waitFor(() => {
				const select = screen.getByRole('combobox') as HTMLSelectElement;
				expect(select.disabled).toBe(true);
			});
		});
	});

	describe('connected state sections', () => {
		beforeEach(() => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true };
				if (cmd === 'midi_get_mappings') return mockMappings;
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});
		});

		it('shows Quick Presets section when connected', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Quick Presets')).toBeInTheDocument();
			});
		});

		it('shows Learn Mapping section when connected', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Learn Mapping')).toBeInTheDocument();
			});
		});

		it('shows Mappings section when connected', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText(/Mappings/)).toBeInTheDocument();
			});
		});
	});

	describe('learn mode', () => {
		beforeEach(() => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true };
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});
		});

		it('shows learn form with inputs', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByPlaceholderText('Mapping name')).toBeInTheDocument();
			});
		});

		it('shows action dropdown', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Select action...')).toBeInTheDocument();
			});
		});

		it('shows deck selector', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Deck 1')).toBeInTheDocument();
			});
		});

		it('disables Learn button when name or action is empty', async () => {
			render(MidiPanel);
			await waitFor(() => {
				const learnButton = screen.getByText('Learn') as HTMLButtonElement;
				expect(learnButton.disabled).toBe(true);
			});
		});

		it('shows learning state UI', async () => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true, learning: true };
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});

			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Move a MIDI control...')).toBeInTheDocument();
			});
		});
	});

	describe('mappings list', () => {
		beforeEach(() => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true };
				if (cmd === 'midi_get_mappings') return mockMappings;
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});
		});

		it('shows mapping count', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText(/Mappings \(2\)/)).toBeInTheDocument();
			});
		});

		it('lists mapping names', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Volume 1')).toBeInTheDocument();
				expect(screen.getByText('Play 1')).toBeInTheDocument();
			});
		});

		it('shows Clear All button when mappings exist', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Clear All')).toBeInTheDocument();
			});
		});

		it('shows no mappings message when empty', async () => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') return mockPorts;
				if (cmd === 'midi_get_status') return { ...mockStatus, connected: true };
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return mockBuiltinPresets;
				return undefined;
			});

			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText(/No mappings configured/)).toBeInTheDocument();
			});
		});
	});

	describe('help text', () => {
		it('shows help text', async () => {
			render(MidiPanel);
			await waitFor(() => {
				expect(screen.getByText('Map MIDI controls to deck functions')).toBeInTheDocument();
			});
		});
	});

	describe('error handling', () => {
		it('displays error message when error occurs', async () => {
			vi.mocked(invoke).mockImplementation(async (cmd) => {
				if (cmd === 'list_midi_ports') throw new Error('Connection failed');
				if (cmd === 'midi_get_status') return mockStatus;
				if (cmd === 'midi_get_mappings') return [];
				if (cmd === 'midi_list_builtin_presets') return [];
				return undefined;
			});

			render(MidiPanel);
			await waitFor(() => {
				// String(error) produces "Error: Connection failed"
				expect(screen.getByText(/Connection failed/)).toBeInTheDocument();
			});
		});
	});
});
