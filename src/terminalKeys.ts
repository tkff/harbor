export type CopyKey = Pick<
  KeyboardEvent,
  "key" | "ctrlKey" | "shiftKey" | "metaKey" | "altKey"
> & { code?: string };
export function isTerminalCopyKey(event: CopyKey, selected: boolean): boolean {
  if (
    (event.key.toLowerCase() !== "c" && event.code !== "KeyC") ||
    event.altKey
  )
    return false;
  if (event.metaKey && !event.ctrlKey) return true;
  return event.ctrlKey && !event.metaKey && (event.shiftKey || selected);
}

export function isTerminalInterruptKey(
  event: CopyKey,
  selected: boolean,
): boolean {
  return (
    !selected &&
    event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey &&
    !event.metaKey &&
    (event.key.toLowerCase() === "c" || event.code === "KeyC")
  );
}
