/**
 * UI utility functions
 * Helpers for DOM manipulation and event handling
 */

import { BUTTON_INIT_DELAY_MS } from '../config';

/**
 * Replace a button to remove old event listeners
 * Returns the new button element
 */
export function replaceButton(buttonId: string): HTMLButtonElement | null {
  const oldButton = document.getElementById(buttonId) as HTMLButtonElement | null;
  if (!oldButton || !oldButton.parentNode) return null;

  const newButton = oldButton.cloneNode(true) as HTMLButtonElement;
  oldButton.parentNode.replaceChild(newButton, oldButton);
  return newButton;
}

/**
 * Set up a button with event listener after a delay
 * Delay ensures DOM is fully rendered
 */
export function setupButton(
  buttonId: string,
  handler: (event: MouseEvent) => void,
  delay: number = BUTTON_INIT_DELAY_MS
): void {
  setTimeout(() => {
    const button = replaceButton(buttonId);
    if (button) {
      button.addEventListener('click', handler as EventListener);
    }
  }, delay);
}

/**
 * Get element by ID with type safety
 */
export function getElement<T extends HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}

/**
 * Set element text content safely
 */
export function setTextContent(id: string, text: string): void {
  const element = getElement(id);
  if (element) {
    element.textContent = text;
  }
}

/**
 * Set image src safely
 */
export function setImageSrc(id: string, src: string): void {
  const element = getElement<HTMLImageElement>(id);
  if (element) {
    element.src = src;
  }
}
