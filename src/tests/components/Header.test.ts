import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/svelte';
import Header from '$lib/components/Header.svelte';
import { toggleTheme } from '$lib/stores/theme';
import { setAccent, ACCENT_PRESETS } from '$lib/stores/accent';

describe('Header', () => {
	beforeEach(() => {
		vi.clearAllMocks();
	});

	describe('rendering', () => {
		it('renders the logo', () => {
			render(Header);
			expect(screen.getByText('OpenDrop')).toBeInTheDocument();
		});

		it('renders the version', () => {
			render(Header);
			expect(screen.getByText('v0.1.0')).toBeInTheDocument();
		});

		it('renders projectm version when provided', () => {
			render(Header, { props: { version: '4.1.0' } });
			expect(screen.getByText('ProjectM 4.1.0')).toBeInTheDocument();
		});

		it('does not render projectm version when not provided', () => {
			render(Header);
			expect(screen.queryByText(/ProjectM/)).not.toBeInTheDocument();
		});

		it('renders status indicators', () => {
			render(Header, { props: { audioRunning: true, visualizerRunning: false } });
			expect(screen.getByText('Audio')).toBeInTheDocument();
			expect(screen.getByText('Visu')).toBeInTheDocument();
		});

		it('renders theme toggle button', () => {
			render(Header);
			expect(screen.getByRole('button', { name: /switch to/i })).toBeInTheDocument();
		});

		it('renders accent color toggle button', () => {
			render(Header);
			expect(screen.getByRole('button', { name: /change accent color/i })).toBeInTheDocument();
		});
	});

	describe('theme toggle', () => {
		it('calls toggleTheme when theme button is clicked', async () => {
			render(Header);
			const themeButton = screen.getByRole('button', { name: /switch to/i });
			await fireEvent.click(themeButton);
			expect(toggleTheme).toHaveBeenCalled();
		});

		it('shows sun icon in dark mode', () => {
			render(Header);
			// In dark mode, we show the sun icon to switch to light
			const themeButton = screen.getByRole('button', { name: /switch to light mode/i });
			expect(themeButton).toBeInTheDocument();
		});
	});

	describe('accent color picker', () => {
		it('opens accent picker when accent button is clicked', async () => {
			render(Header);
			const accentButton = screen.getByRole('button', { name: /change accent color/i });

			// Picker should not be visible initially
			expect(screen.queryByText('Accent Color')).not.toBeInTheDocument();

			await fireEvent.click(accentButton);

			// Picker should now be visible
			expect(screen.getByText('Accent Color')).toBeInTheDocument();
		});

		it('shows all accent color options when opened', async () => {
			render(Header);
			const accentButton = screen.getByRole('button', { name: /change accent color/i });
			await fireEvent.click(accentButton);

			// Check all presets are shown
			for (const preset of ACCENT_PRESETS) {
				expect(screen.getByText(preset.name)).toBeInTheDocument();
			}
		});

		it('calls setAccent when a color is selected', async () => {
			render(Header);
			const accentButton = screen.getByRole('button', { name: /change accent color/i });
			await fireEvent.click(accentButton);

			const magentaButton = screen.getByRole('menuitem', { name: /magenta/i });
			await fireEvent.click(magentaButton);

			expect(setAccent).toHaveBeenCalledWith('magenta');
		});

		it('closes picker when color is selected', async () => {
			render(Header);
			const accentButton = screen.getByRole('button', { name: /change accent color/i });
			await fireEvent.click(accentButton);

			expect(screen.getByText('Accent Color')).toBeInTheDocument();

			const cyanButton = screen.getByRole('menuitem', { name: /cyan/i });
			await fireEvent.click(cyanButton);

			expect(screen.queryByText('Accent Color')).not.toBeInTheDocument();
		});

		it('toggles picker open/closed on button clicks', async () => {
			render(Header);
			const accentButton = screen.getByRole('button', { name: /change accent color/i });

			// Open
			await fireEvent.click(accentButton);
			expect(screen.getByText('Accent Color')).toBeInTheDocument();

			// Close
			await fireEvent.click(accentButton);
			expect(screen.queryByText('Accent Color')).not.toBeInTheDocument();
		});

		it('has correct aria-expanded state', async () => {
			render(Header);
			const accentButton = screen.getByRole('button', { name: /change accent color/i });

			expect(accentButton).toHaveAttribute('aria-expanded', 'false');

			await fireEvent.click(accentButton);
			expect(accentButton).toHaveAttribute('aria-expanded', 'true');
		});
	});

	describe('props', () => {
		it('accepts version prop', () => {
			render(Header, { props: { version: '4.2.0' } });
			expect(screen.getByText('ProjectM 4.2.0')).toBeInTheDocument();
		});

		it('accepts audioRunning prop', () => {
			const { container } = render(Header, { props: { audioRunning: true } });
			// StatusIndicator uses .status class
			const statusElements = container.querySelectorAll('.status');
			expect(statusElements.length).toBeGreaterThan(0);
		});

		it('accepts visualizerRunning prop', () => {
			const { container } = render(Header, { props: { visualizerRunning: true } });
			// StatusIndicator uses .status class
			const statusElements = container.querySelectorAll('.status');
			expect(statusElements.length).toBeGreaterThan(0);
		});

		it('uses default values when no props provided', () => {
			render(Header);
			expect(screen.queryByText(/ProjectM/)).not.toBeInTheDocument();
		});
	});
});
