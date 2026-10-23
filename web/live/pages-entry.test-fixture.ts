// Test-only Pages ingress in workerd. NEVER configure deployment to this entrypoint.
// No auth shortcuts or internal routes: the production router owns every request.
import { handle } from '../server/router';
import type { Env } from '../server/env';
export { DocRoom } from './room';
export default { fetch(req: Request, env: Env) { return handle(req, env); } };
