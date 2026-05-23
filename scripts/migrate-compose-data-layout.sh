#!/usr/bin/env bash
set -euo pipefail

MODE="auto"
COMPOSE_DIR=""
COMPOSE_FILE=""
PROJECT_NAME=""
STOP_SERVICES="false"
YES="false"
DRY_RUN="false"
FORCE="false"
COPY_IMAGE="${AETHER_MIGRATION_COPY_IMAGE:-alpine:3.20}"

APP_CONTAINER="${AETHER_APP_CONTAINER:-aether-app}"
POSTGRES_CONTAINER="${AETHER_POSTGRES_CONTAINER:-aether-postgres}"
REDIS_CONTAINER="${AETHER_REDIS_CONTAINER:-aether-redis}"
MYSQL_CONTAINER="${AETHER_MYSQL_CONTAINER:-aether-mysql}"

POSTGRES_VOLUME="${AETHER_POSTGRES_VOLUME:-}"
REDIS_VOLUME="${AETHER_REDIS_VOLUME:-}"
MYSQL_VOLUME="${AETHER_MYSQL_VOLUME:-}"

COMPOSE=()
COMPOSE_ARGS=()
PROJECT_CANDIDATES=()
SOURCE_COUNT=0
COPIED_COUNT=0

usage() {
  cat <<'EOF'
Usage: scripts/migrate-compose-data-layout.sh [options]

Move legacy Aether Docker Compose data into the current ./datas layout.

Old layouts:
  standard compose: Docker named volumes postgres_data, redis_data, mysql_data
  single-node:      ./data/aether.db

New layout:
  ./datas/postgres
  ./datas/redis
  ./datas/mysql
  ./datas/sqlite

Options:
  --mode MODE              auto, compose, single-node, or all (default: auto)
  --compose-dir DIR        deployment directory (default: current directory)
  -f, --compose-file FILE  compose file used when stopping services
  --project-name NAME      compose project name used to detect named volumes
  --postgres-volume NAME   explicit old Postgres Docker volume
  --redis-volume NAME      explicit old Redis Docker volume
  --mysql-volume NAME      explicit old MySQL Docker volume
  --copy-image IMAGE       helper image with sh and tar (default: alpine:3.20)
  --stop-services          stop/remove compose containers before copying
  --yes                    do not prompt before stopping services
  --force                  allow copying into non-empty target directories
  --dry-run                print the migration plan without changing anything
  -h, --help               show help

Examples:
  scripts/migrate-compose-data-layout.sh --dry-run
  scripts/migrate-compose-data-layout.sh --mode compose --stop-services
  scripts/migrate-compose-data-layout.sh --mode single-node --stop-services
EOF
}

log() {
  printf '>>> %s\n' "$*"
}

warn() {
  printf 'WARN: %s\n' "$*" >&2
}

