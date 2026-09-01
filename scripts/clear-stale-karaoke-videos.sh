#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

# Deletes cached karaoke video files whose recorded version in
# karaoke_video_status (app-core/src/library_db/karaoke_video_status.rs) is
# nonzero but not the current karaoke_video::RENDER_VERSION, then zeroes
# that flavor's version column so the song list / library menu stop
# claiming a video exists that's actually gone. Safe to re-run:
# karaoke_video::is_fresh already treats a missing file as stale regardless
# of what the DB says, so a stale row with no matching file just gets a
# fresh render next time it's requested (cast, or the "Render karaoke
# video" bulk action) anyway -- this script only reclaims disk space
# early instead of waiting for that to happen lazily, song by song.

# Mirrors app-core/src/cache.rs's `nightingale_dir()` precedence exactly:
# config.json's own "data_path" field (settable from the Settings UI, e.g.
# to point at a bigger volume) wins over $NIGHTINGALE_DATA_PATH, which wins
# over the plain default -- config.json itself is always read from the
# env-var-or-default location, never the (possibly already-redirected)
# configured path, to avoid a chicken-and-egg lookup. Getting this wrong
# silently points the script at a different, stale songs.db than the one
# the running server actually migrated -- surfacing as a confusing
# "no such column" error instead of a "wrong file" one.
CONFIG_HOME="${NIGHTINGALE_DATA_PATH:-$HOME/.nightingale}"
CONFIG_JSON="$CONFIG_HOME/config.json"
CONFIGURED_DATA_PATH=""
if [ -f "$CONFIG_JSON" ]; then
  CONFIGURED_DATA_PATH=$(python3 -c "
import json
try:
    with open('$CONFIG_JSON') as f:
        print(json.load(f).get('data_path') or '')
except Exception:
    print('')
" 2>/dev/null)
fi
DATA_PATH="${CONFIGURED_DATA_PATH:-$CONFIG_HOME}"
DB="$DATA_PATH/songs.db"
VIDEOS_DIR="$DATA_PATH/cache/karaoke_videos"

# Parsed from source rather than hardcoded so this script can't silently
# drift out of sync the next time RENDER_VERSION bumps.
CURRENT_VERSION=$(grep -oE 'const RENDER_VERSION: u32 = [0-9]+' app-core/src/karaoke_video.rs | grep -oE '[0-9]+$')
if [ -z "$CURRENT_VERSION" ]; then
  echo "could not determine RENDER_VERSION from app-core/src/karaoke_video.rs" >&2
  exit 1
fi

if [ ! -f "$DB" ]; then
  echo "no songs.db found at $DB" >&2
  exit 1
fi

# A missing column here means this $DB predates the version-column
# migration (app-core/src/library_db/migrations.rs's
# ensure_karaoke_video_version_columns) -- which runs unconditionally on
# every server startup, so either the server hasn't been restarted since
# pulling current code, or (more likely if a restart was already done)
# this script resolved a different songs.db than the one the running
# server actually opened. Fail with that context instead of a bare sqlite3
# "no such column" error.
missing_columns=$(sqlite3 "$DB" "SELECT group_concat(name, ', ') FROM (
  SELECT 'karaoke_video_version' AS name
  WHERE NOT EXISTS (SELECT 1 FROM pragma_table_info('karaoke_video_status') WHERE name = 'karaoke_video_version')
  UNION ALL
  SELECT 'youtube_karaoke_video_version'
  WHERE NOT EXISTS (SELECT 1 FROM pragma_table_info('karaoke_video_status') WHERE name = 'youtube_karaoke_video_version')
);")
if [ -n "$missing_columns" ]; then
  echo "$DB is missing column(s): $missing_columns" >&2
  echo "This songs.db predates the karaoke_video_status version-column migration." >&2
  echo "Either the server hasn't been restarted on current code since pulling, or" >&2
  echo "this script resolved a different songs.db than the one the running server" >&2
  echo "actually uses -- double check the server's own data path (Settings, or its" >&2
  echo "config.json's \"data_path\" field) against: $DATA_PATH" >&2
  exit 1
fi

echo "Scanning for karaoke videos not at RENDER_VERSION $CURRENT_VERSION..."

deleted=0

# Reel-background videos: {hash}.mp4
while IFS='|' read -r hash version; do
  path="$VIDEOS_DIR/$hash.mp4"
  if [ -f "$path" ]; then
    echo "  deleting stale reel v$version: $path"
    rm -f "$path"
    deleted=$((deleted + 1))
  fi
  sqlite3 "$DB" "UPDATE karaoke_video_status SET karaoke_video_version = 0 WHERE file_hash = '$hash';"
done < <(sqlite3 "$DB" "SELECT file_hash, karaoke_video_version FROM karaoke_video_status WHERE karaoke_video_version != 0 AND karaoke_video_version != $CURRENT_VERSION;")

# YouTube-background videos: {hash}_youtube.mp4
while IFS='|' read -r hash version; do
  path="$VIDEOS_DIR/${hash}_youtube.mp4"
  if [ -f "$path" ]; then
    echo "  deleting stale youtube v$version: $path"
    rm -f "$path"
    deleted=$((deleted + 1))
  fi
  sqlite3 "$DB" "UPDATE karaoke_video_status SET youtube_karaoke_video_version = 0 WHERE file_hash = '$hash';"
done < <(sqlite3 "$DB" "SELECT file_hash, youtube_karaoke_video_version FROM karaoke_video_status WHERE youtube_karaoke_video_version != 0 AND youtube_karaoke_video_version != $CURRENT_VERSION;")

echo "Done. Deleted $deleted stale video file(s)."
