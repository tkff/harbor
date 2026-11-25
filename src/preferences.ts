import type { ITerminalOptions } from "@xterm/xterm";
export type Preferences = {
  version: number;
  connectionLogging: boolean;
  sessionLogging: boolean;
  theme: "system" | "light" | "dark";
  accent: "teal" | "blue" | "violet";
  density: "comfortable" | "compact";
  reducedMotion: boolean;
  fontFamily: "system" | "cascadia" | "consolas" | "jetbrains";
  fontSize: number;
  lineHeight: number;
  terminalTheme: "harbor" | "light" | "contrast";
  cursorStyle: "bar" | "block" | "underline";
  cursorBlink: boolean;
  scrollback: number;
  defaultPort: number;
  defaultGroup: string;
};
export const defaultPreferences: Preferences = {
  version: 1,
  connectionLogging: true,
  sessionLogging: false,
  theme: "system",
  accent: "teal",
  density: "comfortable",
  reducedMotion: false,
  fontFamily: "system",
  fontSize: 14,
  lineHeight: 1.35,
  terminalTheme: "harbor",
  cursorStyle: "bar",
  cursorBlink: true,
  scrollback: 5000,
  defaultPort: 22,
  defaultGroup: "General",
};
export function terminalOptions(p: Preferences): ITerminalOptions {
  const families = {
    system:
      '"Cascadia Code", "SFMono-Regular", Consolas, "Liberation Mono", monospace',
    cascadia: '"Cascadia Code", Consolas, monospace',
    consolas: 'Consolas, "Liberation Mono", monospace',
    jetbrains: '"JetBrains Mono", Consolas, monospace',
  };
  const base = {
    black: "#18262d",
    red: "#ed8f8a",
    green: "#8dd6af",
    yellow: "#e5c68c",
    blue: "#8ebbdc",
    magenta: "#c6a3d6",
    cyan: "#7fc9c8",
    white: "#d9e3e5",
  };
  const themes = {
    harbor: {
      ...base,
      background: "#10191f",
      foreground: "#d9e3e5",
      cursor: "#6cd5bb",
      selectionBackground: "#33554f",
    },
    light: {
      background: "#f8fafc",
      foreground: "#182b3b",
      cursor: "#1d6a57",
      selectionBackground: "#b8d8f0",
      black: "#18262d",
      red: "#b42318",
      green: "#17663c",
      yellow: "#896000",
      blue: "#1d4ed8",
      magenta: "#802da0",
      cyan: "#086b78",
      white: "#65727e",
    },
    contrast: {
      ...base,
      background: "#000000",
      foreground: "#ffffff",
      cursor: "#ffffff",
      selectionBackground: "#414141",
    },
  };
  return {
    fontFamily: families[p.fontFamily],
    fontSize: p.fontSize,
    lineHeight: p.lineHeight,
    scrollback: p.scrollback,
    cursorStyle: p.cursorStyle,
    cursorBlink: p.cursorBlink,
    theme: themes[p.terminalTheme],
  };
}
export function samePreferences(a: Preferences, b: Preferences) {
  return (Object.keys(defaultPreferences) as (keyof Preferences)[]).every(
    (key) => a[key] === b[key],
  );
}
