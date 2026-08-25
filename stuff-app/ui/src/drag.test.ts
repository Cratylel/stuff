import { describe, expect, it } from 'vitest';
import { elevationDragPosition, sideDragPosition, topDragPosition } from './drag';

const parent = { width: 1000, depth: 800, height: 500 };
const box = { width: 200, depth: 100, height: 50 };

describe('canvas drag conversion', () => {
  it('uses absolute top-down coordinates and clamps them to the parent', () => {
    expect(topDragPosition({ x: 90, y: 42 }, 0.1, { x: 5, y: 6, z: 7 }, box, parent)).toEqual({
      x: 800,
      y: 420,
      z: 7,
    });
    expect(topDragPosition({ x: -5, y: 999 }, 0.1, { x: 5, y: 6, z: 7 }, box, parent)).toEqual({
      x: 0,
      y: 700,
      z: 7,
    });
  });

  it('maps elevation drag into X/Z, with upward canvas movement increasing Z', () => {
    expect(elevationDragPosition({ x: 30, y: 25 }, 0.1, { x: 1, y: 222, z: 3 }, box, parent)).toEqual({
      x: 300,
      y: 222,
      z: 200,
    });
    expect(elevationDragPosition({ x: 30, y: 45 }, 0.1, { x: 1, y: 222, z: 3 }, box, parent).z).toBe(0);
  });

  it('maps shelf side drag into depth/Z and keeps X unchanged', () => {
    expect(sideDragPosition({ x: 30, y: 25 }, 0.1, { x: 91, y: 2, z: 3 }, box, parent)).toEqual({
      x: 91,
      y: 300,
      z: 200,
    });
  });

  it('rounds canvas-derived positions to integer millimetres for Tauri commands', () => {
    expect(topDragPosition({ x: 26.41747572815534, y: 10.271844660194176 }, 0.103, { x: 0, y: 0, z: 0 }, box, parent)).toEqual({
      x: 256,
      y: 100,
      z: 0,
    });
  });
});
