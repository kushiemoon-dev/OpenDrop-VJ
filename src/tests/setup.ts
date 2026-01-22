import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// Mock Tauri APIs
vi.mock('@tauri-apps/api/core', () => ({
	invoke: vi.fn()
}));

vi.mock('@tauri-apps/api/event', () => ({
	listen: vi.fn(() => Promise.resolve(() => {})),
	emit: vi.fn()
}));

// Mock toast store
vi.mock('$lib/stores/toast', () => ({
	showToast: vi.fn(),
	hideToast: vi.fn(),
	getToast: vi.fn(() => ({ message: '', type: 'info', visible: false })),
	toast: { message: '', type: 'info', visible: false }
}));

// Mock favorites store with in-memory storage for testing
const mockFavorites = new Set<string>();
vi.mock('$lib/stores/favorites', () => ({
	addFavorite: vi.fn((path: string) => mockFavorites.add(path)),
	removeFavorite: vi.fn((path: string) => mockFavorites.delete(path)),
	toggleFavorite: vi.fn((path: string) => {
		if (mockFavorites.has(path)) {
			mockFavorites.delete(path);
			return false;
		} else {
			mockFavorites.add(path);
			return true;
		}
	}),
	isFavorite: vi.fn((path: string) => mockFavorites.has(path)),
	getFavoritePaths: vi.fn(() => [...mockFavorites]),
	getFavoriteCount: vi.fn(() => mockFavorites.size),
	clearFavorites: vi.fn(() => mockFavorites.clear()),
	getFavorites: vi.fn(() => ({ paths: mockFavorites })),
	favorites: { paths: mockFavorites }
}));

// Mock categories store
vi.mock('$lib/stores/categories', () => ({
	extractCategory: vi.fn((path: string) => {
		if (!path) return 'Uncategorized';
		const segments = path.replace(/\\/g, '/').split('/').filter(Boolean);
		const presetIndex = segments.findIndex((s) => s.endsWith('.milk') || s.endsWith('.prjm'));
		if (presetIndex >= 1) return segments[presetIndex - 1];
		return 'Uncategorized';
	}),
	extractCategoryDetails: vi.fn((path: string) => ({
		category: 'Uncategorized',
		subcategory: null
	})),
	buildCategoriesIndex: vi.fn(),
	getAllCategories: vi.fn(() => []),
	getCategoryCount: vi.fn(() => 0),
	getTotalCategories: vi.fn(() => 0),
	getCategoriesWithCounts: vi.fn(() => []),
	getCategories: vi.fn(() => ({ categories: new Map(), basePath: '' })),
	categories: { categories: new Map(), basePath: '' }
}));

// Mock theme store
let mockTheme = 'dark';
vi.mock('$lib/stores/theme', () => ({
	theme: { current: 'dark' },
	toggleTheme: vi.fn(() => {
		mockTheme = mockTheme === 'dark' ? 'light' : 'dark';
	}),
	setTheme: vi.fn((t: string) => {
		mockTheme = t;
	}),
	getTheme: vi.fn(() => mockTheme),
	isDark: vi.fn(() => mockTheme === 'dark'),
	getThemeState: vi.fn(() => ({ current: mockTheme }))
}));

// Mock accent store
let mockAccent = 'cyan';
vi.mock('$lib/stores/accent', () => ({
	accent: { current: 'cyan' },
	setAccent: vi.fn((color: string) => {
		mockAccent = color;
	}),
	getAccent: vi.fn(() => mockAccent),
	getAccentPreset: vi.fn(() => ({ name: 'Cyan', value: 'cyan', color: '#00f0ff', description: 'Default neon blue' })),
	getAccentColors: vi.fn(() => [
		{ name: 'Cyan', value: 'cyan', color: '#00f0ff', description: 'Default neon blue' },
		{ name: 'Magenta', value: 'magenta', color: '#ff00aa', description: 'Hot pink' },
		{ name: 'Purple', value: 'purple', color: '#8b5cf6', description: 'Electric violet' },
		{ name: 'Green', value: 'green', color: '#00ff88', description: 'Matrix green' },
		{ name: 'Orange', value: 'orange', color: '#ff6b35', description: 'Sunset orange' },
		{ name: 'Yellow', value: 'yellow', color: '#fbbf24', description: 'Golden yellow' },
	]),
	cycleAccent: vi.fn(() => {
		const colors = ['cyan', 'magenta', 'purple', 'green', 'orange', 'yellow'];
		const idx = colors.indexOf(mockAccent);
		mockAccent = colors[(idx + 1) % colors.length];
	}),
	getAccentState: vi.fn(() => ({ current: mockAccent })),
	ACCENT_PRESETS: [
		{ name: 'Cyan', value: 'cyan', color: '#00f0ff', description: 'Default neon blue' },
		{ name: 'Magenta', value: 'magenta', color: '#ff00aa', description: 'Hot pink' },
		{ name: 'Purple', value: 'purple', color: '#8b5cf6', description: 'Electric violet' },
		{ name: 'Green', value: 'green', color: '#00ff88', description: 'Matrix green' },
		{ name: 'Orange', value: 'orange', color: '#ff6b35', description: 'Sunset orange' },
		{ name: 'Yellow', value: 'yellow', color: '#fbbf24', description: 'Golden yellow' },
	]
}));

// Mock tags store
const mockPresetTags = new Map<string, string[]>();
vi.mock('$lib/stores/tags', () => ({
	addTag: vi.fn((path: string, tag: string) => {
		const tags = mockPresetTags.get(path) || [];
		if (!tags.includes(tag)) {
			mockPresetTags.set(path, [...tags, tag]);
		}
	}),
	removeTag: vi.fn((path: string, tag: string) => {
		const tags = mockPresetTags.get(path) || [];
		mockPresetTags.set(path, tags.filter((t) => t !== tag));
	}),
	toggleTag: vi.fn((path: string, tag: string) => {
		const tags = mockPresetTags.get(path) || [];
		if (tags.includes(tag)) {
			mockPresetTags.set(path, tags.filter((t) => t !== tag));
			return false;
		} else {
			mockPresetTags.set(path, [...tags, tag]);
			return true;
		}
	}),
	hasTag: vi.fn((path: string, tag: string) => {
		const tags = mockPresetTags.get(path) || [];
		return tags.includes(tag);
	}),
	getPresetTags: vi.fn((path: string) => mockPresetTags.get(path) || []),
	getTaggedPresets: vi.fn(() => []),
	getAllTags: vi.fn(() => []),
	getTagCount: vi.fn(() => 0),
	getTagPresetCount: vi.fn(() => 0),
	clearPresetTags: vi.fn((path: string) => mockPresetTags.delete(path)),
	clearAllTags: vi.fn(() => mockPresetTags.clear()),
	getTags: vi.fn(() => ({
		presetTags: mockPresetTags,
		tagPresets: new Map(),
		allTags: new Set()
	})),
	tags: {
		presetTags: mockPresetTags,
		tagPresets: new Map(),
		allTags: new Set()
	}
}));

// Mock window.__TAURI__
Object.defineProperty(window, '__TAURI__', {
	value: {
		invoke: vi.fn()
	},
	writable: true
});
