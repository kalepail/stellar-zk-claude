export const API_CACHE_CONTROL = "no-store";

export function applyApiCacheControl(response: Response): void {
  response.headers.set("cache-control", API_CACHE_CONTROL);
}
