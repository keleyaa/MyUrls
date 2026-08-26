#!/bin/sh
set -eu

backup_file=${1:?usage: redis-restore.sh BACKUP_FILE NEW_VOLUME [REDIS_IMAGE]}
volume=${2:?usage: redis-restore.sh BACKUP_FILE NEW_VOLUME [REDIS_IMAGE]}
image=${3:-redis:8.10.0@sha256:c29e49ab2f85760a3827b53882e6dd9f5c6c3f0bb7d724e07bb31cbf275a5236}

case "$volume" in
  ''|*[!A-Za-z0-9_.-]*)
    printf '%s\n' 'invalid restore volume name' >&2
    exit 2
    ;;
esac
test -f "$backup_file"

if docker volume inspect "$volume" >/dev/null 2>&1; then
  printf '%s\n' 'restore volume already exists; choose a new volume name' >&2
  exit 2
fi

checksum_file="$backup_file.sha256"
if [ -f "$checksum_file" ]; then
  checksum_dir=$(CDPATH= cd -- "$(dirname -- "$checksum_file")" && pwd)
  checksum_name=$(basename -- "$checksum_file")
  (CDPATH= cd -- "$checksum_dir" && shasum -a 256 -c "$checksum_name")
fi

docker volume create "$volume" >/dev/null
backup_dir=$(CDPATH= cd -- "$(dirname -- "$backup_file")" && pwd)
backup_name=$(basename -- "$backup_file")
docker run --rm --user root \
  --mount "type=volume,source=$volume,target=/data" \
  --mount "type=bind,source=$backup_dir,target=/backup,readonly" \
  "$image" sh -ec "cp /backup/$backup_name /data/dump.rdb && chown redis:redis /data/dump.rdb"

container="myurl-v2-restore-$$-$(date +%s)"
cleanup() {
  docker rm -f "$container" >/dev/null 2>&1 || true
}
trap cleanup EXIT HUP INT TERM

docker run -d --name "$container" \
  --mount "type=volume,source=$volume,target=/data" \
  "$image" redis-server --appendonly no >/dev/null

ready=0
for attempt in $(seq 1 30); do
  if docker exec "$container" redis-cli ping 2>/dev/null | grep -qx PONG; then
    ready=1
    break
  fi
  sleep 1
done
if [ "$ready" -ne 1 ]; then
  printf '%s\n' 'temporary Redis restore server did not become ready' >&2
  exit 1
fi

docker exec "$container" redis-cli CONFIG SET appendonly yes | grep -qx OK
aof_ready=0
for attempt in $(seq 1 30); do
  aof_enabled=$(docker exec "$container" redis-cli INFO persistence 2>/dev/null | sed -n 's/^aof_enabled://p' | tr -d '\r')
  aof_base_size=$(docker exec "$container" redis-cli INFO persistence 2>/dev/null | sed -n 's/^aof_base_size://p' | tr -d '\r')
  if [ "$aof_enabled" = '1' ] && [ "${aof_base_size:-0}" -gt 0 ] 2>/dev/null; then
    aof_ready=1
    break
  fi
  sleep 1
done
if [ "$aof_ready" -ne 1 ]; then
  printf '%s\n' 'Redis did not finish creating the AOF restore baseline' >&2
  exit 1
fi

printf '%s\n' "$volume"
