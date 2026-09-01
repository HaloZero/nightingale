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

DATA_PATH="${NIGHTINGALE_DATA_PATH:-$HOME/.nightingale}"
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
