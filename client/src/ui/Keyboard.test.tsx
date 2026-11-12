import { fireEvent, render } from "@solidjs/testing-library";
import { describe, expect, it } from "vitest";
import type { RemoteController } from "../lib/remote";
import { Keyboard } from "./Keyboard";

function fakeRemote() {
  const calls: string[] = [];
  const r = {
    calls,
    activeModifiers: () => [],
    modifiers: { get: () => "off" },
    tap: (k: string) => calls.push(`tap:${k}`),
    text: (t: string) => calls.push(`text:${t}`),
    backspace: (n: number) => calls.push(`bs:${n}`),
    pressModifier: () => {},
  };
  return r as unknown as RemoteController & { calls: string[] };
}

describe("Keyboard", () => {
  it("une touche de contrôle ne rouvre pas le clavier du téléphone une fois replié", () => {
    const r = fakeRemote();
    const { container, getByText } = render(() => <Keyboard remote={r} />);
    const input = container.querySelector("input")!;
    input.blur();
    fireEvent.click(getByText("Tab"));
    expect(r.calls).toEqual(["tap:tab"]);
    expect(document.activeElement).not.toBe(input);
  });

  it("Entrée envoie le texte du champ puis la touche", () => {
    const r = fakeRemote();
    const { container, getByText } = render(() => <Keyboard remote={r} />);
    const input = container.querySelector("input")!;
    fireEvent.compositionStart(input);
    input.value = "salut";
    fireEvent.input(input);
    fireEvent.click(getByText("⏎"));
    expect(r.calls).toEqual(["text:salut", "tap:enter"]);
    expect(input.value).toBe("");
  });
});
