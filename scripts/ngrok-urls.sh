#!/usr/bin/env bash
# Print ngrok public URLs and write PUBLIC_* (+ local BACKEND_URL) into .env.
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
json="$(curl -sf http://127.0.0.1:4040/api/tunnels)" || {
  echo "ngrok not running (no http://127.0.0.1:4040) — just ngrok" >&2
  exit 1
}

front=""
back=""
while IFS=$'\t' read -r name addr url; do
  name_l="$(printf '%s' "${name}" | tr '[:upper:]' '[:lower:]')"
  if [[ "${addr}" == *":3000"* || "${name_l}" == "frontend" ]]; then
    front="${url}"
  elif [[ "${addr}" == *":8080"* || "${name_l}" == "backend" ]]; then
    back="${url}"
  fi
done < <(printf '%s' "${json}" | python3 -c '
import json, sys
data = json.load(sys.stdin)
for t in data.get("tunnels", []):
    name = t.get("name") or ""
    url = t.get("public_url") or ""
    addr = (t.get("config") or {}).get("addr") or ""
    print(f"{name}\t{addr}\t{url}")
')

echo "PUBLIC_FRONTEND_URL=${front:-<missing>}"
echo "PUBLIC_BACKEND_URL=${back:-<missing — UI demos ok; agents need a second tunnel>}"
echo "BACKEND_URL=http://127.0.0.1:8080  # keep local (frontend server-side proxy)"

env_file="${root}/.env"
if [[ -f "${env_file}" ]]; then
  tmp="$(mktemp)"
  grep -vE '^(PUBLIC_FRONTEND_URL|PUBLIC_BACKEND_URL|BACKEND_URL)=' "${env_file}" > "${tmp}" || true
  {
    echo "BACKEND_URL=http://127.0.0.1:8080"
    [[ -n "${front}" ]] && echo "PUBLIC_FRONTEND_URL=${front}"
    [[ -n "${back}" ]] && echo "PUBLIC_BACKEND_URL=${back}"
  } >> "${tmp}"
  mv "${tmp}" "${env_file}"
  echo "updated ${env_file}"
fi
