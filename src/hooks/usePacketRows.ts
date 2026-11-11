// Fetches packet rows in pages as the list scrolls, and keeps a small cache of them.
// Positions are in the filtered list, which only grows while the filter stays the same,
// so only the last, partly filled page ever needs fetching again.

import { useCallback, useEffect, useReducer, useRef } from "react";

import { backend } from "../lib/backend";
import type { PacketRow } from "../lib/types";

const PAGE = 100;
/** Most pages kept; the ones furthest from the view are dropped first. */
const KEEP_PAGES = 60;

export interface PacketRows {
  /** The row at a position, if it has arrived. */
  rowAt(position: number): PacketRow | undefined;
  /** Asks for the rows between two positions (inclusive). */
  ensure(first: number, last: number): void;
  /** Finds where a frame number sits, among the rows fetched so far. */
  positionOf(number: number): number | null;
}

/** `key` changes whenever the list is a different list (new capture, new filter). */
export function usePacketRows(key: string, shown: number): PacketRows {
  const cache = useRef(new Map<number, PacketRow[]>());
  const pending = useRef(new Set<number>());
  const generation = useRef(key);
  const [, refresh] = useReducer((n: number) => n + 1, 0);

  if (generation.current !== key) {
    generation.current = key;
    cache.current = new Map();
    pending.current = new Set();
  }

  const shownRef = useRef(shown);
  shownRef.current = shown;

  const fetchPage = useCallback((page: number) => {
    const asked = generation.current;
    pending.current.add(page);
    backend
      .captureRows(page * PAGE, PAGE)
      .then((rows) => {
        if (generation.current !== asked) return;
        cache.current.set(page, rows);
        refresh();
      })
      .catch(console.error)
      .finally(() => {
        if (generation.current === asked) pending.current.delete(page);
      });
  }, []);

  const ensure = useCallback(
    (first: number, last: number) => {
      const total = shownRef.current;
      if (total === 0) return;
      const lastPage = Math.floor(Math.min(last, total - 1) / PAGE);
      const firstPage = Math.floor(Math.max(0, first) / PAGE);
      for (let page = firstPage; page <= lastPage; page++) {
        if (pending.current.has(page)) continue;
        const rows = cache.current.get(page);
        const expected = Math.min(PAGE, total - page * PAGE);
        if (!rows || rows.length < expected) fetchPage(page);
      }
      // Forget pages far away from the view.
      if (cache.current.size > KEEP_PAGES) {
        const middle = (firstPage + lastPage) / 2;
        const byDistance = [...cache.current.keys()].sort((a, b) => Math.abs(b - middle) - Math.abs(a - middle));
        for (const page of byDistance.slice(0, cache.current.size - KEEP_PAGES)) cache.current.delete(page);
      }
    },
    [fetchPage],
  );

  // A new list starts empty; the view asks for what it needs.
  useEffect(() => refresh(), [key]);

  const rowAt = useCallback((position: number) => cache.current.get(Math.floor(position / PAGE))?.[position % PAGE], []);

  const positionOf = useCallback((number: number) => {
    for (const [page, rows] of cache.current) {
      const index = rows.findIndex((row) => row.number === number);
      if (index >= 0) return page * PAGE + index;
    }
    return null;
  }, []);

  return { rowAt, ensure, positionOf };
}
