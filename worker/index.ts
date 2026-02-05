export default {
  async fetch(request): Promise<Response> {
    const url = new URL(request.url);

    if (url.pathname.startsWith("/api/")) {
      return Response.json({
        message: "Hello from Cloudflare Workers!",
        path: url.pathname,
      });
    }

    // Fall through to assets (SPA)
    return new Response("Not Found", { status: 404 });
  },
} satisfies ExportedHandler;
