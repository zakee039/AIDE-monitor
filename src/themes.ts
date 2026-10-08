import { lightQuotaColors, darkQuotaColors } from "./quota-colors";
import type { CSSProperties } from "react";
import cream from "../examples/themes/cream/theme.json";
import midnight from "../examples/themes/midnight/theme.json";

export interface ThemeDocument {
  schemaVersion: 1;
  minHudVersion: string;
  id: string;
  name: string;
  author?: string;
  description?: string;
  tokens: {
    colors: Record<"background" | "surface" | "text" | "textMuted" | "border" | "accent" | "accentText" | "success" | "warning" | "exhausted" | "error" | "unknown" | "stale", string>;
    typography: { fontFamily: "system" | "system-monospace"; fontSize: number; lineHeight: number };
    spacing: { padding: number; gap: number };
    radius: number;
  };
  layout: {
    preset: "compact-rows" | "cards" | "horizontal";
    density: "compact" | "comfortable";
    quotaVisualization: "text" | "bar-text";
    regionOrder: Array<"accounts" | "recommendation">;
    accountSlots: Array<"account-label" | "quota-windows" | "availability">;
  };
}

export const defaultTheme = cream as ThemeDocument;
export const midnightTheme = midnight as ThemeDocument;

export const paperTheme: ThemeDocument = {
  ...defaultTheme, id: "paper", name: "明亮",
  tokens: { ...defaultTheme.tokens, colors: { background: "#F7F8FA", surface: "#FFFFFF", text: "#182334", textMuted: "#4B596E", border: "#778396", accent: "#285AB3", accentText: "#FFFFFF", success: "#246844", warning: "#795419", exhausted: "#914512", error: "#A92D44", unknown: "#4B596E", stale: "#74508F" } },
};

/** Only explicit numeric/color tokens become CSS; a theme never injects CSS text. */
export function themeStyle(theme: ThemeDocument): CSSProperties {
  const styles: Record<string, string | number> = {};
  for (const [key, value] of Object.entries(theme.tokens.colors)) {
    if (/^#[0-9a-f]{6}$/i.test(value)) styles[`--${key.replace(/[A-Z]/g, letter => `-${letter.toLowerCase()}`)}`] = value;
  }
  const background = theme.tokens.colors.background;
  const rgb = [1, 3, 5].map(offset => parseInt(background.slice(offset, offset + 2), 16));
  const dark = rgb[0] * .2126 + rgb[1] * .7152 + rgb[2] * .0722 < 128;
  const quotaColors = dark ? darkQuotaColors : lightQuotaColors;
  ["red", "orange", "blue", "green"].forEach((color, index) => { styles[`--quota-${color}`] = quotaColors[index]; });
  const { typography, spacing, radius } = theme.tokens;
  styles["--theme-font-size"] = `${Math.max(12, Math.min(20, typography.fontSize))}px`;
  styles["--theme-line-height"] = Math.max(1.2, Math.min(1.8, typography.lineHeight));
  styles["--theme-padding"] = `${Math.max(4, Math.min(24, spacing.padding))}px`;
  styles["--theme-gap"] = `${Math.max(4, Math.min(20, spacing.gap))}px`;
  styles["--theme-radius"] = `${Math.max(0, Math.min(20, radius))}px`;
  styles.fontFamily = typography.fontFamily === "system-monospace" ? '"Cascadia Code", "SFMono-Regular", Consolas, monospace' : 'Inter, "Segoe UI", "Microsoft YaHei", system-ui, sans-serif';
  return styles as CSSProperties;
}
