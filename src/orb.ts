import type { Snapshot } from "../contracts/hud-api.ts";

/** Use the same recommendation as the expanded footer; aggregate coverage is unrelated. */
export function orbDisplay(snapshot: Pick<Snapshot, "totalQuota" | "recommendation">, now: number) {
  const percent = snapshot.totalQuota?.percent;
  const known = percent != null && Number.isFinite(percent);
  const waiting = snapshot.recommendation.state === "waiting";
  const resetAt = snapshot.recommendation.estimatedAvailableAt;
  const minutes = resetAt ? Math.ceil((Date.parse(resetAt) - now) / 60000) : NaN;
  const timeLines = waiting && Number.isFinite(minutes) && minutes > 0
    ? minutes >= 1440
      ? [`${Math.floor(minutes / 1440)}d`, `${Math.floor(minutes % 1440 / 60)}h`]
      : [`${Math.floor(minutes / 60)}h`, `${minutes % 60}m`]
    : null;
  const text = waiting
    ? timeLines?.join(" ") ?? "—"
    : known && (percent > 0 || !snapshot.totalQuota?.partial)
      ? `${percent > 0 && percent < 1 ? "<1" : Math.floor(percent)}%`
      : "—";
  return { waiting, timeLines, text };
}
