export const lightQuotaColors = ["#BA3038", "#A75B12", "#286FAD", "#28784E"] as const;
export const darkQuotaColors = ["#FF8585", "#FFBD70", "#80BCFF", "#77D7A0"] as const;
export function quotaBandForPercent(percent: number | null | undefined): string {
  if (percent == null || !Number.isFinite(percent)) return "";
  return percent < 20 ? "quota-red" : percent < 50 ? "quota-orange" : percent < 80 ? "quota-blue" : "quota-green";
}
