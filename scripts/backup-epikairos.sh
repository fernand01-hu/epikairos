#!/bin/sh
# Nightly backup of the EpiKairos databases, keeping the last 14 days.
# Uses SQLite's online backup API, so the copy is consistent while the site runs.
#
#   APP_DIR     where school.db and feeds.db live   (default: ~/epikairos)
#   BACKUP_DIR  where the copies go                 (default: ~/backups/daily)
#   KEEP_DAYS   how many days to keep               (default: 14)
#
# Cron example (03:30 every night):
#   30 3 * * * $HOME/.local/bin/backup-epikairos.sh >> $HOME/.local/state/epikairos-backup.log 2>&1
set -eu

APP_DIR="${APP_DIR:-$HOME/epikairos}"
BACKUP_DIR="${BACKUP_DIR:-$HOME/backups/daily}"
KEEP_DAYS="${KEEP_DAYS:-14}"
STAMP="$(date +%Y%m%d_%H%M)"

mkdir -p "$BACKUP_DIR"
chmod 700 "$BACKUP_DIR"

for name in school feeds; do
    src="$APP_DIR/$name.db"
    dst="$BACKUP_DIR/${name}_$STAMP.db"
    python3 - "$src" "$dst" <<'PY'
import sqlite3, sys
src, dst = sys.argv[1], sys.argv[2]
s = sqlite3.connect(f"file:{src}?mode=ro", uri=True)
d = sqlite3.connect(dst)
s.backup(d)
d.execute("PRAGMA journal_mode=DELETE")   # one self-contained file, no -wal/-shm
ok = d.execute("PRAGMA integrity_check").fetchone()[0]
d.close(); s.close()
if ok != "ok":
    sys.exit(f"integrity check failed for {dst}: {ok}")
PY
    chmod 600 "$dst"
    echo "$(date '+%F %T') saved $dst ($(du -h "$dst" | cut -f1))"
done

# Drop copies older than KEEP_DAYS
find "$BACKUP_DIR" -name '*.db' -type f -mtime +"$KEEP_DAYS" -print -delete
