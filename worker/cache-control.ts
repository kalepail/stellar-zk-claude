export const API_CACHE_CONTROL = "no-store";
export const LEADERBOARD_CACHE_CONTROL =
  "public, max-age=5, s-maxage=15, stale-while-revalidate=30";
export const LEADERBOARD_PRIVATE_CACHE_CONTROL = "private, max-age=5, stale-while-revalidate=15";

export function applyApiCacheControl(response: Response): void {
  if (!response.headers.has("cache-control")) {
    response.headers.set("cache-control", API_CACHE_CONTROL);
  }
}
