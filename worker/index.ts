import { Hono } from "hono";
import { HTTPException } from "hono/http-exception";
import { applyApiCacheControl } from "./cache-control";
export { ProofCoordinatorDO } from "./durable/coordinator";
import type { WorkerEnv } from "./env";
import { createApiRouter } from "./api/routes";
import { handleQueueBatch } from "./queue/consumer";
import type { ProofQueueMessage } from "./types";
import { safeErrorMessage } from "./utils";

const app = new Hono<{ Bindings: WorkerEnv }>();

app.use("/api/*", async (c, next) => {
  await next();
  applyApiCacheControl(c.res);
});

app.route("/api", createApiRouter());

app.notFound((c) => {
  if (c.req.path.startsWith("/api/")) {
    return c.json(
      {
        success: false,
        error: `unknown api route: ${c.req.path}`,
      },
      404,
    );
  }

  return c.env.ASSETS.fetch(c.req.raw);
});

app.onError((error, c) => {
  if (error instanceof HTTPException) {
    const response = error.getResponse();
    if (c.req.path.startsWith("/api/")) {
      applyApiCacheControl(response);
    }
    return response;
  }

  console.error(`[proof-worker] ${safeErrorMessage(error)}`);

  if (c.req.path.startsWith("/api/")) {
    return c.json(
      {
        success: false,
        error: "internal server error",
      },
      500,
    );
  }

  return new Response("Internal Server Error", { status: 500 });
});

export default {
  fetch(
    request: Request,
    env: WorkerEnv,
    executionCtx: ExecutionContext,
  ): Response | Promise<Response> {
    return app.fetch(request, env, executionCtx);
  },

  async queue(batch: MessageBatch<unknown>, env: WorkerEnv): Promise<void> {
    await handleQueueBatch(batch as MessageBatch<ProofQueueMessage>, env);
  },
} satisfies ExportedHandler<WorkerEnv>;
