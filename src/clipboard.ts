import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { isTauri } from "./api";
export async function copyTerminalText(text: string): Promise<void> {
  if (isTauri()) await writeText(text);
  else await navigator.clipboard.writeText(text);
}
