#!/bin/sh
set -eu

backup_dir=${1:-ops/backups}
mkdir -p "$backup_dir"
stamp=$(date -u +%Y%m%dT%H%M%SZ)
file="$backup_dir/redis-$stamp.rdb"
temporary="$file.tmp"

docker compose exec -T redis sh -ec '
  if [ -n "$$REDIS_PASSWORD" ]; then
    REDISCLI_AUTH="$$REDIS_PASSWORD" redis-cli --rdb -
  else
    redis-cli --rdb -
  fi
' > "$temporary"
mv "$temporary" "$file"
shasum -a 256 "$file" > "$file.sha256"

find "$backup_dir" -type f -name '*.rdb' -print | sort -r | awk 'NR > 7 { print }' | while IFS= read -r old_file; do
  rm -f "$old_file" "$old_file.sha256"
done

printf '%s\n' "$file"
