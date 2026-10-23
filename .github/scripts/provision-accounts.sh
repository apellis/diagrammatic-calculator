#!/usr/bin/env bash
# Provisions what the accounts API (phase 6) needs on Cloudflare, idempotently, before each deploy:
#   - the D1 database "diagcalc" (created if missing; its ID is written to web/wrangler.toml),
#   - the R2 bucket "diagcalc" (created if missing),
#   - the Pages secret SESSION_SECRET (a fresh random value, set only if the project has none),
#   - the DMARC record _dmarc.send.ape.computer (created if missing).
# Each part reports one line in the run summary. A part the API token has no permission for is
# skipped with a note saying which permission to add; nothing here fails the workflow. If the
# database and bucket are both available, ACCOUNTS=true is exported and the deploy binds them;
# otherwise web/wrangler.toml is removed and the site deploys without accounts.
#
# Needs CLOUDFLARE_API_TOKEN, CLOUDFLARE_ACCOUNT_ID; optional D1_DATABASE_ID (overrides lookup).
# Run from web/. Secret values are never printed.
set -uo pipefail

API=https://api.cloudflare.com/client/v4
ACC="$API/accounts/$CLOUDFLARE_ACCOUNT_ID"
NAME=diagcalc
ZONE=ape.computer
DMARC_NAME=_dmarc.send.ape.computer
DMARC_VALUE='v=DMARC1; p=quarantine; adkim=s; aspf=s'

