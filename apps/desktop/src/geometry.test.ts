import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const here = path.dirname(fileURLToPath(import.meta.url));
const css = readFileSync(path.join(here, "App.css"), "utf8");
const tauri = JSON.parse(
  readFileSync(path.join(here, "..", "src-tauri", "tauri.conf.json"), "utf8"),
) as { app: { windows: { minWidth: number; minHeight: number }[] } };

describe("new-UI geometry", () => {
  it("keeps the intentional 300px sidebar", () => {
    expect(css).toMatch(/--sidebar-w:\s*300px/);
  });

  it("keeps 52px pane headers on all three panes", () => {
    for (const selector of ["\\.sb-top", "\\.conv-head", "\\.os-head"]) {
      expect(css).toMatch(new RegExp(`${selector}\\s*\\{[^}]*height:\\s*52px`));
    }
  });

  it("declares a native 900px minimum window width", () => {
    expect(tauri.app.windows[0].minWidth).toBe(900);
  });

  it("adds a compact desktop rule so the three panes cannot overflow", () => {
    // The default with-os grid needs 300 + 440 + 400 = 1140px before it can
    // shrink, which overflows below 1080px. The compact rule must let the two
    // conversation/OS columns collapse toward zero so 900px fits.
    const compact = css.match(
      /@media[^{]*max-width:\s*1079px[^{]*\{([\s\S]*?)\}\s*\}/,
    );
    expect(compact).not.toBeNull();
    expect(compact?.[1]).toMatch(/\.body\.with-os/);
    expect(compact?.[1]).toMatch(/minmax\(\s*0/);
  });

  it("uses no mobile off-canvas transform", () => {
    expect(css).not.toMatch(/translateX\(\s*-\s*100%/);
    // No phone/tablet media breakpoints — this app is desktop-only.
    expect(css).not.toMatch(/@media[^{]*max-width:\s*(4|5|6|7|8)\d\dpx/);
  });

  it("respects reduced motion", () => {
    expect(css).toMatch(/@media\s*\(prefers-reduced-motion:\s*reduce\)/);
  });
});
