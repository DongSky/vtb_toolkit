import { useLocale } from "../i18n";
interface WordCloudProps {
  /** [word, count] pairs (from stats_report.word_freq, already top-N). */
  words: [string, number][];
  /** Cap the number of words rendered. */
  max?: number;
}

/** Map a count within [min,max] to a font size in px. */
function fontSize(count: number, min: number, max: number): number {
  if (max <= min) return 20;
  const t = (count - min) / (max - min);
  return Math.round(12 + t * 26); // 12–38px
}

const HUES = [210, 340, 20, 120, 275, 45];

/**
 * A lightweight tag-style word cloud — no canvas/layout library, just
 * sized inline words. Frequency drives size and opacity so the eye lands
 * on the hot terms (LAPLACE-style 词云).
 */
export default function WordCloud({ words, max = 60 }: WordCloudProps) {
  useLocale();
  const shown = words.slice(0, max);
  if (shown.length === 0) return null;
  const counts = shown.map(([, c]) => c);
  const lo = Math.min(...counts);
  const hi = Math.max(...counts);

  return (
    <div className="word-cloud" data-testid="word-cloud">
      {shown.map(([word, count], i) => {
        const size = fontSize(count, lo, hi);
        const opacity = 0.55 + 0.45 * ((count - lo) / Math.max(hi - lo, 1));
        return (
          <span
            key={word}
            className="word-cloud-item"
            data-testid="word-cloud-item"
            title={`${word}: ${count}`}
            style={{
              fontSize: size,
              opacity,
              color: `hsl(${HUES[i % HUES.length]}, 60%, 45%)`,
            }}
          >
            {word}
          </span>
        );
      })}
    </div>
  );
}