summary() { echo "$1" >> "${GITHUB_STEP_SUMMARY:-/dev/stdout}"; }
cf() { # method url [json]
  if [ $# -ge 3 ]; then
    curl -sS -X "$1" "$2" -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN" \
      -H "Content-Type: application/json" --data "$3"
  else
    curl -sS -X "$1" "$2" -H "Authorization: Bearer $CLOUDFLARE_API_TOKEN"
  fi
}
errors() { jq -r '[.errors[]? | "\(.code): \(.message)"] | join("; ")' 2>/dev/null; }
ok() { jq -e '.success == true' >/dev/null 2>&1; }

uuid_re='^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'

# --- D1 -----------------------------------------------------------------------------------------
db_id=""
if [ -n "${D1_DATABASE_ID:-}" ] && printf '%s' "$D1_DATABASE_ID" | grep -Eq "$uuid_re"; then
  db_id="$D1_DATABASE_ID"
  summary "D1: database ID taken from the D1_DATABASE_ID secret."
else
  r=$(cf GET "$ACC/d1/database?name=$NAME")
  if printf '%s' "$r" | ok; then
    db_id=$(printf '%s' "$r" | jq -r --arg n "$NAME" '[.result[]? | select(.name == $n) | .uuid][0] // ""')
    if [ -n "$db_id" ]; then
      summary "D1: database \"$NAME\" found."
    else
      r=$(cf POST "$ACC/d1/database" "{\"name\":\"$NAME\"}")
      if printf '%s' "$r" | ok; then
        db_id=$(printf '%s' "$r" | jq -r '.result.uuid')
        summary "D1: database \"$NAME\" created."
      else
        summary "D1: could not create the database ($(printf '%s' "$r" | errors))."
      fi
    fi
  else
    summary "D1: not available to the API token; add the account permission D1: Edit ($(printf '%s' "$r" | errors))."
  fi
fi

# --- R2 -----------------------------------------------------------------------------------------
bucket=false
r=$(cf GET "$ACC/r2/buckets/$NAME")
if printf '%s' "$r" | ok; then
  bucket=true
  summary "R2: bucket \"$NAME\" found."
elif printf '%s' "$r" | jq -e '[.errors[]?.code] | index(10006)' >/dev/null 2>&1; then
  r=$(cf POST "$ACC/r2/buckets" "{\"name\":\"$NAME\"}")
  if printf '%s' "$r" | ok; then
    bucket=true
    summary "R2: bucket \"$NAME\" created."
  else
    summary "R2: could not create the bucket ($(printf '%s' "$r" | errors)). If R2 is not yet enabled, enable it once under R2 in the dashboard."
  fi
else
  summary "R2: not available ($(printf '%s' "$r" | errors)). Add the account permission Workers R2 Storage: Edit, and enable R2 in the dashboard if it never was."
fi

# --- SESSION_SECRET -------------------------------------------------------------------------------
r=$(cf GET "$ACC/pages/projects/$NAME")
if printf '%s' "$r" | ok; then
  if printf '%s' "$r" | jq -e '.result.deployment_configs.production.env_vars.SESSION_SECRET' >/dev/null 2>&1; then
    summary "SESSION_SECRET: already set on the Pages project."
  else
    secret=$(openssl rand -base64 48 | tr -d '\n')
    body=$(jq -cn --arg v "$secret" '{deployment_configs:{production:{env_vars:{SESSION_SECRET:{type:"secret_text",value:$v}}}}}')
    unset secret
    r=$(cf PATCH "$ACC/pages/projects/$NAME" "$body")
    unset body
    if printf '%s' "$r" | ok; then
      summary "SESSION_SECRET: generated and set on the Pages project."
    else
      summary "SESSION_SECRET: could not be set ($(printf '%s' "$r" | errors))."
    fi
  fi
  for s in GOOGLE_CLIENT_ID GOOGLE_CLIENT_SECRET RESEND_API_KEY; do
    if printf '%s' "$r" | jq -e --arg s "$s" '.result.deployment_configs.production.env_vars[$s]' >/dev/null 2>&1; then
      summary "$s: set."
    else
      summary "$s: not set yet on the Pages project."
    fi
  done
else
  summary "Pages project: could not be read ($(printf '%s' "$r" | errors))."
fi

# --- DMARC ----------------------------------------------------------------------------------------
r=$(cf GET "$API/zones?name=$ZONE")
zone=$(printf '%s' "$r" | jq -r '.result[0].id // ""' 2>/dev/null)
if [ -z "$zone" ]; then
  summary "DMARC: zone $ZONE not visible to the API token; add the zone permission DNS: Edit for $ZONE ($(printf '%s' "$r" | errors))."
else
  r=$(cf GET "$API/zones/$zone/dns_records?type=TXT&name=$DMARC_NAME")
  if ! printf '%s' "$r" | ok; then
    summary "DMARC: DNS records not readable; add the zone permission DNS: Edit for $ZONE ($(printf '%s' "$r" | errors))."
  elif [ "$(printf '%s' "$r" | jq '.result | length')" -gt 0 ]; then
    summary "DMARC: record $DMARC_NAME found."
  else
    body=$(jq -cn --arg n "$DMARC_NAME" --arg c "\"$DMARC_VALUE\"" '{type:"TXT",name:$n,content:$c,ttl:1}')
    r=$(cf POST "$API/zones/$zone/dns_records" "$body")
    if printf '%s' "$r" | ok; then
      summary "DMARC: record $DMARC_NAME created."
    else
      summary "DMARC: could not create the record; add the zone permission DNS: Edit for $ZONE ($(printf '%s' "$r" | errors))."
    fi
  fi
fi

# --- Result ---------------------------------------------------------------------------------------
if [ -n "$db_id" ] && [ "$bucket" = true ]; then
  sed -i "s/^database_id = .*/database_id = \"$db_id\"/" wrangler.toml
  echo "ACCOUNTS=true" >> "${GITHUB_ENV:-/dev/null}"
  summary "Accounts: D1 database and R2 bucket bound, migrations applied."
else
  rm -f wrangler.toml
  echo "ACCOUNTS=false" >> "${GITHUB_ENV:-/dev/null}"
  summary "Accounts: storage not ready; deployed without accounts (the app stays anonymous)."
fi
exit 0
