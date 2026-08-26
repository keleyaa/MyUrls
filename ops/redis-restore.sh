#!/bin/sh
set -eu

backup_file=${1:?usage: redis-restore.sh BACKUP_FILE NEW_VOLUME SAMPLE_CODE EXPECTED_URL [REDIS_IMAGE]}
volume=${2:?usage: redis-restore.sh BACKUP_FILE NEW_VOLUME SAMPLE_CODE EXPECTED_URL [REDIS_IMAGE]}
sample_code=${3:?usage: redis-restore.sh BACKUP_FILE NEW_VOLUME SAMPLE_CODE EXPECTED_URL [REDIS_IMAGE]}
expected_url=${4:?usage: redis-restore.sh BACKUP_FILE NEW_VOLUME SAMPLE_CODE EXPECTED_URL [REDIS_IMAGE]}
image=${5:-redis:8.10.0@sha256:c29e49ab2f85760a3827b53882e6dd9f5c6c3f0bb7d724e07bb31cbf275a5236}

case "$volume" in
  ''|*[!A-Za-z0-9_.-]*)
    printf '%s\n' 'invalid restore volume name' >&2
    exit 2
    ;;
esac
sample_length=${#sample_code}
case "$sample_code" in
  ''|*[!A-Za-z0-9_-]*)
    printf '%s\n' 'invalid sample code' >&2
    exit 2
    ;;
esac
if [ "$sample_length" -lt 4 ] || [ "$sample_length" -gt 32 ]; then
  printf '%s\n' 'invalid sample code length' >&2
  exit 2
fi
test -f "$backup_file"

if docker volume inspect "$volume" >/dev/null 2>&1; then
  printf '%s\n' 'restore volume already exists; choose a new volume name' >&2
  exit 2
fi

checksum_file="$backup_file.sha256"
if [ ! -f "$checksum_file" ]; then
  printf '%s\n' 'backup checksum sidecar is required' >&2
  exit 2
fi
checksum_dir=$(CDPATH= cd -- "$(dirname -- "$checksum_file")" && pwd)
checksum_name=$(basename -- "$checksum_file")
(CDPATH= cd -- "$checksum_dir" && shasum -a 256 -c "$checksum_name")

container="myurl-v2-restore-$$-$(date +%s)"
volume_created=0
cleanup() {
  status=$?
  docker rm -f "$container" >/dev/null 2>&1 || true
  if [ "$status" -ne 0 ] && [ "$volume_created" -eq 1 ]; then
    docker volume rm "$volume" >/dev/null 2>&1 || true
  fi
  exit "$status"
}
trap cleanup EXIT HUP INT TERM

docker volume create "$volume" >/dev/null
volume_created=1
backup_dir=$(CDPATH= cd -- "$(dirname -- "$backup_file")" && pwd)
backup_name=$(basename -- "$backup_file")
docker run --rm --user root \
  --mount "type=volume,source=$volume,target=/data" \
  --mount "type=bind,source=$backup_dir,target=/backup,readonly" \
  "$image" sh -ec "cp /backup/$backup_name /data/dump.rdb && chown redis:redis /data/dump.rdb"

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

sample_value=$(docker exec "$container" redis-cli --raw GET "myurl:link:$sample_code")
if [ "$sample_value" != "$expected_url" ]; then
  printf '%s\n' 'restored sample does not match the expected target' >&2
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
