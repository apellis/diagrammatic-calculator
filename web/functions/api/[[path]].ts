// Pages Function for every request under /api/: the accounts API (server/router.ts). Static files
// are served without invoking it.
import { handle } from '../../server/router';
import type { Env } from '../../server/env';

export const onRequest = (context: { request: Request; env: Env }) => handle(context.request, context.env);
