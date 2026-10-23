// The existing Accounts storage step resolves the Pages D1 binding. Copy that ID only;
// don't change its provisioning behavior or print secrets. Run from web/ after provisioning.
import { readFileSync, writeFileSync } from 'node:fs';
const pages = readFileSync('wrangler.toml', 'utf8');
const id = /^database_id\s*=\s*"([a-f0-9]{8}-(?:[a-f0-9]{4}-){3}[a-f0-9]{12})"\s*$/m.exec(pages)?.[1];
if (!id || id === '00000000-0000-0000-0000-000000000000') throw new Error('Accounts storage has not resolved the database');
const path = 'live/wrangler.toml';
const source = readFileSync(path, 'utf8');
if (!source.includes('database_id = "00000000-0000-0000-0000-000000000000"')) throw new Error('Unexpected live deployment template');
writeFileSync(path, source.replace('database_id = "00000000-0000-0000-0000-000000000000"', `database_id = "${id}"`));
console.log('Live room configured with the existing account database.');
