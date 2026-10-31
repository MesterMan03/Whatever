export function percentile(values: readonly number[], fraction: number): number | null {
  if (!values.length) return null;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.min(sorted.length - 1, Math.ceil(sorted.length * fraction) - 1)] ?? null;
}
export function distribution(values: readonly number[]) {
  return {
    count: values.length,
    mean: values.length ? values.reduce((a, b) => a + b, 0) / values.length : null,
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
    p99: percentile(values, 0.99),
    max: values.length ? Math.max(...values) : null,
  };
}
export function score(milliseconds: number | null): number | null {
  return milliseconds !== null && milliseconds > 0 ? (1000 * (1000 / 60)) / milliseconds : null;
}
export function geometricMean(values: readonly (number | null)[]): number | null {
  const valid = values.filter(
    (value): value is number => value !== null && Number.isFinite(value) && value > 0,
  );
  return valid.length
    ? Math.exp(valid.reduce((sum, value) => sum + Math.log(value), 0) / valid.length)
    : null;
}
