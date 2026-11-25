import { useEffect } from "react";

/** Editors that currently hold unsaved drafts, keyed by a stable id. */
const dirtyEditors = new Map<string, string>();

/**
 * Registers an editor's dirty state so tab switches and item switches can ask
 * before throwing the draft away.
 */
export function useUnsavedGuard(key: string, dirty: boolean, label: string) {
  useEffect(() => {
    if (dirty) dirtyEditors.set(key, label);
    else dirtyEditors.delete(key);
    return () => {
      dirtyEditors.delete(key);
    };
  }, [key, dirty, label]);
}

/**
 * Returns true when it is safe to continue: nothing is dirty, or the user
 * agreed to discard. Pass `only` to check a single editor.
 */
export function confirmDiscard(only?: string): boolean {
  const labels = only
    ? dirtyEditors.has(only)
      ? [dirtyEditors.get(only)!]
      : []
    : [...dirtyEditors.values()];
  if (labels.length === 0) return true;
  return window.confirm(
    `You have unsaved changes in ${labels.join(", ")}. Discard them?`
  );
}

/** Line-ending-insensitive comparison for "is this draft dirty?" checks. */
export function sameText(a: string, b: string): boolean {
  const norm = (s: string) => s.replace(/\r\n?/g, "\n").replace(/\n+$/, "");
  return norm(a) === norm(b);
}
