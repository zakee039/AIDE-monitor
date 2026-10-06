import type { Snapshot } from "../contracts/hud-api.ts";

/** The orb countdown follows its selected provider, independently of the expanded footer. */
export function orbDisplay(snapshot: Pick<Snapshot, "totalQuota" | "recommendation">, now: number) {
  const percent = snapshot.totalQuota?.percent;
  const known = percent != null && Number.isFinite(percent);
  const recommendation = snapshot.totalQuota?.recommendation ?? snapshot.recommendation;
  const waiting = recommendation.state === "waiting" && !(known && percent > 0);
  const resetAt = recommendation.estimatedAvailableAt;
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