die() {
  printf 'ERROR: %s\n' "$*" >&2
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      [[ $# -ge 2 ]] || die "--mode requires a value"
      MODE="$2"
      shift 2
      ;;
    --compose-dir)
      [[ $# -ge 2 ]] || die "--compose-dir requires a value"
      COMPOSE_DIR="$2"
      shift 2
      ;;
    -f|--compose-file)
      [[ $# -ge 2 ]] || die "--compose-file requires a value"
      COMPOSE_FILE="$2"
      shift 2
      ;;
    --project-name)
      [[ $# -ge 2 ]] || die "--project-name requires a value"
      PROJECT_NAME="$2"
      shift 2
      ;;
    --postgres-volume)
      [[ $# -ge 2 ]] || die "--postgres-volume requires a value"
      POSTGRES_VOLUME="$2"
      shift 2
      ;;
    --redis-volume)
      [[ $# -ge 2 ]] || die "--redis-volume requires a value"
      REDIS_VOLUME="$2"
      shift 2
      ;;
    --mysql-volume)
      [[ $# -ge 2 ]] || die "--mysql-volume requires a value"
      MYSQL_VOLUME="$2"
      shift 2
      ;;
    --copy-image)
      [[ $# -ge 2 ]] || die "--copy-image requires a value"
      COPY_IMAGE="$2"
      shift 2
      ;;
    --stop-services)
      STOP_SERVICES="true"
      shift
      ;;
    --yes|-y)
      YES="true"
      shift
      ;;
    --force)
      FORCE="true"
      shift
      ;;
    --dry-run)
      DRY_RUN="true"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die "unknown argument: $1"
      ;;
  esac
done

case "${MODE}" in
  auto|compose|single-node|all)
    ;;
  *)
    die "unsupported mode: ${MODE}; expected auto, compose, single-node, or all"
    ;;
esac

absolute_dir() {
  local path="$1"
  cd -- "${path}" && pwd -P
}

normalize_project_name() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]' | sed 's/[^a-z0-9_-]/_/g'
}

env_file_value() {
  local file="$1"
  local key="$2"

  [[ -f "${file}" ]] || return 0
  awk -v key="${key}" '
    {
      line = $0
      sub(/^[[:space:]]*/, "", line)
      if (line ~ /^#/ || line !~ /^[A-Za-z_][A-Za-z0-9_]*=/) {
        next
      }
      name = line
      sub(/=.*/, "", name)
      if (name == key) {
        value = line
        sub(/^[^=]*=/, "", value)
        gsub(/^[[:space:]]+|[[:space:]]+$/, "", value)
        gsub(/^"|"$/, "", value)
        gsub(/^'\''|'\''$/, "", value)
        print value
      }
    }
  ' "${file}" | tail -n1
}

append_unique_project_candidate() {
  local value="$1"
  local existing

  [[ -n "${value}" ]] || return 0
  if [[ "${#PROJECT_CANDIDATES[@]}" -gt 0 ]]; then
    for existing in "${PROJECT_CANDIDATES[@]}"; do
      [[ "${existing}" != "${value}" ]] || return 0
    done
  fi
  PROJECT_CANDIDATES+=("${value}")
}

resolve_compose_cli() {
  if [[ "${#COMPOSE[@]}" -gt 0 ]]; then
    return
  fi

  if docker compose version >/dev/null 2>&1; then
    COMPOSE=(docker compose)
    return
  fi

  if command -v docker-compose >/dev/null 2>&1; then
    COMPOSE=(docker-compose)
    return
  fi

  die "docker compose or docker-compose is required"
}

compose() {
  "${COMPOSE[@]}" "${COMPOSE_ARGS[@]}" "$@"
}

volume_exists() {
  local volume="$1"
  [[ -n "${volume}" ]] && docker volume inspect "${volume}" >/dev/null 2>&1
}

container_exists() {
  docker inspect "$1" >/dev/null 2>&1
}

container_running() {
  [[ "$(docker inspect -f '{{.State.Running}}' "$1" 2>/dev/null || true)" == "true" ]]
}

volume_from_container_mount() {
  local container="$1"
  local destination="$2"

  container_exists "${container}" || return 0
  docker inspect -f "{{range .Mounts}}{{if and (eq .Destination \"${destination}\") (eq .Type \"volume\")}}{{.Name}}{{end}}{{end}}" "${container}" 2>/dev/null || true
}

project_candidate_matches() {
  local project="$1"
  local candidate

  [[ -n "${project}" ]] || return 1
  if [[ "${#PROJECT_CANDIDATES[@]}" -gt 0 ]]; then
    for candidate in "${PROJECT_CANDIDATES[@]}"; do
      [[ "${project}" != "${candidate}" ]] || return 0
    done
  fi
  return 1
}

first_matching_labeled_volume() {
  local compose_volume="$1"
  local volume
  local label
  local project
  local matches=()
  local project_matches=()

  while IFS= read -r volume; do
    [[ -n "${volume}" ]] || continue
    label="$(docker volume inspect -f '{{ index .Labels "com.docker.compose.volume" }}' "${volume}" 2>/dev/null || true)"
    [[ "${label}" == "${compose_volume}" ]] || continue

    project="$(docker volume inspect -f '{{ index .Labels "com.docker.compose.project" }}' "${volume}" 2>/dev/null || true)"
    matches+=("${volume}")
    if project_candidate_matches "${project}"; then
      project_matches+=("${volume}")
    fi
  done < <(docker volume ls -q)

  if [[ "${#project_matches[@]}" -eq 1 ]]; then
    printf '%s\n' "${project_matches[0]}"
    return
  fi
  if [[ "${#project_matches[@]}" -gt 1 ]]; then
    die "multiple ${compose_volume} volumes match project candidates: ${project_matches[*]}; pass an explicit --${compose_volume%_data}-volume"
  fi

  if [[ "${#matches[@]}" -eq 1 ]]; then
    printf '%s\n' "${matches[0]}"
    return
  fi
  if [[ "${#matches[@]}" -gt 1 ]]; then
    die "multiple ${compose_volume} volumes found: ${matches[*]}; pass an explicit --${compose_volume%_data}-volume"
  fi

  return 0
}

first_matching_candidate_volume() {
  local compose_volume="$1"
  local project
  local candidate

  for project in "${PROJECT_CANDIDATES[@]}"; do
    candidate="${project}_${compose_volume}"
    if volume_exists "${candidate}"; then
      printf '%s\n' "${candidate}"
      return
    fi
  done

  if volume_exists "${compose_volume}"; then
    printf '%s\n' "${compose_volume}"
  fi

  return 0
}

detect_volume() {
  local compose_volume="$1"
  local explicit="$2"
  local container="$3"
  local destination="$4"
  local detected=""

  if [[ -n "${explicit}" ]]; then
    volume_exists "${explicit}" || die "Docker volume not found: ${explicit}"
    printf '%s\n' "${explicit}"
    return
  fi

  detected="$(volume_from_container_mount "${container}" "${destination}")"
  if [[ -n "${detected}" ]]; then
    printf '%s\n' "${detected}"
    return
  fi

  detected="$(first_matching_labeled_volume "${compose_volume}")"
  if [[ -n "${detected}" ]]; then
    printf '%s\n' "${detected}"
    return
  fi

  first_matching_candidate_volume "${compose_volume}"
}

directory_has_entries() {
  local dir="$1"
  [[ -d "${dir}" ]] || return 1
  [[ -n "$(find "${dir}" -mindepth 1 -maxdepth 1 -print -quit 2>/dev/null)" ]]
}

ensure_empty_or_force() {
  local dir="$1"
  local label="$2"

  if directory_has_entries "${dir}" && [[ "${FORCE}" != "true" ]]; then
    die "${label} target is not empty: ${dir}; inspect it or rerun with --force"
  fi
}

confirm_stop_services() {
  [[ "${STOP_SERVICES}" == "true" ]] || return 0
  [[ "${YES}" == "true" ]] && return

  if [[ ! -r /dev/tty ]]; then
    die "--stop-services requires --yes when no interactive terminal is available"
  fi

  cat >/dev/tty <<EOF

This migration must stop Aether Compose containers before copying database files.
Old Docker volumes and old ./data are kept for rollback.

Type "yes" to stop services and continue: 
EOF
  local answer
  IFS= read -r answer </dev/tty || answer=""
  [[ "${answer}" == "yes" ]] || die "aborted"
}

running_containers() {
  local container
  for container in "${APP_CONTAINER}" "${POSTGRES_CONTAINER}" "${REDIS_CONTAINER}" "${MYSQL_CONTAINER}"; do
    if container_running "${container}"; then
      printf '%s\n' "${container}"
    fi
  done
}

stop_services() {
  [[ "${STOP_SERVICES}" == "true" ]] || return 0

  log "stopping compose services"
  if [[ -n "${COMPOSE_FILE}" && -f "${COMPOSE_FILE}" ]]; then
    resolve_compose_cli
    compose down
    return
  fi

  local container
  for container in "${APP_CONTAINER}" "${POSTGRES_CONTAINER}" "${REDIS_CONTAINER}" "${MYSQL_CONTAINER}"; do
    if container_exists "${container}"; then
      docker rm -f "${container}" >/dev/null
    fi
  done
}

migrate_volume_to_dir() {
  local label="$1"
  local volume="$2"
  local target="$3"

  [[ -n "${volume}" ]] || return 0
  SOURCE_COUNT=$((SOURCE_COUNT + 1))
  ensure_empty_or_force "${target}" "${label}"

  log "${label}: ${volume} -> ${target}"
  if [[ "${DRY_RUN}" == "true" ]]; then
    return
  fi

  mkdir -p "${target}"
  docker run --rm \
    -e FORCE_COPY="${FORCE}" \
    -v "${volume}:/from:ro" \
    -v "${target}:/to" \
    "${COPY_IMAGE}" \
    sh -ec '
      if [ "$FORCE_COPY" != "true" ] && [ -n "$(ls -A /to 2>/dev/null)" ]; then
        echo "target directory is not empty: /to" >&2
        exit 1
      fi
      cd /from
      tar cf - . | (cd /to && tar xpf -)
    '
  COPIED_COUNT=$((COPIED_COUNT + 1))
}

migrate_local_dir() {
  local label="$1"
  local source="$2"
  local target="$3"

  [[ -d "${source}" ]] || return 0
  if ! directory_has_entries "${source}"; then
    warn "${label} source exists but is empty, skipping: ${source}"
    return
  fi

  SOURCE_COUNT=$((SOURCE_COUNT + 1))
  ensure_empty_or_force "${target}" "${label}"

  log "${label}: ${source} -> ${target}"
  if [[ "${DRY_RUN}" == "true" ]]; then
    return
  fi

  mkdir -p "${target}"
  cp -R "${source}/." "${target}/"
  COPIED_COUNT=$((COPIED_COUNT + 1))
}

docker info >/dev/null 2>&1 || die "Docker is not running"

if [[ -z "${COMPOSE_DIR}" ]]; then
  COMPOSE_DIR="$(pwd -P)"
fi
COMPOSE_DIR="$(absolute_dir "${COMPOSE_DIR}")"

if [[ -z "${COMPOSE_FILE}" ]]; then
  if [[ -f "${COMPOSE_DIR}/docker-compose.yml" ]]; then
    COMPOSE_FILE="${COMPOSE_DIR}/docker-compose.yml"
  elif [[ -f "${COMPOSE_DIR}/docker-compose.single-node.yml" ]]; then
    COMPOSE_FILE="${COMPOSE_DIR}/docker-compose.single-node.yml"
  fi
elif [[ "${COMPOSE_FILE}" != /* ]]; then
  COMPOSE_FILE="${COMPOSE_DIR}/${COMPOSE_FILE}"
fi

if [[ -n "${COMPOSE_FILE}" && -f "${COMPOSE_FILE}" ]]; then
  COMPOSE_ARGS+=(--project-directory "${COMPOSE_DIR}" -f "${COMPOSE_FILE}")
fi

append_unique_project_candidate "${PROJECT_NAME}"
append_unique_project_candidate "${COMPOSE_PROJECT_NAME:-}"
append_unique_project_candidate "$(env_file_value "${COMPOSE_DIR}/.env" "COMPOSE_PROJECT_NAME")"
append_unique_project_candidate "$(normalize_project_name "$(basename -- "${COMPOSE_DIR}")")"
append_unique_project_candidate "aether"

DATAS_DIR="${COMPOSE_DIR}/datas"
POSTGRES_TARGET="${DATAS_DIR}/postgres"
REDIS_TARGET="${DATAS_DIR}/redis"
MYSQL_TARGET="${DATAS_DIR}/mysql"
SQLITE_SOURCE="${COMPOSE_DIR}/data"
SQLITE_TARGET="${DATAS_DIR}/sqlite"

DETECTED_POSTGRES_VOLUME=""
DETECTED_REDIS_VOLUME=""
DETECTED_MYSQL_VOLUME=""
EFFECTIVE_MODE="${MODE}"

if [[ "${MODE}" == "auto" || "${MODE}" == "compose" || "${MODE}" == "all" ]]; then
  DETECTED_POSTGRES_VOLUME="$(detect_volume "postgres_data" "${POSTGRES_VOLUME}" "${POSTGRES_CONTAINER}" "/var/lib/postgresql/data")"
  DETECTED_REDIS_VOLUME="$(detect_volume "redis_data" "${REDIS_VOLUME}" "${REDIS_CONTAINER}" "/data")"
  DETECTED_MYSQL_VOLUME="$(detect_volume "mysql_data" "${MYSQL_VOLUME}" "${MYSQL_CONTAINER}" "/var/lib/mysql")"
fi

if [[ "${MODE}" == "auto" ]]; then
  if [[ -n "${DETECTED_POSTGRES_VOLUME}" || -n "${DETECTED_REDIS_VOLUME}" || -n "${DETECTED_MYSQL_VOLUME}" ]]; then
    EFFECTIVE_MODE="compose"
  elif [[ -d "${SQLITE_SOURCE}" ]]; then
    EFFECTIVE_MODE="single-node"
  else
    EFFECTIVE_MODE="auto"
  fi
fi

log "compose directory: ${COMPOSE_DIR}"
if [[ -n "${COMPOSE_FILE}" ]]; then
  log "compose file: ${COMPOSE_FILE}"
fi
log "migration mode: ${EFFECTIVE_MODE}"
log "helper image: ${COPY_IMAGE}"

case "${EFFECTIVE_MODE}" in
  compose)
    [[ -n "${DETECTED_POSTGRES_VOLUME}" ]] || warn "Postgres legacy volume not found"
    [[ -n "${DETECTED_REDIS_VOLUME}" ]] || warn "Redis legacy volume not found"
    [[ -n "${DETECTED_MYSQL_VOLUME}" ]] || warn "MySQL legacy volume not found; this is normal if the mysql profile was never used"
    ;;
  single-node)
    [[ -d "${SQLITE_SOURCE}" ]] || warn "single-node legacy ./data directory not found"
    ;;
  all)
    ;;
  auto)
    ;;
esac

if [[ "${DRY_RUN}" == "true" ]]; then
  log "dry run only; no services will be stopped and no files will be copied"
fi

if [[ "${DRY_RUN}" != "true" ]]; then
  RUNNING="$(running_containers)"
  if [[ -n "${RUNNING}" && "${STOP_SERVICES}" != "true" ]]; then
    printf '%s\n' "${RUNNING}" >&2
    die "containers are still running; stop them first or rerun with --stop-services"
  fi
  confirm_stop_services
  stop_services
fi

if [[ "${DRY_RUN}" != "true" ]]; then
  mkdir -p "${DATAS_DIR}"
fi

if [[ "${EFFECTIVE_MODE}" == "compose" || "${EFFECTIVE_MODE}" == "all" ]]; then
  migrate_volume_to_dir "postgres" "${DETECTED_POSTGRES_VOLUME}" "${POSTGRES_TARGET}"
  migrate_volume_to_dir "redis" "${DETECTED_REDIS_VOLUME}" "${REDIS_TARGET}"
  migrate_volume_to_dir "mysql" "${DETECTED_MYSQL_VOLUME}" "${MYSQL_TARGET}"
fi

if [[ "${EFFECTIVE_MODE}" == "single-node" || "${EFFECTIVE_MODE}" == "all" ]]; then
  migrate_local_dir "sqlite" "${SQLITE_SOURCE}" "${SQLITE_TARGET}"
fi

if [[ "${SOURCE_COUNT}" -eq 0 ]]; then
  if directory_has_entries "${POSTGRES_TARGET}" || directory_has_entries "${SQLITE_TARGET}"; then
    log "no legacy source found; target ./datas layout already has data"
    exit 0
  fi
  die "no legacy data source found to migrate"
fi

if [[ "${DRY_RUN}" == "true" ]]; then
  log "dry run complete"
  exit 0
fi

log "migration complete: copied ${COPIED_COUNT} source(s)"
cat <<EOF

Old Docker volumes and old ./data were not deleted.

Next steps:
  1. Make sure docker-compose.yml uses ./datas/* bind mounts.
  2. Start services:
       cd "${COMPOSE_DIR}"
       docker compose up -d
  3. After verifying the app, keep old volumes for a while before deleting them manually.
EOF
