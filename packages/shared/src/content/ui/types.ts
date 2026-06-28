import type { PaletteKey } from "../assets/types.js";

export type UiIconShape = "heart" | "drumstick";

export type UiIconVariant = "full" | "half" | "empty";

/** One cell in the pixel grid (5×5 default). */
export type UiIconCell = {
  x: number;
  y: number;
  /** When true, cell is omitted in `half` variant (heart left-only fill). */
  skipInHalf?: boolean;
};

export type UiIconDef = {
  kind: string;
  shape: UiIconShape;
  variant: UiIconVariant;
  fillPalette: PaletteKey;
  bgPalette: PaletteKey;
  grid: number;
  px: number;
  cells: UiIconCell[];
};
