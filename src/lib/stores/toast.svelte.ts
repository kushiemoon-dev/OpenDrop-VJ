/**
 * Toast notification store
 *
 * Usage:
 *   import { toast, showToast } from '$lib/stores/toast';
 *   showToast('Message', 'error');
 */

type ToastType = 'info' | 'success' | 'error' | 'warning';

interface ToastState {
  message: string;
  type: ToastType;
  visible: boolean;
}

let toastState = $state<ToastState>({
  message: '',
  type: 'info',
  visible: false
});

let timeoutId: ReturnType<typeof setTimeout> | null = null;

/**
 * Show a toast notification
 * @param message - The message to display
 * @param type - The type of toast (info, success, error, warning)
 * @param duration - How long to show the toast in ms (default: 3000)
 */
export function showToast(message: string, type: ToastType = 'info', duration: number = 3000): void {
  if (timeoutId) {
    clearTimeout(timeoutId);
  }

  toastState.message = message;
  toastState.type = type;
  toastState.visible = true;

  timeoutId = setTimeout(() => {
    toastState.visible = false;
    timeoutId = null;
  }, duration);
}

/**
 * Hide the current toast immediately
 */
export function hideToast(): void {
  if (timeoutId) {
    clearTimeout(timeoutId);
    timeoutId = null;
  }
  toastState.visible = false;
}

/**
 * Get the current toast state (reactive)
 */
export function getToast(): ToastState {
  return toastState;
}

export { toastState as toast };
