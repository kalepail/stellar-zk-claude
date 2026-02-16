import { Hono } from "hono";
import { HTTPException } from "hono/http-exception";
import { applyApiCacheControl } from "./cache-control";
export { ProofCoordinatorDO } from "./durable/coordinator";
import type { WorkerEnv } from "./env";
import { createApiRouter } from "./api/routes";
import { recordLeaderboardSyncFailure, runScheduledLeaderboardSync } from "./leaderboard-sync";
import {
  handleClaimDlqBatch,
  handleClaimQueueBatch,
  handleDlqBatch,
  handleQueueBatch,
} from "./queue/consumer";
import type { ClaimQueueMessage, ProofQueueMessage } from "./types";
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
    if (batch.queue === "stellar-zk-proof-jobs-dlq") {
      await handleDlqBatch(batch as MessageBatch<ProofQueueMessage>, env);
    } else if (batch.queue === "stellar-zk-claim-jobs") {
      await handleClaimQueueBatch(batch as MessageBatch<ClaimQueueMessage>, env);
    } else if (batch.queue === "stellar-zk-claim-jobs-dlq") {
      await handleClaimDlqBatch(batch as MessageBatch<ClaimQueueMessage>, env);
    } else {
      await handleQueueBatch(batch as MessageBatch<ProofQueueMessage>, env);
    }
  },

  async scheduled(
    controller: ScheduledController,
    env: WorkerEnv,
    _executionCtx: ExecutionContext,
  ): Promise<void> {
    try {
      const result = await runScheduledLeaderboardSync(env, controller.scheduledTime);
      if (!result.enabled) {
        return;
      }

      if (result.warning) {
        console.warn(`[leaderboard-sync] ${result.warning}`);
      }
    } catch (error) {
      try {
        await recordLeaderboardSyncFailure(env, error);
      } catch (recordError) {
        console.error(
          `[leaderboard-sync] failed recording scheduled sync error: ${safeErrorMessage(recordError)}`,
        );
      }
      console.error(`[leaderboard-sync] scheduled sync failed: ${safeErrorMessage(error)}`);
    }
  },
} satisfies ExportedHandler<WorkerEnv>;
