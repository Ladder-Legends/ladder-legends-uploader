/**
 * Tests for UI utility functions
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { replaceButton, setTextContent, setImageSrc, getElement } from '../lib/ui';

describe('ui', () => {
  beforeEach(() => {
    document.body.innerHTML = '';
  });

  describe('getElement', () => {
    it('should get element by ID', () => {
      document.body.innerHTML = '<div id="test">Test</div>';
      const element = getElement('test');
      expect(element).toBeTruthy();
      expect(element?.textContent).toBe('Test');
    });

    it('should return null for non-existent element', () => {
      const element = getElement('does-not-exist');
      expect(element).toBeNull();
    });
  });

  describe('setTextContent', () => {
    it('should set text content of element', () => {
      document.body.innerHTML = '<div id="test"></div>';
      setTextContent('test', 'Hello World');
      expect(document.getElementById('test')?.textContent).toBe('Hello World');
    });

    it('should handle non-existent element gracefully', () => {
      expect(() => setTextContent('does-not-exist', 'test')).not.toThrow();
    });
  });

  describe('setImageSrc', () => {
    it('should set image src attribute', () => {
      document.body.innerHTML = '<img id="test-img" />';
      setImageSrc('test-img', 'https://example.com/image.png');
      const img = document.getElementById('test-img') as HTMLImageElement;
      expect(img.src).toBe('https://example.com/image.png');
    });

    it('should handle non-existent element gracefully', () => {
      expect(() => setImageSrc('does-not-exist', 'test.png')).not.toThrow();
    });
  });

  describe('replaceButton', () => {
    it('should replace button and remove event listeners', () => {
      document.body.innerHTML = '<button id="test-btn">Click me</button>';

      const oldButton = document.getElementById('test-btn') as HTMLButtonElement;
      const handler = vi.fn();
      oldButton.addEventListener('click', handler);

      const newButton = replaceButton('test-btn');

      expect(newButton).toBeTruthy();
      expect(newButton?.id).toBe('test-btn');

      // Old handler should not be called on new button
      newButton?.click();
      expect(handler).not.toHaveBeenCalled();
    });

    it('should return null for non-existent button', () => {
      const result = replaceButton('does-not-exist');
      expect(result).toBeNull();
    });
  });
});
