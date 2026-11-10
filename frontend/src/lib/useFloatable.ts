import { useCallback, useEffect, useRef, useState } from "react";
import { pageZoom } from "./uiScale";

export type Floatable = {
  floating: boolean;
  pos: { x: number; y: number };
  pop: () => void;   // оторвать в float
  dock: () => void;  // вернуть в шапку
  dragging: boolean;
  onDragStart: (e: React.PointerEvent) => void;
};

// Держим панель в пределах видимого окна. Защита от x/y за экраном: битый сохранённый pos
// (например x=innerWidth-360 при innerWidth=0 -> -360) или сжатие окна не должны прятать панель.
function clampPos(p: { x: number; y: number }): { x: number; y: number } {
  const vw = (window.innerWidth || 1280) / pageZoom();
  const vh = (window.innerHeight || 800) / pageZoom();
  const x = Math.round(Math.max(4, Math.min(vw - 60, Number.isFinite(p.x) ? p.x : vw - 360)));
  const y = Math.round(Math.max(4, Math.min(vh - 40, Number.isFinite(p.y) ? p.y : 64)));
  return { x, y };
}

// Докнут в шапке ↔ оторван во float и таскается по всему окну. Позиция и режим — в localStorage.
export function useFloatable(key: string, initial: { x: number; y: number }): Floatable {
  const [floating, setFloating] = useState<boolean>(() => localStorage.getItem(`fl:${key}:on`) === "1");
  const [pos, setPos] = useState<{ x: number; y: number }>(() => {
    try { const s = localStorage.getItem(`fl:${key}:pos`); if (s) return clampPos(JSON.parse(s)); } catch { /* ignore */ }
    return clampPos(initial);
  });
  const [dragging, setDragging] = useState(false);
  const off = useRef({ x: 0, y: 0 });

  useEffect(() => { localStorage.setItem(`fl:${key}:on`, floating ? "1" : "0"); }, [key, floating]);
  useEffect(() => { localStorage.setItem(`fl:${key}:pos`, JSON.stringify(pos)); }, [key, pos]);
  // окно уменьшилось/повернулось -> подтянуть панель обратно в видимую область (не оставить за краем).
  useEffect(() => {
    const onResize = () => setPos((p) => clampPos(p));
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const onDragStart = useCallback((e: React.PointerEvent) => {
    if (e.button !== 0) return;
    const z = pageZoom();
    off.current = { x: e.clientX / z - pos.x, y: e.clientY / z - pos.y };
    setDragging(true);
    e.preventDefault();
  }, [pos]);

  useEffect(() => {
    if (!dragging) return;
    const move = (e: PointerEvent) => {
      const z = pageZoom();
      const x = Math.max(4, Math.min(window.innerWidth / z - 60, e.clientX / z - off.current.x));
      const y = Math.max(4, Math.min(window.innerHeight / z - 40, e.clientY / z - off.current.y));
      setPos({ x, y });
    };
    const up = () => setDragging(false);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
    return () => { window.removeEventListener("pointermove", move); window.removeEventListener("pointerup", up); };
  }, [dragging]);

  return {
    floating, pos, dragging,
    // при отрыве во float всегда подтягиваем в видимую область (страховка от битого сохранённого pos).
    pop: () => { setPos((p) => clampPos(p)); setFloating(true); },
    dock: () => setFloating(false),
    onDragStart,
  };
}

// Слот в шапке для докнутых баров (порталим сюда).
export function dockSlot(): HTMLElement | null {
  return typeof document !== "undefined" ? document.getElementById("dock-slot") : null;
}
