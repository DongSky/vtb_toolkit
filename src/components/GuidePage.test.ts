import { expect, it, vi } from "vitest";
import guide from "../../public/guide/index.html?raw";

it("renders the offline guide in three languages, searches and retains section links", () => {
  const body = guide.match(/<body>([\s\S]*)<\/body>/)![1];
  document.body.innerHTML = body;
  const scripts = document.querySelectorAll("script");
  new Function(scripts[scripts.length - 1].textContent!)();
  const select = document.getElementById("language") as HTMLSelectElement;
  for (const lang of ["zh-CN", "en", "ja"]) {
    select.value = lang; select.dispatchEvent(new Event("change"));
    expect(document.documentElement.lang).toBe(lang);
    expect(document.querySelectorAll("section")).toHaveLength(20);
    expect(document.querySelectorAll("nav a")).toHaveLength(20);
    for (const link of document.querySelectorAll("nav a")) {
      expect(document.querySelector(link.getAttribute("href")!)).not.toBeNull();
    }
    expect(document.querySelector("#offline")).toHaveTextContent("Whisper");
  }
  const input = document.getElementById("search") as HTMLInputElement;
  input.value = "OAuth"; input.dispatchEvent(new Event("input"));
  expect(document.getElementById("accounts")).not.toHaveAttribute("hidden");
  expect(document.getElementById("themes")).toHaveAttribute("hidden");
  const print = vi.spyOn(window, "print").mockImplementation(() => {});
  (document.getElementById("print") as HTMLButtonElement).click();
  expect(print).toHaveBeenCalledOnce(); print.mockRestore();
  window.history.replaceState(null, "", "/");
});
