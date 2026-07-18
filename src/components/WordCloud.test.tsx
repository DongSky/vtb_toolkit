import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import WordCloud from "./WordCloud";

describe("WordCloud", () => {
  it("renders nothing for empty input", () => {
    const { container } = render(<WordCloud words={[]} />);
    expect(container.firstChild).toBeNull();
  });

  it("renders each word and sizes the hottest larger", () => {
    render(
      <WordCloud
        words={[
          ["草", 100],
          ["哈哈哈", 50],
          ["泪目", 10],
        ]}
      />,
    );
    const items = screen.getAllByTestId("word-cloud-item");
    expect(items).toHaveLength(3);
    const big = parseFloat((items[0] as HTMLElement).style.fontSize);
    const small = parseFloat((items[2] as HTMLElement).style.fontSize);
    expect(big).toBeGreaterThan(small);
    expect(screen.getByText("草")).toBeInTheDocument();
    expect(screen.getByTitle("泪目: 10")).toBeInTheDocument();
  });

  it("caps rendered words at max", () => {
    const words: [string, number][] = Array.from({ length: 80 }, (_, i) => [
      `w${i}`,
      80 - i,
    ]);
    render(<WordCloud words={words} max={20} />);
    expect(screen.getAllByTestId("word-cloud-item")).toHaveLength(20);
  });
});
