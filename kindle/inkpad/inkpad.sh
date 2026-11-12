#!/bin/sh
# inkpad launcher: takes the screen from the stock Kindle UI, runs an inkpad
# binary, and ALWAYS gives the screen back, even if the binary crashes.
# Usage: inkpad.sh <binary-name> [args...]   (binary lives next to this script)
# The takeover steps mirror what KOReader's launcher does on firmware 5.x.

DIR=/mnt/us/inkpad
LOG="$DIR/inkpad.log"
BIN="$DIR/$1"
shift

# Keep the log small.
[ -f "$LOG" ] && [ "$(wc -c < "$LOG")" -gt 200000 ] && mv -f "$LOG" "$LOG.old"
exec >> "$LOG" 2>&1
echo "=== $(date) start $BIN $*"

PILLOW_DISABLED=no
AWESOME_STOPPED=no
VOLUMD_STOPPED=no

restore() {
    [ "$VOLUMD_STOPPED" = yes ] && killall -CONT volumd
    [ "$AWESOME_STOPPED" = yes ] && killall -CONT awesome
    if [ "$PILLOW_DISABLED" = yes ]; then
        lipc-set-prop com.lab126.pillow disableEnablePillow enable
    fi
    # Bring the stock home screen back so it repaints everything.
    lipc-set-prop com.lab126.appmgrd start app://com.lab126.booklet.home
    echo "=== $(date) restored"
}
trap restore EXIT
trap 'exit 1' INT TERM HUP

if [ ! -f "$BIN" ]; then
    echo "missing binary: $BIN"
    exit 1
fi
chmod +x "$BIN"

# Hide the stock status bar/dialogs, freeze the window manager so it can't
# draw over us, and pause volumd so USB storage can't pull the disk away.
lipc-set-prop com.lab126.pillow disableEnablePillow disable && PILLOW_DISABLED=yes
killall -STOP awesome && AWESOME_STOPPED=yes
killall -STOP volumd && VOLUMD_STOPPED=yes
usleep 300000

"$BIN" "$@"
echo "exit code $?"
